#!/usr/bin/env python3
"""Read-only acceptance of the migrated cluster and Git-owned dashboards.

Set KUBECONFIG, GRAFANA_URL, GRAFANA_PASSWORD_FILE, PROMETHEUS_URL and
LOKI_URL. Run port-forwards or use private authenticated routes. Does not
run model requests, play audio, or mutate device state.
"""
import base64
import json
import os
from pathlib import Path
import subprocess
import unittest
import urllib.parse
import urllib.request


def get(url, headers=None):
    with urllib.request.urlopen(urllib.request.Request(url, headers=headers or {}), timeout=20) as r:
        body = r.read()
        return json.loads(body) if r.headers.get_content_type() == "application/json" else body


def kubectl(*args):
    return json.loads(subprocess.check_output(["kubectl", *args, "-o", "json"]))


class KubernetesRuntime(unittest.TestCase):
    def test_all_workloads_ready_and_single_instance(self):
        data = kubectl("-n", "home-assistant", "get", "deployments")
        self.assertEqual(len(data["items"]), 8)
        for item in data["items"]:
            with self.subTest(workload=item["metadata"]["name"]):
                self.assertEqual(item["spec"]["replicas"], 1)
                self.assertEqual(item["status"].get("availableReplicas"), 1)
                self.assertEqual(item["spec"]["strategy"]["type"], "Recreate")

    def test_application_volumes_retained(self):
        data = kubectl("get", "pv")
        volumes = [v for v in data["items"] if v["metadata"]["name"].startswith("runtime-")]
        self.assertEqual(len(volumes), 7)
        for volume in volumes:
            self.assertEqual(volume["status"]["phase"], "Bound")
            self.assertEqual(volume["spec"]["persistentVolumeReclaimPolicy"], "Retain")

    def test_grafana_project_dashboards(self):
        url = os.environ["GRAFANA_URL"].rstrip("/")
        password = Path(os.environ["GRAFANA_PASSWORD_FILE"]).read_text().strip()
        auth = base64.b64encode(("admin:" + password).encode()).decode()
        data = get(url + "/api/search", {"Authorization": "Basic " + auth})
        uids = {x.get("uid") for x in data}
        self.assertTrue({"ricardo-nettop", "ricardo-runtime", "ricardo-cluster", "ricardo-logs"} <= uids)

    def test_prometheus_endpoint_checks(self):
        query = urllib.parse.urlencode({"query": "ricardo_endpoint_up"})
        data = get(os.environ["PROMETHEUS_URL"].rstrip("/") + "/api/v1/query?" + query)
        values = data["data"]["result"]
        self.assertEqual(len(values), 8)
        for value in values:
            self.assertEqual(float(value["value"][1]), 1.0, value["metric"].get("endpoint"))

    def test_loki_has_infrastructure_logs(self):
        data = get(os.environ["LOKI_URL"].rstrip("/") + "/loki/api/v1/label/namespace/values")
        self.assertIn("monitoring", data["data"])


if __name__ == "__main__":
    unittest.main(verbosity=2)
