#!/usr/bin/env python3
"""deploy.sh reads its private values before touching the cluster.

The real kubectl is replaced by a stub that records calls and fails, so the
script stops at its first cluster call; the tests only assert what happens
before that point.
"""
from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
SCRIPT = HERE / "deploy.sh"

GOOD_ENV = """# private values
WEBHOOK_URL=https://bot.example.com/telegram/webhook
LAN_CIDR=192.0.2.0/24
NODE_LAN_IP="192.0.2.27"
"""


class DeployEnvTests(unittest.TestCase):
    def run_deploy(self, env_text: str) -> subprocess.CompletedProcess:
        tmp = Path(tempfile.mkdtemp(prefix="currency-deploy-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(tmp, ignore_errors=True))
        env_file = tmp / "currency-bot.env"
        env_file.write_text(env_text)
        kubectl = tmp / "kubectl"
        kubectl.write_text('#!/bin/sh\necho "kubectl $*" >> "$KUBECTL_LOG"\nexit 3\n')
        kubectl.chmod(0o755)
        env = os.environ.copy()
        env.update({
            "PRIVATE_ENV": str(env_file),
            "KUBECTL": str(kubectl),
            "KUBECTL_LOG": str(tmp / "kubectl.log"),
            "CURRENCY_BOT_IMAGE": "ghcr.io/example/currency-bot@sha256:" + "0" * 64,
            "RENDER_DIR": str(tmp / "rendered"),
            "PATH": f"{tmp}:{env['PATH']}",
        })
        res = subprocess.run(["bash", str(SCRIPT), "--skip-build"], env=env,
                             text=True, capture_output=True, timeout=60)
        res.kubectl_called = (tmp / "kubectl.log").exists()
        return res

    def test_valid_values_pass_validation(self):
        res = self.run_deploy(GOOD_ENV)
        self.assertNotIn("WEBHOOK_URL must", res.stderr)
        self.assertNotIn("are required", res.stderr)
        self.assertTrue(res.kubectl_called, res.stderr)

    def test_bad_webhook_url_is_rejected_before_kubectl(self):
        res = self.run_deploy(GOOD_ENV.replace("https://bot.example.com", "http://bot.example.com"))
        self.assertNotEqual(res.returncode, 0)
        self.assertIn("WEBHOOK_URL must", res.stderr)
        self.assertFalse(res.kubectl_called)

    def test_missing_lan_values_are_rejected(self):
        res = self.run_deploy("WEBHOOK_URL=https://bot.example.com/telegram/webhook\n")
        self.assertNotEqual(res.returncode, 0)
        self.assertIn("are required", res.stderr)
        self.assertFalse(res.kubectl_called)


if __name__ == "__main__":
    unittest.main(verbosity=2)
