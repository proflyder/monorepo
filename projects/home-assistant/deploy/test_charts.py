"""Render contracts that protect state and dashboard delivery during upgrades."""
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
import yaml

HERE = Path(__file__).resolve().parent
OBS = HERE.parent / "observability/chart"


@unittest.skipUnless(shutil.which("helm"), "Helm is required for chart contracts")
class ChartContracts(unittest.TestCase):
    def test_public_version_manifest_is_tracked_and_not_ignored(self):
        root = HERE.parents[2]
        manifest = root / "projects/home/kubernetes/versions.sh"
        self.assertTrue(manifest.is_file())
        result = subprocess.run(["git", "check-ignore", str(manifest)], cwd=root, capture_output=True)
        self.assertEqual(result.returncode, 1, "Public versions must not match private env ignore rules")
        tracked = subprocess.check_output(["git", "ls-files", "--error-unmatch", "projects/home/kubernetes/versions.sh"], cwd=root)
        self.assertTrue(tracked)

    def test_runtime_is_single_writer_and_retains_data(self):
        values = {"nodeName": "example-node", "storage": {
            "data": {"size": "1Gi", "path": "/srv/example/data"}},
            "workloads": {"example": {"replicas": 1, "containers": [{
                "name": "example", "image": "example.invalid/runtime:1"}],
                "volumes": []}}}
        with tempfile.NamedTemporaryFile(mode="w", suffix=".yaml") as f:
            yaml.safe_dump(values, f)
            f.flush()
            rendered = subprocess.check_output(["helm", "template", "test", str(HERE / "chart"), "-f", f.name])
        docs = list(yaml.safe_load_all(rendered))
        deployment = next(x for x in docs if x["kind"] == "Deployment")
        self.assertEqual(deployment["spec"]["replicas"], 1)
        self.assertEqual(deployment["spec"]["strategy"]["type"], "Recreate")
        pod = deployment["spec"]["template"]["spec"]
        self.assertTrue(pod["hostNetwork"])
        self.assertEqual(pod["dnsPolicy"], "ClusterFirstWithHostNet")
        self.assertFalse(pod["automountServiceAccountToken"])
        volume = next(x for x in docs if x["kind"] == "PersistentVolume")
        self.assertEqual(volume["spec"]["persistentVolumeReclaimPolicy"], "Retain")
        self.assertEqual(volume["spec"]["local"]["path"], "/srv/example/data")

    def test_haos_proxy_and_tailscale_ingress_are_wired(self):
        runtime_values = {
            "nodeName": "example-node",
            "haProxy": {
                "enabled": True,
                "upstream": "192.0.2.28:8123",
                "privateBridge": {
                    "enabled": True,
                    "bindAddress": "192.0.2.27",
                    "allowedClient": "192.0.2.28/32",
                    "matterEnabled": True,
                },
            },
        }
        with tempfile.NamedTemporaryFile(mode="w", suffix=".yaml") as f:
            yaml.safe_dump(runtime_values, f)
            f.flush()
            rendered = subprocess.check_output(["helm", "template", "runtime", str(HERE / "chart"), "-f", f.name])
        docs = [x for x in yaml.safe_load_all(rendered) if x]
        config = next(x for x in docs if x["kind"] == "ConfigMap" and x["metadata"]["name"] == "haos-proxy")
        cfg = config["data"]["haproxy.cfg"]
        self.assertIn("server haos 192.0.2.28:8123 check", cfg)
        self.assertIn("bind 192.0.2.27:8643", cfg)
        self.assertIn("bind 192.0.2.27:5580", cfg)
        proxy = next(x for x in docs if x["kind"] == "Deployment" and x["metadata"]["name"] == "haos-proxy")
        self.assertTrue(proxy["spec"]["template"]["spec"]["hostNetwork"])

        tailscale = HERE.parents[1] / "home/kubernetes/tailscale/chart"
        rendered = subprocess.check_output([
            "helm", "template", "access", str(tailscale),
            "--set", "hostname=example-tailnet.test",
            "--set", "haosAddress=192.0.2.28",
        ])
        docs = [x for x in yaml.safe_load_all(rendered) if x]
        ingress = next(x for x in docs if x["kind"] == "Ingress")
        self.assertEqual(ingress["spec"]["tls"][0]["hosts"], ["example-tailnet.test"])
        endpoint = next(x for x in docs if x["kind"] == "EndpointSlice")
        self.assertEqual(endpoint["endpoints"][0]["addresses"], ["192.0.2.28"])

    def test_every_dashboard_is_packaged_once_with_stable_uid(self):
        rendered = subprocess.check_output(["helm", "template", "dashboards", str(OBS), "-n", "monitoring"])
        maps = [x for x in yaml.safe_load_all(rendered) if x and x["kind"] == "ConfigMap"]
        files = list((OBS / "dashboards").glob("*.json"))
        self.assertEqual(len(maps), len(files))
        uids = set()
        for config in maps:
            self.assertEqual(config["metadata"]["labels"]["grafana_dashboard"], "1")
            self.assertIn("grafana_folder", config["metadata"]["annotations"])
            for data in config["data"].values():
                dashboard = json.loads(data)
                self.assertNotIn(dashboard["uid"], uids)
                uids.add(dashboard["uid"])
                self.assertTrue(dashboard["panels"])


if __name__ == "__main__":
    unittest.main()
