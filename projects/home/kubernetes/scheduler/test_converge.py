#!/usr/bin/env python3
"""Converge: the rendered manifest wins, runtime state is carried over."""
from __future__ import annotations

import json
import pathlib
import sys
import tempfile
import textwrap
import unittest
from unittest import mock

import yaml

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import converge  # noqa: E402

MANIFEST = textwrap.dedent(
    """
    apiVersion: apps/v1
    kind: Deployment
    metadata:
      name: ha-bridge
      namespace: home-assistant
      labels:
        app.kubernetes.io/name: ha-bridge
    spec:
      replicas: 0
      strategy:
        type: Recreate
      selector:
        matchLabels:
          app.kubernetes.io/name: ha-bridge
      template:
        spec:
          containers:
            - name: ha-bridge
              image: ghcr.io/proflyder/ha-bridge@sha256:aaa
              env:
                - name: BRIDGE_EXECUTE
                  value: "false"
              volumeMounts:
                - name: data
                  mountPath: /data
          volumes:
            - name: data
              persistentVolumeClaim:
                claimName: ha-bridge-data
    """
)

LIVE = {
    "metadata": {"name": "ha-bridge", "generation": 18, "resourceVersion": "12345"},
    "spec": {
        "replicas": 1,
        "template": {
            "spec": {
                "containers": [
                    {
                        "name": "ha-bridge",
                        "image": "ghcr.io/proflyder/ha-bridge@sha256:old",
                        "env": [{"name": "BRIDGE_EXECUTE", "value": "true"}],
                        "volumeMounts": [
                            {"name": "bin", "mountPath": "/opt/ricardo/bin", "readOnly": True},
                            {"name": "data", "mountPath": "/data"},
                        ],
                    }
                ],
                "volumes": [
                    {"name": "bin", "hostPath": {"path": "/opt/ricardo/current/bin", "type": "Directory"}},
                    {"name": "data", "persistentVolumeClaim": {"claimName": "ha-bridge-data"}},
                ],
            }
        },
    },
}


class ConvergeTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = pathlib.Path(self.tmp.name)
        self.manifest = self.dir / "bridge.yaml"
        self.manifest.write_text(MANIFEST)
        self.requests: list[tuple[str, ...]] = []
        self.replaced: dict | None = None

    def tearDown(self):
        self.tmp.cleanup()

    def fake_run(self, kubectl, *args):
        self.requests.append(args)
        if args[:3] == ("-n", "home-assistant", "get"):
            return mock.Mock(returncode=0, stdout=json.dumps(LIVE), stderr="")
        if args[:3] == ("-n", "home-assistant", "replace"):
            payload = pathlib.Path(args[args.index("-f") + 1])
            self.replaced = json.loads(payload.read_text())
            return mock.Mock(returncode=0, stdout="replaced", stderr="")
        raise AssertionError(f"unexpected call: {args}")

    def test_drift_is_pruned_and_runtime_mode_is_preserved(self):
        with mock.patch.object(converge, "_run", side_effect=self.fake_run):
            result = converge.converge(
                "kubectl",
                "home-assistant",
                "ha-bridge",
                self.manifest,
                preserve_replicas=True,
                preserve_env=True,
            )
        self.assertIn("converged", result)
        assert self.replaced is not None
        spec = self.replaced["spec"]
        self.assertEqual(spec["replicas"], 1)
        volumes = spec["template"]["spec"]["volumes"]
        self.assertEqual([v["name"] for v in volumes], ["data"])
        mounts = spec["template"]["spec"]["containers"][0]["volumeMounts"]
        self.assertEqual([m["name"] for m in mounts], ["data"])
        self.assertNotIn("hostPath", json.dumps(self.replaced))
        # Manifest image wins; live execute mode is carried over.
        container = spec["template"]["spec"]["containers"][0]
        self.assertEqual(container["image"], "ghcr.io/proflyder/ha-bridge@sha256:aaa")
        self.assertEqual(container["env"], [{"name": "BRIDGE_EXECUTE", "value": "true"}])
        self.assertEqual(self.replaced["metadata"]["resourceVersion"], "12345")

    def test_without_preserve_the_manifest_is_authoritative(self):
        with mock.patch.object(converge, "_run", side_effect=self.fake_run):
            converge.converge("kubectl", "home-assistant", "ha-bridge", self.manifest)
        assert self.replaced is not None
        self.assertEqual(self.replaced["spec"]["replicas"], 0)
        self.assertEqual(
            self.replaced["spec"]["template"]["spec"]["containers"][0]["env"],
            [{"name": "BRIDGE_EXECUTE", "value": "false"}],
        )

    def test_missing_deployment_is_created(self):
        calls: list[tuple[str, ...]] = []

        def no_current(kubectl, *args):
            calls.append(args)
            if args[:3] == ("-n", "home-assistant", "get"):
                return mock.Mock(returncode=1, stdout="", stderr="not found")
            return mock.Mock(returncode=0, stdout="created", stderr="")

        with mock.patch.object(converge, "_run", side_effect=no_current):
            result = converge.converge("kubectl", "home-assistant", "ha-bridge", self.manifest)
        self.assertIn("created", result)
        self.assertEqual(calls[-1][2], "create")

    def test_rendered_manifest_is_yaml_with_the_expected_name(self):
        self.assertEqual(converge.desired_deployment(self.manifest)["metadata"]["name"], "ha-bridge")


if __name__ == "__main__":
    unittest.main()
