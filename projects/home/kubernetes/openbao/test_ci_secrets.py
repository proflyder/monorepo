"""Contracts for the OpenBao -> deploy-home secret hand-off.

The workflow must keep values out of the log, materialise byte-identical files
with mode 600 and fail closed when a key is missing. These tests exercise the
pure helper that performs the write, so no OpenBao or VPN is needed.
"""
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(DIR))

import ci_secrets  # noqa: E402


def kv_response(**values):
    return {"data": {"data": values, "metadata": {"version": 1}}}


KUBECONFIG = (
    "apiVersion: v1\n"
    "kind: Config\n"
    "clusters:\n"
    "- cluster:\n"
    "    server: https://192.0.2.27:6443\n"
    "  name: home\n"
)
SCHEDULER_ENV = "HA_URL=http://192.0.2.10:8123\nTZ=Europe/Berlin\n"
RUNTIME_VALUES = "hermes:\n  image: ghcr.io/example/hermes@sha256:abc\n"



def key_args(*pairs):
    """`--key field=file` arguments for the CLI under test."""
    args = []
    for field, filename in pairs:
        args += ["--key", f"{field}={filename}"]
    return args

class Payload(unittest.TestCase):
    def test_accepts_full_kv_v2_response_and_inner_mapping(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "payload.json"
            path.write_text(json.dumps(kv_response(kubeconfig=KUBECONFIG)))
            self.assertEqual(ci_secrets.load_payload(path)["kubeconfig"], KUBECONFIG)

            path.write_text(json.dumps({"kubeconfig": KUBECONFIG}))
            self.assertEqual(ci_secrets.load_payload(path)["kubeconfig"], KUBECONFIG)

    def test_rejects_payload_without_data(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "payload.json"
            path.write_text(json.dumps({"errors": ["permission denied"]}))
            with self.assertRaises(ValueError):
                ci_secrets.load_payload(path)


class Materialise(unittest.TestCase):
    def test_writes_exact_bytes_with_private_modes(self):
        payload = {
            "kubeconfig": KUBECONFIG,
            "scheduler_env": SCHEDULER_ENV,
            "runtime_values": RUNTIME_VALUES,
        }
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / "ci"
            written = ci_secrets.materialise(
                payload, target,
                [("kubeconfig", "kubeconfig"),
                 ("scheduler_env", "scheduler.env"),
                 ("runtime_values", "runtime-values.yaml")],
            )
            self.assertEqual(written, ["kubeconfig", "scheduler.env", "runtime-values.yaml"])
            self.assertEqual((target / "kubeconfig").read_bytes(),
                             KUBECONFIG.encode())
            self.assertEqual((target / "scheduler.env").read_bytes(),
                             SCHEDULER_ENV.encode())
            self.assertEqual((target / "runtime-values.yaml").read_bytes(),
                             RUNTIME_VALUES.encode())
            self.assertEqual(target.stat().st_mode & 0o777, 0o700)
            for name in written:
                self.assertEqual((target / name).stat().st_mode & 0o777, 0o600)

    def test_missing_or_empty_key_fails_closed(self):
        with tempfile.TemporaryDirectory() as tmp:
            for payload in ({"kubeconfig": KUBECONFIG},
                            {"kubeconfig": KUBECONFIG, "scheduler_env": ""}):
                with self.subTest(keys=sorted(payload)):
                    with self.assertRaises(ValueError):
                        ci_secrets.materialise(
                            payload, Path(tmp) / "ci",
                            [("kubeconfig", "kubeconfig"),
                             ("scheduler_env", "scheduler.env")],
                        )
            self.assertFalse((Path(tmp) / "ci" / "kubeconfig").exists())

    def test_rejects_path_traversal(self):
        with tempfile.TemporaryDirectory() as tmp:
            for name in ("../escape", "/etc/passwd", "nested/file"):
                with self.subTest(name=name):
                    with self.assertRaises(ValueError):
                        ci_secrets.materialise(
                            {"kubeconfig": KUBECONFIG}, Path(tmp) / "ci",
                            [("kubeconfig", name)],
                        )


class Masking(unittest.TestCase):
    def test_masks_every_non_trivial_line(self):
        lines = ci_secrets.mask_values(KUBECONFIG)
        self.assertIn("apiVersion: v1", lines)
        self.assertIn("    server: https://192.0.2.27:6443", lines)

    def test_ignores_short_values(self):
        self.assertEqual(ci_secrets.mask_values("true\n"), [])
        self.assertEqual(ci_secrets.mask_values("192.0.2.27\n"), ["192.0.2.27"])

    def test_cli_prints_masks_and_never_the_raw_value(self):
        with tempfile.TemporaryDirectory() as tmp:
            payload = Path(tmp) / "payload.json"
            payload.write_text(json.dumps(kv_response(kubeconfig=KUBECONFIG,
                                                      scheduler_env=SCHEDULER_ENV)))
            result = subprocess.run(
                [sys.executable, str(DIR / "ci_secrets.py"),
                 "--payload", str(payload), "--dir", str(Path(tmp) / "ci"),
                 *key_args(("kubeconfig", "kubeconfig"),
                           ("scheduler_env", "scheduler.env"))],
                capture_output=True, text=True, check=True,
            )
            lines = result.stdout.splitlines()
            self.assertTrue(all(line.startswith("::add-mask::") for line in lines[:-1]))
            self.assertTrue(lines[-1].startswith("ci_secrets: materialised 2 file(s)"))
            # The summary line carries no secret material.
            self.assertNotIn("HA_URL", lines[-1])
            self.assertNotIn("192.0.2.27", lines[-1])
            self.assertEqual(result.stderr, "")
            self.assertEqual((Path(tmp) / "ci" / "scheduler.env").read_text(),
                             SCHEDULER_ENV)
            self.assertEqual((Path(tmp) / "ci" / "kubeconfig").stat().st_mode & 0o777,
                             0o600)


if __name__ == "__main__":
    unittest.main()
