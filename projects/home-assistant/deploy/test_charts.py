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
