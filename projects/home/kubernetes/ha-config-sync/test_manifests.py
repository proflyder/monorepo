"""Structural contracts for the ha-config-sync deployment (no cluster needed).

Flux applies these manifests; the Rust solution owns the behaviour. These tests
pin the security posture, the secret wiring, the narrow network egress and the
alert expressions the dashboards/runbooks rely on.
"""
from __future__ import annotations

import re
import unittest
from pathlib import Path

import yaml

HERE = Path(__file__).resolve().parent
MANIFESTS = HERE / "manifests"


def docs(name: str) -> list[dict]:
    return [doc for doc in yaml.safe_load_all((MANIFESTS / name).read_text()) if doc]


def one(name: str, kind: str) -> dict:
    return next(doc for doc in docs(name) if doc["kind"] == kind)


class DeploymentTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.deployment = one("deployment.yaml", "Deployment")
        cls.spec = cls.deployment["spec"]
        cls.pod = cls.spec["template"]["spec"]
        cls.container = cls.pod["containers"][0]

    def test_single_recreate_replica(self):
        self.assertEqual(self.spec["replicas"], 1)
        self.assertEqual(self.spec["strategy"]["type"], "Recreate")
        self.assertEqual(
            self.deployment["metadata"]["namespace"], "home-assistant"
        )

    def test_no_service_account_or_privilege(self):
        self.assertIs(self.pod["automountServiceAccountToken"], False)
        security = self.pod["securityContext"]
        self.assertIs(security["runAsNonRoot"], True)
        self.assertEqual(security["runAsUser"], 65534)
        self.assertEqual(security["runAsGroup"], 65534)
        self.assertEqual(security["seccompProfile"]["type"], "RuntimeDefault")
        container = self.container["securityContext"]
        self.assertIs(container["allowPrivilegeEscalation"], False)
        self.assertIs(container["readOnlyRootFilesystem"], True)
        self.assertEqual(container["capabilities"]["drop"], ["ALL"])
        self.assertNotIn("privileged", container)

    def test_environment_matches_the_solution_contract(self):
        env = {item["name"]: item["value"] for item in self.container["env"]}
        self.assertEqual(env["HA_URL"], "http://${HAOS_IP}:8123")
        self.assertEqual(env["SSH_ADDR"], "${HAOS_IP}:22222")
        self.assertEqual(env["SSH_USER"], "root")
        self.assertEqual(env["SSH_KEY_FILE"], "/run/secrets/ha-config-sync/ssh_private_key")
        self.assertEqual(env["SSH_KNOWN_HOSTS_FILE"], "/run/secrets/ha-config-sync/known_hosts")
        self.assertEqual(env["HA_TOKEN_FILE"], "/run/secrets/ha-config-sync/ha_token")
        self.assertEqual(env["DESIRED_DIR"], "/opt/ha-voice")
        self.assertEqual(env["MANIFEST_FILE"], "/opt/ha-voice-sync.yaml")
        self.assertEqual(env["SYNC_MODE"], "apply")
        self.assertEqual(env["METRICS_ADDR"], "0.0.0.0:9119")
        self.assertEqual(env["RESTART_GUARD_ENTITY"], "input_boolean.alarm_ringing")

    def test_image_and_probes(self):
        self.assertEqual(self.container["image"], "${HA_CONFIG_SYNC_IMAGE}")
        self.assertEqual(self.container["ports"][0]["containerPort"], 9119)
        for probe in ("readinessProbe", "livenessProbe"):
            self.assertEqual(self.container[probe]["httpGet"]["path"], "/healthz")
        self.assertTrue(self.container["resources"]["limits"]["memory"].endswith("Mi"))

    def test_secret_is_mounted_read_only(self):
        mounts = {mount["name"]: mount for mount in self.container["volumeMounts"]}
        self.assertIs(mounts["secrets"]["readOnly"], True)
        volumes = {volume["name"]: volume for volume in self.pod["volumes"]}
        self.assertEqual(volumes["secrets"]["secret"]["secretName"], "ha-config-sync")


class SecretAndNetworkTests(unittest.TestCase):
    def test_external_secret_wiring(self):
        secret = one("external-secrets.yaml", "ExternalSecret")
        data = {item["secretKey"]: item["remoteRef"] for item in secret["spec"]["data"]}
        self.assertEqual(
            data["ssh_private_key"],
            {"key": "k8s/home-assistant/ha-config-sync", "property": "ssh_private_key"},
        )
        self.assertEqual(
            data["known_hosts"],
            {"key": "k8s/home-assistant/ha-config-sync", "property": "known_hosts"},
        )
        self.assertEqual(
            data["ha_token"],
            {"key": "k8s/home-assistant/hermes-environment", "property": "HERMES_HA_TOKEN"},
        )
        self.assertEqual(secret["spec"]["target"]["name"], "ha-config-sync")
        self.assertEqual(secret["spec"]["target"]["deletionPolicy"], "Retain")

    def test_network_egress_is_only_dns_and_haos(self):
        policies = {doc["metadata"]["name"]: doc for doc in docs("network-policy.yaml")}
        egress = policies["ha-config-sync-egress"]["spec"]["egress"]
        ports: set[int] = set()
        for rule in egress:
            for port in rule.get("ports", []):
                ports.add(port["port"])
        self.assertEqual(ports, {53, 22222, 8123})
        rendered = yaml.safe_dump(egress)
        self.assertIn("${HAOS_IP}/32", rendered)
        for forbidden in (443, 6443, 22):
            self.assertNotIn(forbidden, ports)

        ingress = policies["ha-config-sync-ingress"]["spec"]["ingress"]
        self.assertEqual(
            {port["port"] for rule in ingress for port in rule.get("ports", [])},
            {9119},
        )
        rendered = yaml.safe_dump(ingress)
        self.assertIn("monitoring", rendered)

    def test_public_manifests_carry_no_private_addresses(self):
        for path in MANIFESTS.glob("*.yaml"):
            text = path.read_text()
            for address in re.findall(r"\b\d{1,3}(?:\.\d{1,3}){3}\b", text):
                self.assertTrue(
                    address.startswith("192.0.2.") or address in {"0.0.0.0"},
                    f"{path.name}: unexpected address {address}",
                )


class MonitoringTests(unittest.TestCase):
    def test_service_monitor_selects_the_service(self):
        monitor = one("servicemonitor.yaml", "ServiceMonitor")
        service = one("service.yaml", "Service")
        self.assertEqual(
            monitor["spec"]["selector"]["matchLabels"],
            {"app.kubernetes.io/name": "ha-config-sync"},
        )
        self.assertEqual(
            service["metadata"]["labels"]["app.kubernetes.io/name"], "ha-config-sync"
        )
        self.assertEqual(monitor["spec"]["endpoints"][0]["path"], "/metrics")
        self.assertEqual(monitor["spec"]["endpoints"][0]["port"], "http")

    def test_alert_contract(self):
        rules = [
            rule
            for group in one("prometheusrule.yaml", "PrometheusRule")["spec"]["groups"]
            for rule in group["rules"]
        ]
        by_name = {rule["alert"]: rule for rule in rules}
        self.assertEqual(
            set(by_name),
            {
                "HaConfigDrift",
                "HaConfigSyncFailing",
                "HaConfigSyncRolledBack",
                "HaConfigSyncDown",
            },
        )
        expected = {
            "HaConfigDrift": "max(ha_config_sync_drift_files) > 0",
            "HaConfigSyncFailing": "min(ha_config_sync_last_check_success) == 0",
            "HaConfigSyncRolledBack": (
                'increase(ha_config_sync_apply_total{result="rolled_back"}[1h]) > 0'
            ),
            "HaConfigSyncDown": 'absent(up{job=~".*ha-config-sync.*"} == 1)',
        }
        for name, expr in expected.items():
            with self.subTest(alert=name):
                rule = by_name[name]
                self.assertEqual(" ".join(rule["expr"].split()), expr)
                self.assertEqual(rule["labels"]["severity"], "warning")
                self.assertEqual(rule["labels"]["component"], "ha-config")
                self.assertTrue(rule["annotations"]["summary"].strip())
                self.assertTrue(rule["annotations"]["description"].strip())
                self.assertRegex(rule["annotations"]["summary"], r"[А-Яа-яЁё]")
        self.assertEqual(by_name["HaConfigDrift"]["for"], "15m")
        self.assertEqual(by_name["HaConfigSyncFailing"]["for"], "30m")
        self.assertEqual(by_name["HaConfigSyncDown"]["for"], "15m")

    def test_kustomization_covers_every_manifest(self):
        kustomization = yaml.safe_load((MANIFESTS / "kustomization.yaml").read_text())
        resources = set(kustomization["resources"])
        files = {path.name for path in MANIFESTS.glob("*.yaml")} - {"kustomization.yaml"}
        self.assertEqual(resources, files)


if __name__ == "__main__":
    unittest.main()
