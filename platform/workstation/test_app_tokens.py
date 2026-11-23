import io
import json
import unittest
from contextlib import redirect_stdout
from unittest import mock

import app_tokens
import workstation as ws


class FakeBao:
    def __init__(self, values):
        self.values = values
        self.calls = []

    def call(self, method, path, body=None, missing_ok=False):
        self.calls.append((method, path, body))
        if method == "GET":
            return {
                "data": {
                    "data": dict(self.values),
                    "metadata": {"version": 7},
                }
            }
        if method == "POST":
            assert body["options"]["cas"] == 7
            self.values = dict(body["data"])
            return {}
        raise AssertionError((method, path))


class ModuleCatalogTests(unittest.TestCase):
    def test_python_and_rust_module_lists_match(self):
        import pathlib
        import re

        root = pathlib.Path(__file__).resolve().parents[2]
        rust = (root / "packages/rust/home-app-auth/src/lib.rs").read_text()
        modules = set(re.findall(r'Self::\w+ => "([a-z_]+)"', rust))
        self.assertEqual(modules, set(app_tokens.MODULES))


class PreflightTests(unittest.TestCase):
    def test_legacy_value_is_found_everywhere_except_scheduler_auth(self):
        import base64

        legacy = "legacy-token"
        secrets_doc = {
            "items": [
                {
                    "metadata": {"name": "scheduler-auth"},
                    "data": {"token": base64.b64encode(legacy.encode()).decode()},
                },
                {
                    "metadata": {"name": "runtime-values"},
                    "data": {
                        "values": base64.b64encode(
                            f"env: {{HERMES_SCHEDULER_TOKEN: {legacy}}}".encode()
                        ).decode()
                    },
                },
            ]
        }
        configmaps = {"items": [{"metadata": {"name": "ha-config"}, "data": {"token": legacy}}]}
        found = app_tokens.legacy_consumers(legacy, secrets_doc, configmaps)
        self.assertEqual(
            found,
            ["ConfigMap/ha-config:token", "Secret/runtime-values:values"],
        )

    def test_preflight_stops_before_issue(self):
        import base64

        values = {"token": "legacy-token-value", "service_token": "old"}
        bao = FakeBao(values)
        stdout = json.dumps(
            {
                "items": [
                    {
                        "metadata": {"name": "runtime-values"},
                        "data": {"values": base64.b64encode(b"legacy-token-value").decode()},
                    }
                ]
            }
        )
        output = io.StringIO()
        with mock.patch.object(ws, "operator", return_value=bao), mock.patch.object(
            app_tokens.subprocess,
            "run",
            return_value=mock.Mock(stdout=stdout, returncode=0),
        ), redirect_stdout(output):
            with self.assertRaises(SystemExit) as error:
                app_tokens.main([])
        self.assertIn("runtime-values", str(error.exception))
        self.assertEqual([call[0] for call in bao.calls], ["GET"])


class IssueTests(unittest.TestCase):
    def test_issue_stores_only_the_hash_and_keeps_the_other_fields(self):
        bao = FakeBao({"token": "mama", "service_token": "old", "vpn.json": "{}"})
        values = app_tokens.issue(bao, app_tokens.PATH, "owner", "home_secret")
        self.assertEqual(values["token"], "mama")
        self.assertEqual(values["vpn.json"], "{}")
        self.assertNotIn("home_secret", values["app_tokens"])
        seed = json.loads(values["app_tokens"])
        entry = seed["tokens"][0]
        self.assertEqual(entry["sha256"], app_tokens.digest("home_secret"))
        self.assertTrue(entry["owner"])
        self.assertIn("admin", entry["modules"])
        self.assertNotEqual(values["service_token"], "mama")
        self.assertEqual(values["service_token_previous"], "old")
        self.assertEqual(bao.calls[0][0], "GET")
        self.assertEqual(bao.calls[1][0], "POST")

    def test_issue_replaces_the_same_name_and_starts_empty(self):
        first = app_tokens.merge_token({"tokens": []}, "owner", "one")
        second = app_tokens.merge_token(first, "owner", "two")
        self.assertEqual(len(second["tokens"]), 1)
        self.assertEqual(second["tokens"][0]["sha256"], app_tokens.digest("two"))

    def test_init_creates_the_service_token_once_and_never_prints(self):
        bao = FakeBao({"token": "mama"})
        self.assertTrue(app_tokens.init_service_token(bao, app_tokens.PATH))
        created = bao.values["service_token"]
        self.assertTrue(created)
        self.assertEqual(bao.values["service_token_previous"], "")
        self.assertFalse(app_tokens.init_service_token(bao, app_tokens.PATH))
        self.assertEqual(bao.values["service_token"], created)

    def test_finalize_removes_the_previous_service_token(self):
        bao = FakeBao({"token": "mama", "service_token": "new", "service_token_previous": "old"})
        values = app_tokens.finalize(bao, app_tokens.PATH)
        self.assertNotIn("service_token_previous", values)
        self.assertEqual(values["service_token"], "new")
        self.assertEqual(bao.calls[1][2]["options"]["cas"], 7)

    def test_dry_run_never_writes_or_prints_a_token(self):
        bao = FakeBao({"token": "mama", "service_token": "previous-service-token"})
        app_tokens.issue(bao, app_tokens.PATH, "owner", "hidden", dry_run=True)
        self.assertEqual([call[0] for call in bao.calls], ["GET"])
        with mock.patch.object(ws, "operator", return_value=bao), mock.patch.object(
            app_tokens, "preflight", return_value=[]
        ):
            output = io.StringIO()
            with redirect_stdout(output):
                app_tokens.main(["--dry-run", "--token-file", "/dev/null"])
        self.assertNotIn("hidden", output.getvalue())
        self.assertIn("dry run", output.getvalue())

    def test_main_writes_the_owner_token_to_a_private_file_once(self):
        import pathlib
        import stat
        import tempfile

        bao = FakeBao({"token": "mama", "service_token": "previous-service-token"})
        with tempfile.TemporaryDirectory() as directory:
            token_file = pathlib.Path(directory) / "owner.token"
            with mock.patch.object(ws, "operator", return_value=bao), mock.patch.object(
                app_tokens, "preflight", return_value=[]
            ):
                output = io.StringIO()
                with redirect_stdout(output):
                    app_tokens.main(["--token-file", str(token_file)])
            text = output.getvalue()
            token = token_file.read_text().strip()
            self.assertTrue(token.startswith("home_"))
            self.assertEqual(stat.S_IMODE(token_file.stat().st_mode), 0o600)
            seed = json.loads(bao.values["app_tokens"])
            self.assertEqual(seed["tokens"][0]["sha256"], app_tokens.digest(token))
            self.assertNotIn(token, text)
            self.assertNotIn(bao.values["service_token"], text)
            self.assertNotIn(bao.values["service_token_previous"], text)
            self.assertIn(str(token_file), text)


if __name__ == "__main__":
    unittest.main()
