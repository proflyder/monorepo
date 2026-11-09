"""Structural sanity checks for the backup PrometheusRule.

``promtool`` is not available in the hermetic test environment, so this test
parses the manifest with PyYAML and asserts the alert contract Alertmanager and
the Telegram template rely on (names, warning severity, annotations).
"""
from __future__ import annotations

from pathlib import Path
import unittest

import yaml

RULE_FILE = Path(__file__).with_name("rules") / "prometheusrule-backups.yaml"
REQUIRED_ALERTS = {
    "VzdumpBackupStale",
    "OffsiteBackupFailed",
    "OffsiteBackupStale",
    "HaBackupStale",
    "PveNodeExporterDown",
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


if __name__ == "__main__":
    unittest.main()
