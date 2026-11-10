#!/usr/bin/env python3
"""Manifest contracts for projects/home/kubernetes/wakegrant.

Flux applies the manifests from `deploy/home-apps` digests, so there is no
deploy script left to exercise. These tests keep the parts only the deployment
knows: the Secret is mounted as files (never as env values), the ExternalSecret
targets the documented OpenBao key and never deletes the live Secret, and the
Kustomization keeps the digest/pin contract. The protocol itself is covered by
the Rust tests in solutions/wakegrant.
"""
from __future__ import annotations

import unittest
from pathlib import Path

import yaml

HERE = Path(__file__).resolve().parent
MANIFEST = HERE / "manifests" / "wakegrant.yaml"
EXTERNAL_SECRETS = HERE / "manifests" / "external-secrets.yaml"
KUSTOMIZATION = HERE / "manifests" / "kustomization.yaml"
DEPLOY = HERE / "deploy.sh"
ENV_EXAMPLE = HERE / "wakegrant.env.example"


def load_docs(path: Path) -> list[dict]:
    return [doc for doc in yaml.safe_load_all(path.read_text()) if doc]


class ManifestTests(unittest.TestCase):
    def test_manifest_keeps_secrets_out_of_environment_values(self):
        text = MANIFEST.read_text()
        # Tokens and keys arrive as files from the ESO-owned Secret.
        self.assertIn("WAKE_GRANT_ISSUER_TOKEN_FILE", text)
        self.assertIn("WAKE_GRANT_MACHINES_FILE", text)
        self.assertNotIn("WAKE_GRANT_ISSUER_TOKEN\n", text)
        self.assertNotIn("WAKE_GRANT_ISSUER_TOKEN:", text)
        self.assertIn("readOnlyRootFilesystem: true", text)
        self.assertIn("defaultMode: 0440", text)

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

    def test_kustomization_keeps_the_pinned_image_contract(self):
        text = KUSTOMIZATION.read_text()
        self.assertIn("${WAKEGRANT_IMAGE}", text)
        self.assertIn("kustomize.toolkit.fluxcd.io/prune", text)

    def test_no_deploy_script_or_private_env_is_left(self):
        # The CI deploy path is gone: Flux owns the workload and the private
        # values live in OpenBao `k8s/flux-system/app-settings`.
        self.assertFalse(DEPLOY.exists())
        self.assertFalse(ENV_EXAMPLE.exists())


if __name__ == "__main__":
    unittest.main(verbosity=2)
