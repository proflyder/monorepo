"""Hermetic checks for the bb-dispatch SSH bridge (no CT, no live bb).

The bridge is exercised as a real process: stdin carries the request and a
local throw-away HTTP server plays the home-dispatcher plugin, so the test sees
the exact headers and body that the bridge would send to the loopback route.
"""
import hashlib
import hmac
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ROOT = Path(__file__).with_name("bb")
BRIDGE = ROOT / "bb-dispatch"
HELPER = ROOT / "authorized-key.sh"
INSTALL = ROOT / "install-bb.sh"


def load_bridge_module():
    """Import the extension-less bridge to test its signing primitive."""
    spec = importlib.util.spec_from_loader(
        "bb_dispatch", importlib.machinery.SourceFileLoader("bb_dispatch", str(BRIDGE))
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

BODY_LIMIT = 16 * 1024
TOKEN = "test-plugin-token"
SECRET = "test-hmac-secret"
PUBKEY_BODY = "AAAAC3NzaC1lZDI1NTE5AAAAIEtest+dispatch+key+body"


class Recorder(BaseHTTPRequestHandler):
    """Minimal plugin double: records the request and answers from `plan`."""

    protocol_version = "HTTP/1.1"
    plan = {"status": 200, "body": b'{"ok":true,"result":{},"speech":"ok"}'}
    delay = 0.0
    requests = []

    def do_POST(self):  # noqa: N802 - http.server API
        length = int(self.headers.get("content-length") or "0")
        body = self.rfile.read(length)
        type(self).requests.append(
            {
                "path": self.path,
                "headers": {key.lower(): value for key, value in self.headers.items()},
                "body": body,
            }
        )
        if self.delay:
            time.sleep(self.delay)
        try:
            self.send_response(self.plan["status"])
            self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(self.plan["body"])))
            self.end_headers()
            self.wfile.write(self.plan["body"])
        except (BrokenPipeError, ConnectionResetError):
            pass

    def log_message(self, *args):  # keep the test output clean
        return


class BridgeTest(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.config = self.tmp / "config"
        self.config.mkdir(mode=0o700)
        self.write_secret("token", TOKEN)
        self.write_secret("hmac", SECRET)
        Recorder.plan = {
            "status": 200,
            "body": b'{"ok":true,"result":{},"speech":"ok"}',
        }
        Recorder.delay = 0.0
        Recorder.requests = []
        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Recorder)
        self.server.daemon_threads = True
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.addCleanup(self.server.server_close)
        self.addCleanup(self.server.shutdown)
        self.base_url = f"http://127.0.0.1:{self.server.server_address[1]}"

    def write_secret(self, name, value):
        path = self.config / name
        path.write_text(value + "\n", encoding="utf-8")
        path.chmod(0o600)

    def run_bridge(self, body: bytes, env: dict[str, str] | None = None):
        environment = {
            key: value
            for key, value in os.environ.items()
            if not key.startswith("BB_DISPATCH_") and key != "SSH_ORIGINAL_COMMAND"
        }
        environment["BB_DISPATCH_CONFIG_DIR"] = str(self.config)
        environment["BB_DISPATCH_URL"] = self.base_url
        if env:
            environment.update(env)
        return subprocess.run(
            [sys.executable, str(BRIDGE)],
            input=body,
            check=False,
            capture_output=True,
            env=environment,
            timeout=60,
        )

    def assert_no_secrets(self, result):
        output = result.stdout + result.stderr
        self.assertNotIn(TOKEN.encode(), output)
        self.assertNotIn(SECRET.encode(), output)

    # ----- happy path and signature ---------------------------------------

    def test_forwards_one_request_with_plugin_signature(self):
        body = b'{"method":"targets","params":{}}'
        result = self.run_bridge(body)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, b'{"ok":true,"result":{},"speech":"ok"}\n')
        self.assertEqual(len(Recorder.requests), 1)
        captured = Recorder.requests[0]
        self.assertEqual(captured["path"], "/api/v1/plugins/home-dispatcher/http/rpc")
        self.assertEqual(captured["body"], body)
        headers = captured["headers"]
        self.assertEqual(headers["content-type"], "application/json")
        self.assertEqual(headers["x-bb-plugin-token"], TOKEN)
        timestamp = headers["x-bb-dispatch-timestamp"]
        self.assertRegex(timestamp, r"^\d{10}$")
        expected = hmac.new(
            SECRET.encode(), timestamp.encode() + b"\n" + body, hashlib.sha256
        ).hexdigest()
        self.assertEqual(headers["x-bb-dispatch-signature"], expected)
        self.assert_no_secrets(result)

    def test_signature_matches_the_plugin_vector(self):
        """Fixed vectors produced by solutions/bb-home-dispatcher/lib/hmac.js.

        The second body contains Cyrillic text, so signing a re-encoded or
        ASCII-escaped body instead of the exact request bytes fails here.
        """
        bridge = load_bridge_module()
        vectors = [
            (
                "test-secret",
                "1700000000",
                b'{"method":"targets","params":{}}',
                "c23477170d767793a508b9a22802e51d0253a585fc11f130ea5a3e4ad331d66b",
            ),
            (
                "another-secret",
                "1800000000",
                '{"method":"send","params":{"threadId":"thr_test",'
                '"text":"привет","idempotencyKey":"k1"}}'.encode("utf-8"),
                "88636def0f6b737931c6e346d259a5ea61c23284ec87f117c08b347edaa557cf",
            ),
        ]
        for secret, timestamp, body, expected in vectors:
            with self.subTest(secret=secret):
                self.assertEqual(bridge.sign(secret, timestamp, body), expected)

    def test_cyrillic_body_is_signed_as_sent(self):
        body = json.dumps(
            {
                "method": "send",
                "params": {"threadId": "thr_test", "text": "привет"},
            },
            ensure_ascii=False,
        ).encode("utf-8")
        result = self.run_bridge(body)
        self.assertEqual(result.returncode, 0, result.stderr)
        captured = Recorder.requests[0]
        self.assertEqual(captured["body"], body)
        timestamp = captured["headers"]["x-bb-dispatch-timestamp"]
        expected = hmac.new(
            SECRET.encode(), timestamp.encode() + b"\n" + body, hashlib.sha256
        ).hexdigest()
        self.assertEqual(captured["headers"]["x-bb-dispatch-signature"], expected)
        self.assert_no_secrets(result)

    def test_env_file_supplies_url_and_timeout(self):
        self.config.joinpath("env").write_text(
            f"# bridge settings\nBB_DISPATCH_URL='{self.base_url}'\nBB_DISPATCH_TIMEOUT=5\n",
            encoding="utf-8",
        )
        result = self.run_bridge(
            b'{"method":"status","params":{}}',
            {"BB_DISPATCH_URL": ""},
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(Recorder.requests), 1)

    # ----- plugin answers are passed through ------------------------------

    def test_plugin_refusal_is_passed_through_with_exit_zero(self):
        answer = b'{"ok":false,"code":"invalid_signature","speech":"\xd0\x9d\xd0\xb5"}'
        Recorder.plan = {"status": 401, "body": answer}
        result = self.run_bridge(b'{"method":"targets","params":{}}')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout), json.loads(answer))
        self.assert_no_secrets(result)

    def test_http_error_without_a_plugin_answer_is_a_bridge_error(self):
        Recorder.plan = {"status": 502, "body": b"<html>bad gateway</html>"}
        result = self.run_bridge(b'{"method":"targets","params":{}}')
        self.assertEqual(result.returncode, 1)
        self.assertEqual(
            json.loads(result.stdout),
            {
                "ok": False,
                "code": "bridge_error",
                "speech": "Сервер агентов недоступен.",
            },
        )
        self.assert_no_secrets(result)

    # ----- transport failures ---------------------------------------------

    def test_connection_refused_is_a_bridge_error(self):
        port = self.server.server_address[1]
        self.server.shutdown()
        self.server.server_close()
        result = self.run_bridge(
            b'{"method":"targets","params":{}}',
            {"BB_DISPATCH_URL": f"http://127.0.0.1:{port}"},
        )
        self.assertEqual(result.returncode, 1)
        payload = json.loads(result.stdout)
        self.assertFalse(payload["ok"])
        self.assertEqual(payload["code"], "bridge_error")
        self.assert_no_secrets(result)

    def test_timeout_is_a_bridge_error(self):
        Recorder.delay = 3.0
        result = self.run_bridge(
            b'{"method":"targets","params":{}}',
            {"BB_DISPATCH_TIMEOUT": "1"},
        )
        self.assertEqual(result.returncode, 1)
        self.assertEqual(json.loads(result.stdout)["code"], "bridge_error")

    # ----- local refusals never reach bb ----------------------------------

    def test_request_over_the_body_limit_is_refused(self):
        body = b'{"method":"targets","params":{}}' + b" " * BODY_LIMIT
        self.assertGreater(len(body), BODY_LIMIT)
        result = self.run_bridge(body)
        self.assertEqual(result.returncode, 2)
        payload = json.loads(result.stdout)
        self.assertEqual(payload["code"], "body_too_large")
        self.assertEqual(Recorder.requests, [])

    def test_request_exactly_at_the_body_limit_is_forwarded(self):
        prefix = b'{"method":"targets","params":{}}'
        body = prefix + b" " * (BODY_LIMIT - len(prefix))
        self.assertEqual(len(body), BODY_LIMIT)
        result = self.run_bridge(body)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(Recorder.requests), 1)

    def test_unknown_method_is_refused_locally(self):
        result = self.run_bridge(b'{"method":"delete_everything","params":{}}')
        self.assertEqual(result.returncode, 2)
        self.assertEqual(json.loads(result.stdout)["code"], "unknown_method")
        self.assertEqual(Recorder.requests, [])

    def test_malformed_input_is_refused_locally(self):
        cases = {
            b"not json": "invalid_request",
            b"[]": "invalid_request",
            b"{}": "unknown_method",
            b'{"method":5}': "unknown_method",
            b'{"method":"targets","params":[]}': "invalid_request",
            b"\xff\xfe": "invalid_request",
            b"": "invalid_request",
        }
        for body, code in cases.items():
            with self.subTest(body=body):
                result = self.run_bridge(body)
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertEqual(json.loads(result.stdout)["code"], code)
        self.assertEqual(Recorder.requests, [])

    def test_minimal_environment_keeps_utf8_output(self):
        """sshd starts the forced command without a locale or HOME."""
        answer = '{"ok":false,"code":"invalid_request","speech":"Некорректный запрос."}'
        Recorder.plan = {"status": 400, "body": answer.encode("utf-8")}
        result = subprocess.run(
            [sys.executable, str(BRIDGE)],
            input=b'{"method":"targets","params":{}}',
            check=False,
            capture_output=True,
            env={
                "PATH": "/usr/bin:/bin",
                "LANG": "C",
                "LC_ALL": "C",
                "BB_DISPATCH_CONFIG_DIR": str(self.config),
                "BB_DISPATCH_URL": self.base_url,
            },
            timeout=60,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.decode("utf-8"), answer + "\n")

    # ----- configuration ---------------------------------------------------

    def test_missing_credentials_are_not_configured(self):
        for name in ("token", "hmac"):
            with self.subTest(name=name):
                path = self.config / name
                original = path.read_bytes()
                path.unlink()
                result = self.run_bridge(b'{"method":"targets","params":{}}')
                path.write_bytes(original)
                path.chmod(0o600)
                self.assertEqual(result.returncode, 3)
                self.assertEqual(json.loads(result.stdout)["code"], "not_configured")
        self.assertEqual(Recorder.requests, [])

    def test_world_readable_secret_is_refused(self):
        (self.config / "hmac").chmod(0o644)
        result = self.run_bridge(b'{"method":"targets","params":{}}')
        self.assertEqual(result.returncode, 3)
        self.assertEqual(json.loads(result.stdout)["code"], "not_configured")
        self.assert_no_secrets(result)
        self.assertEqual(Recorder.requests, [])

    def test_non_loopback_url_is_refused(self):
        result = self.run_bridge(
            b'{"method":"targets","params":{}}',
            {"BB_DISPATCH_URL": "http://192.0.2.30:38886"},
        )
        self.assertEqual(result.returncode, 3)
        self.assertEqual(json.loads(result.stdout)["code"], "not_configured")
        self.assertEqual(Recorder.requests, [])

    def test_ssh_original_command_is_ignored(self):
        result = self.run_bridge(
            b'{"method":"status","params":{"limit":1}}',
            {"SSH_ORIGINAL_COMMAND": "rm -rf /; bb plugin list"},
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(Recorder.requests[0]["body"], b'{"method":"status","params":{"limit":1}}')


class AuthorizedKeyTest(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.key = self.tmp / "dispatch.pub"
        self.key.write_text(
            f"ssh-ed25519 {PUBKEY_BODY} dispatch@k3s\n", encoding="utf-8"
        )
        self.target = self.tmp / "home" / ".ssh" / "authorized_keys"

    def install(self, *extra):
        script = (
            f'set -euo pipefail; source "{HELPER}"; '
            f'install_authorized_key "$1" "" "$2" "$3" /usr/local/bin/bb-dispatch'
        )
        return subprocess.run(
            ["bash", "-c", script, "bash", str(self.target), str(self.key),
             "192.0.2.27", *extra],
            check=False,
            capture_output=True,
            text=True,
        )

    def test_line_is_restricted_and_idempotent(self):
        first = self.install()
        self.assertEqual(first.returncode, 0, first.stderr)
        line = self.target.read_text(encoding="utf-8")
        self.assertIn('from="192.0.2.27"', line)
        self.assertIn('command="/usr/local/bin/bb-dispatch"', line)
        self.assertIn(",restrict ", line)
        self.assertIn(PUBKEY_BODY, line)
        self.assertNotIn("no-pty", line)
        self.assertEqual(len(line.splitlines()), 1)
        self.assertEqual(self.target.stat().st_mode & 0o777, 0o600)
        self.assertEqual(self.target.parent.stat().st_mode & 0o777, 0o700)

        second = self.install()
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertIn("already present", second.stdout)
        self.assertEqual(len(self.target.read_text().splitlines()), 1)

    def test_second_key_is_appended_next_to_the_first(self):
        self.install()
        other = self.tmp / "other.pub"
        other.write_text(
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEsecond+key+body offsite@k3s\n",
            encoding="utf-8",
        )
        script = (
            f'set -euo pipefail; source "{HELPER}"; '
            'install_authorized_key "$1" "" "$2" "$3" /usr/local/bin/bb-dispatch'
        )
        result = subprocess.run(
            ["bash", "-c", script, "bash", str(self.target), str(other), "192.0.2.27"],
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(self.target.read_text().splitlines()), 2)

    def test_unsafe_key_or_source_is_rejected(self):
        multiline = self.tmp / "multi.pub"
        multiline.write_text(
            f"ssh-ed25519 {PUBKEY_BODY} a\nssh-ed25519 {PUBKEY_BODY} b\n",
            encoding="utf-8",
        )
        quoted = self.tmp / "quoted.pub"
        quoted.write_text(f"ssh-ed25519 {PUBKEY_BODY} a\n", encoding="utf-8")
        for keyfile, source in ((multiline, "192.0.2.27"), (quoted, '1"2')):
            with self.subTest(keyfile=keyfile.name):
                script = (
                    f'set -euo pipefail; source "{HELPER}"; '
                    'install_authorized_key "$1" "" "$2" "$3" /usr/local/bin/bb-dispatch'
                )
                result = subprocess.run(
                    ["bash", "-c", script, "bash", str(self.target),
                     str(keyfile), source],
                    check=False,
                    capture_output=True,
                    text=True,
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(self.target.exists())


class InstallScriptTest(unittest.TestCase):
    def test_help_documents_the_dispatch_options(self):
        result = subprocess.run(
            ["bash", str(INSTALL), "--help"],
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("--dispatch-pubkey", result.stdout)
        self.assertIn("--dispatch-from", result.stdout)

    def test_dispatch_options_must_come_together(self):
        result = subprocess.run(
            ["bash", str(INSTALL), "--dispatch-from", "192.0.2.27"],
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("must be used together", result.stderr)

    def test_bridge_is_executable_with_a_pinned_interpreter(self):
        self.assertTrue(os.access(BRIDGE, os.R_OK))
        self.assertTrue(BRIDGE.stat().st_mode & 0o111, "bb-dispatch must be executable")
        first = BRIDGE.read_text(encoding="utf-8").splitlines()[0]
        self.assertEqual(first, "#!/usr/bin/python3")
        compiled = subprocess.run(
            [sys.executable, "-m", "py_compile", str(BRIDGE)],
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(compiled.returncode, 0, compiled.stderr)


if __name__ == "__main__":
    unittest.main()
