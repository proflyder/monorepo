"""Structural sanity checks for the backup and Alertmanager PrometheusRules.

``promtool`` is not available in the hermetic test environment, so this test
parses the manifests with PyYAML and asserts the alert contract Alertmanager and
the Telegram template rely on (names, severity, annotations).
"""
from __future__ import annotations

from pathlib import Path
import unittest

import yaml

RULE_DIR = Path(__file__).with_name("rules")
RULE_FILE = RULE_DIR / "prometheusrule-backups.yaml"
ALERTMANAGER_RULE_FILE = RULE_DIR / "prometheusrule-alertmanager.yaml"
REQUIRED_ALERTS = {
    "VzdumpBackupStale",
    "OffsiteBackupFailed",
    "OffsiteBackupStale",
    "HaBackupStale",
    "PveNodeExporterDown",
}
REQUIRED_ALERTMANAGER_ALERTS = {
    "AlertmanagerFailedToSendAlerts",
    "AlertmanagerClusterFailedToSendAlerts",
    "HeartbeatPingFailing",
}


class BackupRuleTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.document = yaml.safe_load(RULE_FILE.read_text(encoding="utf-8"))
        cls.rules = [
            rule
            for group in cls.document["spec"]["groups"]
            for rule in group["rules"]
        ]

    def test_manifest_contract(self) -> None:
        self.assertEqual(self.document["apiVersion"], "monitoring.coreos.com/v1")
        self.assertEqual(self.document["kind"], "PrometheusRule")
        self.assertEqual(self.document["metadata"]["namespace"], "monitoring")

    def test_all_required_alerts_are_present(self) -> None:
        names = {rule["alert"] for rule in self.rules}
        self.assertEqual(names, REQUIRED_ALERTS)

    def test_alerts_are_warning_with_telegram_annotations(self) -> None:
        for rule in self.rules:
            with self.subTest(alert=rule["alert"]):
                self.assertEqual(rule["labels"]["severity"], "warning")
                self.assertTrue(rule["annotations"]["summary"].strip())
                self.assertTrue(rule["annotations"]["description"].strip())
                self.assertTrue(rule["expr"].strip())
                self.assertNotIn("\t", rule["expr"])

    def test_vzdump_absent_covers_every_expected_guest(self) -> None:
        absent = [
            rule["expr"]
            for rule in self.rules
            if rule["alert"] == "VzdumpBackupStale" and "absent(" in rule["expr"]
        ]
        self.assertEqual(len(absent), 1)
        for vmid in ("100", "101", "102"):
            self.assertIn(f'vmid="{vmid}"', absent[0])

    def test_pve_scrape_target_is_substituted_from_flux_settings(self) -> None:
        values = yaml.safe_load(
            (Path(__file__).with_name("values") / "prometheus.yaml").read_text(encoding="utf-8")
        )
        configs = values["prometheus"]["prometheusSpec"]["additionalScrapeConfigs"]
        self.assertEqual(len(configs), 1)
        self.assertEqual(configs[0]["job_name"], "pve-node")
        # The real host:port stays out of Git.
        self.assertEqual(configs[0]["static_configs"][0]["targets"], ["${PVE_NODE_TARGET}"])

    def test_grafana_root_url_uses_the_home_suffix(self) -> None:
        values = yaml.safe_load(
            (Path(__file__).with_name("values") / "prometheus.yaml").read_text(encoding="utf-8")
        )
        server = values["grafana"]["grafana.ini"]["server"]
        self.assertEqual(server["root_url"], "https://grafana.${HOME_DOMAIN}/")


class AlertmanagerRuleTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.document = yaml.safe_load(ALERTMANAGER_RULE_FILE.read_text(encoding="utf-8"))
        cls.rules = [
            rule
            for group in cls.document["spec"]["groups"]
            for rule in group["rules"]
        ]

    def test_manifest_contract(self) -> None:
        self.assertEqual(self.document["apiVersion"], "monitoring.coreos.com/v1")
        self.assertEqual(self.document["kind"], "PrometheusRule")
        self.assertEqual(self.document["metadata"]["namespace"], "monitoring")
        self.assertEqual(self.document["metadata"]["name"], "home-alertmanager-alerts")

    def test_rules_kustomization_includes_the_manifest(self) -> None:
        kustomization = yaml.safe_load((RULE_DIR / "kustomization.yaml").read_text(encoding="utf-8"))
        self.assertIn(ALERTMANAGER_RULE_FILE.name, kustomization["resources"])

    def test_all_required_alerts_are_present(self) -> None:
        names = {rule["alert"] for rule in self.rules}
        self.assertEqual(names, REQUIRED_ALERTMANAGER_ALERTS)

    def test_default_alerts_keep_severity_and_runbook_but_exclude_webhook(self) -> None:
        by_name = {rule["alert"]: rule for rule in self.rules}
        expected = {
            "AlertmanagerFailedToSendAlerts": (
                "warning",
                "https://runbooks.prometheus-operator.dev/runbooks/alertmanager/alertmanagerfailedtosendalerts",
            ),
            "AlertmanagerClusterFailedToSendAlerts": (
                "critical",
                "https://runbooks.prometheus-operator.dev/runbooks/alertmanager/alertmanagerclusterfailedtosendalerts",
            ),
        }
        for name, (severity, runbook) in expected.items():
            with self.subTest(alert=name):
                rule = by_name[name]
                self.assertEqual(rule["labels"]["severity"], severity)
                self.assertEqual(rule["annotations"]["runbook_url"], runbook)
                self.assertEqual(rule["for"], "5m")
                self.assertIn('integration!="webhook"', rule["expr"])
                self.assertIn("alertmanager_notifications_failed_total", rule["expr"])
                self.assertIn("alertmanager_notifications_total", rule["expr"])

    def test_heartbeat_ping_failing_is_a_bounded_warning(self) -> None:
        rule = next(r for r in self.rules if r["alert"] == "HeartbeatPingFailing")
        self.assertEqual(rule["labels"]["severity"], "warning")
        self.assertEqual(rule["for"], "10m")
        self.assertIn('integration="webhook"', rule["expr"])
        self.assertIn("increase(", rule["expr"])
        self.assertIn("[30m]", rule["expr"])
        self.assertIn(">= 4", rule["expr"])
        self.assertTrue(rule["annotations"]["summary"].strip())
        self.assertTrue(rule["annotations"]["description"].strip())

    def test_default_chart_rules_are_disabled(self) -> None:
        values = yaml.safe_load(
            (Path(__file__).with_name("values") / "prometheus.yaml").read_text(encoding="utf-8")
        )
        disabled = values["defaultRules"]["disabled"]
        self.assertEqual(disabled, {
            "AlertmanagerFailedToSendAlerts": True,
            "AlertmanagerClusterFailedToSendAlerts": True,
        })


if __name__ == "__main__":
    unittest.main()
