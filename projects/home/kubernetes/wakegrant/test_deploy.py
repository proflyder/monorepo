#!/usr/bin/env python3
"""Deploy contracts for projects/home/kubernetes/wakegrant.

Hermetic: no cluster, no network, no secrets. deploy.sh is exercised with a
stub kubectl that records calls and fails, so only the pre-cluster behaviour is
asserted. The protocol itself is covered by the Go tests in solutions/wakegrant.
"""
from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

import yaml

HERE = Path(__file__).resolve().parent
SCRIPT = HERE / "deploy.sh"
MANIFEST = HERE / "manifests" / "wakegrant.yaml"
EXTERNAL_SECRETS = HERE / "manifests" / "external-secrets.yaml"

GOOD_ENV = """# private values
WAKE_GRANT_NODE_PORT=30130
"""


def load_docs(path: Path) -> list[dict]:
    return [doc for doc in yaml.safe_load_all(path.read_text()) if doc]


class DeployEnvTests(unittest.TestCase):
    def run_deploy(self, env_text: str, kubectl_script: str | None = None,
                   build: bool = False) -> subprocess.CompletedProcess:
        tmp = Path(tempfile.mkdtemp(prefix="wakegrant-deploy-"))
        self.addCleanup(lambda: shutil.rmtree(tmp, ignore_errors=True))
        env_file = tmp / "wakegrant.env"
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
            env["WAKE_GRANT_IMAGE"] = "ghcr.io/example/wakegrant@sha256:" + "0" * 64
        args = ["bash", str(SCRIPT)] + ([] if build else ["--skip-build"])
        res = subprocess.run(args, env=env, text=True, capture_output=True, timeout=60)
        res.kubectl_log = tmp / "kubectl.log"
        res.bazel_log = tmp / "bazel.log"
        res.render_dir = tmp / "rendered"
        res.kubectl_called = (tmp / "kubectl.log").exists()
        return res

    def test_valid_values_pass_validation(self):
        res = self.run_deploy(GOOD_ENV)
        self.assertNotIn("WAKE_GRANT_NODE_PORT must", res.stderr)
        self.assertTrue(res.kubectl_called, res.stderr)

    def test_missing_or_invalid_node_port_is_rejected_before_kubectl(self):
        for value in ("", "8080", "40000", "abc"):
            with self.subTest(value=value):
                env_text = GOOD_ENV.replace(
                    "WAKE_GRANT_NODE_PORT=30130",
                    f"WAKE_GRANT_NODE_PORT={value}",
                )
                res = self.run_deploy(env_text)
                self.assertNotEqual(res.returncode, 0)
                self.assertIn("WAKE_GRANT_NODE_PORT", res.stderr)
                self.assertFalse(res.kubectl_called)

    def test_render_exposes_a_nodeport_and_no_placeholders(self):
        res = self.run_deploy(GOOD_ENV)
        self.assertTrue(res.kubectl_called, res.stderr)
        rendered_path = res.render_dir / "wakegrant.yaml"
        text = rendered_path.read_text()
        self.assertNotIn("${", text)

        docs = {doc["kind"]: doc for doc in load_docs(rendered_path)}
        self.assertEqual(set(docs), {"PersistentVolumeClaim", "Deployment", "Service"})

        pvc = docs["PersistentVolumeClaim"]
        self.assertEqual(pvc["metadata"]["name"], "wakegrant-data")
        self.assertEqual(pvc["spec"]["storageClassName"], "local-retain")

        deployment = docs["Deployment"]
        self.assertEqual(deployment["spec"]["replicas"], 1)
        self.assertEqual(deployment["spec"]["strategy"]["type"], "Recreate")
        pod = deployment["spec"]["template"]["spec"]
        self.assertTrue(pod["securityContext"]["runAsNonRoot"])
        container = pod["containers"][0]
        self.assertEqual(container["securityContext"]["readOnlyRootFilesystem"], True)
        self.assertEqual(container["securityContext"]["allowPrivilegeEscalation"], False)
        self.assertEqual(container["securityContext"]["capabilities"]["drop"], ["ALL"])
        self.assertEqual(container["readinessProbe"]["httpGet"]["path"], "/healthz")
        self.assertEqual(container["livenessProbe"]["httpGet"]["path"], "/healthz")
        self.assertEqual(container["image"], "ghcr.io/example/wakegrant@sha256:" + "0" * 64)
        self.assertEqual(container["resources"]["limits"]["memory"], "128Mi")

        service = docs["Service"]
        self.assertEqual(service["spec"]["type"], "NodePort")
        ports = {port["name"]: port for port in service["spec"]["ports"]}
        self.assertEqual(ports["http"]["port"], 8099)
        self.assertEqual(ports["http"]["nodePort"], 30130)

    def test_manifest_keeps_secrets_out_of_environment_values(self):
        text = MANIFEST.read_text()
        # Tokens and keys arrive as files from the ESO-owned Secret.
        self.assertIn("WAKE_GRANT_ISSUER_TOKEN_FILE", text)
        self.assertIn("WAKE_GRANT_MACHINES_FILE", text)
        self.assertNotIn("WAKE_GRANT_ISSUER_TOKEN\n", text)
        self.assertNotIn("WAKE_GRANT_ISSUER_TOKEN:", text)
        self.assertIn("readOnlyRootFilesystem: true", text)
        self.assertIn("defaultMode: 0440", text)

    def test_build_invokes_the_bazel_binary_from_path(self):
        res = self.run_deploy(GOOD_ENV, build=True)
        self.assertTrue(res.bazel_log.exists(), res.stderr)
        self.assertIn("build //solutions/wakegrant:wakegrant_image",
                      res.bazel_log.read_text())

    def test_external_secret_targets_openbao_with_safe_ownership(self):
        docs = load_docs(EXTERNAL_SECRETS)
        self.assertEqual(len(docs), 1)
        doc = docs[0]
        self.assertEqual(doc["kind"], "ExternalSecret")
        self.assertEqual(doc["metadata"]["namespace"], "home-assistant")
        self.assertEqual(doc["metadata"]["name"], "wakegrant")
        self.assertEqual(doc["spec"]["secretStoreRef"],
                         {"name": "openbao", "kind": "ClusterSecretStore"})
        self.assertEqual(doc["spec"]["target"]["creationPolicy"], "Owner")
        self.assertEqual(doc["spec"]["target"]["deletionPolicy"], "Retain")
        self.assertEqual(doc["spec"]["dataFrom"][0]["extract"]["key"],
                         "k8s/home-assistant/wakegrant")


if __name__ == "__main__":
    unittest.main(verbosity=2)
