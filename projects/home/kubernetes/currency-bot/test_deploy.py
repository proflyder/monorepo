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

import yaml

HERE = Path(__file__).resolve().parent
SCRIPT = HERE / "deploy.sh"

GOOD_ENV = """# private values
WEBHOOK_URL=https://bot.example.com/telegram/webhook
LAN_CIDR=192.0.2.0/24
NODE_LAN_IP="192.0.2.27"
CURRENCY_BOT_NODE_PORT=30100
"""


class DeployEnvTests(unittest.TestCase):
    def run_deploy(self, env_text: str, kubectl_script: str | None = None,
                   build: bool = False,
                   extra_env: dict[str, str] | None = None) -> subprocess.CompletedProcess:
        tmp = Path(tempfile.mkdtemp(prefix="currency-deploy-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(tmp, ignore_errors=True))
        env_file = tmp / "currency-bot.env"
        env_file.write_text(env_text)
        kubectl = tmp / "kubectl"
        kubectl.write_text(kubectl_script or '#!/bin/sh\necho "kubectl $*" >> "$KUBECTL_LOG"\nexit 3\n')
        kubectl.chmod(0o755)
        bazel = tmp / "bazel"
        bazel.write_text('#!/bin/sh\necho "bazel $*" >> "$BAZEL_LOG"\nexit 0\n')
        bazel.chmod(0o755)
        env = os.environ.copy()
        env.pop("BAZELISK", None)
        env.update({
            "PRIVATE_ENV": str(env_file),
            "KUBECTL": str(kubectl),
            "KUBECTL_LOG": str(tmp / "kubectl.log"),
            "BAZEL_LOG": str(tmp / "bazel.log"),
            "RENDER_DIR": str(tmp / "rendered"),
            "PATH": f"{tmp}:{env['PATH']}",
        })
        if not build:
            env["CURRENCY_BOT_IMAGE"] = "ghcr.io/example/currency-bot@sha256:" + "0" * 64
        if extra_env:
            env.update(extra_env)
        args = ["bash", str(SCRIPT)] + ([] if build else ["--skip-build"])
        res = subprocess.run(args, env=env,
                             text=True, capture_output=True, timeout=60)
        res.kubectl_log = str(tmp / "kubectl.log")
        res.bazel_log = tmp / "bazel.log"
        res.render_dir = tmp / "rendered"
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

    def test_missing_or_invalid_node_port_is_rejected(self):
        for value in ("", "8080", "40000", "abc"):
            with self.subTest(value=value):
                env_text = GOOD_ENV.replace(
                    "CURRENCY_BOT_NODE_PORT=30100",
                    f"CURRENCY_BOT_NODE_PORT={value}",
                )
                res = self.run_deploy(env_text)
                self.assertNotEqual(res.returncode, 0)
                self.assertIn("CURRENCY_BOT_NODE_PORT", res.stderr)
                self.assertFalse(res.kubectl_called)

    def test_render_exposes_api_nodeport_and_nft_template_matches(self):
        res = self.run_deploy(GOOD_ENV)
        self.assertTrue(res.kubectl_called, res.stderr)

        services = [
            obj
            for obj in yaml.safe_load_all((res.render_dir / "currency-bot.yaml").read_text())
            if obj and obj.get("kind") == "Service" and obj["metadata"]["name"] == "currency-bot"
        ]
        self.assertEqual(len(services), 1)
        service = services[0]
        self.assertEqual(service["spec"]["type"], "NodePort")
        ports = {p["name"]: p for p in service["spec"]["ports"]}
        self.assertEqual(ports["http"]["port"], 8080)
        self.assertEqual(ports["http"]["nodePort"], 30100)
        self.assertNotIn("CURRENCY_BOT_NODE_PORT", (res.render_dir / "currency-bot.yaml").read_text())

        policies = {
            obj["metadata"]["name"]: obj
            for obj in yaml.safe_load_all((res.render_dir / "network-policies.yaml").read_text())
            if obj and obj.get("kind") == "NetworkPolicy"
        }
        ingress = policies["currency-bot-ingress"]["spec"]["ingress"]
        api_sources = set()
        for rule in ingress:
            if any(p.get("port") == 8080 for p in rule.get("ports", [])):
                api_sources.update(block["ipBlock"]["cidr"] for block in rule.get("from", []) if "ipBlock" in block)
        self.assertIn("192.0.2.27/32", api_sources)
        self.assertIn("192.0.2.0/24", api_sources)

    def test_forbidden_namespace_read_is_not_an_apply(self):
        # CI's ServiceAccount cannot read cluster-scoped namespaces; that must
        # not turn into an attempt to create the namespace.
        stub = """#!/bin/sh
echo "kubectl $*" >> "$KUBECTL_LOG"
case "$*" in
  "get namespace currency-bot -o name")
    echo 'Error from server (Forbidden): namespaces "currency-bot" is forbidden' >&2; exit 1 ;;
esac
exit 3
"""
        res = self.run_deploy(GOOD_ENV, stub)
        self.assertIn("assuming admin-managed", res.stdout)
        log = Path(res.kubectl_log).read_text()
        self.assertNotIn("namespace.yaml", log)

    def test_missing_namespace_is_applied(self):
        stub = """#!/bin/sh
echo "kubectl $*" >> "$KUBECTL_LOG"
case "$*" in
  "get namespace currency-bot -o name")
    echo 'Error from server (NotFound): namespaces "currency-bot" not found' >&2; exit 1 ;;
esac
exit 3
"""
        res = self.run_deploy(GOOD_ENV, stub)
        log = Path(res.kubectl_log).read_text()
        self.assertIn("apply -f", log)
        self.assertIn("namespace.yaml", log)

    def test_build_invokes_the_bazel_binary_from_path(self):
        # CI puts Bazelisk on PATH as `bazel` (setup-bazel). A helper function
        # named `bazel` used to shadow it, so the build silently never ran.
        res = self.run_deploy(GOOD_ENV, build=True)
        self.assertTrue(res.bazel_log.exists(), res.stderr)
        self.assertIn("build //solutions/currency-bot:currency-bot_image",
                      res.bazel_log.read_text())

    def test_prebuilt_digest_skips_the_build_and_is_rendered(self):
        # deploy-home passes the digest promoted from the pull request
        # (DECISIONS 36): no Bazel, no push, the digest goes into the manifest.
        digest = "sha256:" + "d" * 64
        res = self.run_deploy(
            GOOD_ENV,
            build=True,
            extra_env={"PREBUILT_DIGESTS": f"currency-bot={digest}"},
        )
        self.assertTrue(res.kubectl_called, res.stderr)
        self.assertFalse(
            res.bazel_log.exists() and res.bazel_log.read_text().strip(),
            "prebuilt digest must not trigger a build",
        )
        self.assertIn(
            f"ghcr.io/proflyder/currency-bot@{digest}",
            (Path(res.render_dir) / "currency-bot.yaml").read_text(),
        )

    def test_malformed_prebuilt_digest_fails_closed(self):
        res = self.run_deploy(
            GOOD_ENV,
            build=True,
            extra_env={"PREBUILT_DIGESTS": "currency-bot=latest"},
        )
        self.assertNotEqual(res.returncode, 0)
        self.assertIn("malformed prebuilt digest", res.stderr)
        self.assertFalse(res.kubectl_called)


if __name__ == "__main__":
    unittest.main(verbosity=2)
