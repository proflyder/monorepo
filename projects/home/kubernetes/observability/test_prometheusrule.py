"""Structural sanity checks for the backup, Alertmanager and Flux PrometheusRules.

``promtool`` is not available in the hermetic test environment, so this test
parses the manifests with PyYAML and asserts the alert contract Alertmanager and
the Telegram template rely on (names, severity, annotations). The Flux rules get
an extra contract check: every label they select on `gotk_resource_info` must be
one that kube-state-metrics is configured to emit, because a selector on a label
that never exists silently disables the alert (the historical
`gotk_reconcile_condition` bug). When `promtool` happens to be on PATH the rules
are also syntax-checked; otherwise that test skips.
"""
from __future__ import annotations

from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest

import yaml

RULE_DIR = Path(__file__).with_name("rules")
RULE_FILE = RULE_DIR / "prometheusrule-backups.yaml"
ALERTMANAGER_RULE_FILE = RULE_DIR / "prometheusrule-alertmanager.yaml"
FLUX_RULE_FILE = RULE_DIR / "prometheusrule-flux.yaml"
VALUES_FILE = Path(__file__).with_name("values") / "prometheus.yaml"
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
REQUIRED_FLUX_ALERTS = {
    "FluxKustomizationNotReady",
    "HelmReleaseNotReady",
    "PinsStale",
}
# Metric deleted from the Flux controllers in 2.x; nothing may reference it.
REMOVED_FLUX_METRIC = "gotk_reconcile_condition"


def normalize(expr: str) -> str:
    """Collapse whitespace so expression contracts survive re-indentation."""
    return " ".join(expr.split())


def rules_of(path: Path) -> list[dict]:
    document = yaml.safe_load(path.read_text(encoding="utf-8"))
    return [rule for group in document["spec"]["groups"] for rule in group["rules"]]


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


class FluxRuleTests(unittest.TestCase):
    """Delivery alerts read the Flux state through kube-state-metrics."""

    @classmethod
    def setUpClass(cls) -> None:
        cls.document = yaml.safe_load(FLUX_RULE_FILE.read_text(encoding="utf-8"))
        cls.rules = {
            rule["alert"]: rule
            for group in cls.document["spec"]["groups"]
            for rule in group["rules"]
        }
        values = yaml.safe_load(VALUES_FILE.read_text(encoding="utf-8"))
        cls.ksm = values["kube-state-metrics"]
        cls.resources = {
            resource["groupVersionKind"]["kind"]: resource
            for resource in cls.ksm["customResourceState"]["config"]["spec"]["resources"]
        }

    def test_manifest_contract(self) -> None:
        self.assertEqual(self.document["apiVersion"], "monitoring.coreos.com/v1")
        self.assertEqual(self.document["kind"], "PrometheusRule")
        self.assertEqual(self.document["metadata"]["namespace"], "monitoring")
        self.assertEqual(self.document["metadata"]["name"], "home-flux-alerts")

    def test_all_required_alerts_are_present(self) -> None:
        self.assertEqual(set(self.rules), REQUIRED_FLUX_ALERTS)

    def test_alerts_are_warning_with_telegram_annotations(self) -> None:
        for name, rule in self.rules.items():
            with self.subTest(alert=name):
                self.assertEqual(rule["labels"]["severity"], "warning")
                self.assertEqual(rule["labels"]["component"], "flux")
                self.assertEqual(rule["for"], "15m")
                self.assertTrue(rule["annotations"]["summary"].strip())
                self.assertTrue(rule["annotations"]["description"].strip())
                self.assertNotIn("\t", rule["expr"])

    def test_no_rule_references_the_removed_flux_metric(self) -> None:
        # Flux 2.x controllers only export gotk_reconcile_duration_seconds_* and
        # gotk_token_*; the old reconcile-condition expression could never fire,
        # and its absent() branch alerts forever.
        for path in sorted(RULE_DIR.glob("prometheusrule-*.yaml")):
            for rule in rules_of(path):
                with self.subTest(rules=path.name, name=rule.get("alert") or rule.get("record")):
                    self.assertNotIn(REMOVED_FLUX_METRIC, rule["expr"])

    def test_readiness_alerts_select_the_flux_custom_resource_metric(self) -> None:
        for alert, kind in (
            ("FluxKustomizationNotReady", "Kustomization"),
            ("HelmReleaseNotReady", "HelmRelease"),
        ):
            with self.subTest(alert=alert):
                self.assertEqual(
                    normalize(self.rules[alert]["expr"]),
                    f'gotk_resource_info{{ customresource_kind="{kind}", '
                    'exported_namespace="flux-system", ready!="True", '
                    'suspended!="true" } == 1',
                )

    def test_pins_stale_covers_configmap_source_and_missing_metrics(self) -> None:
        # A healthy steady state is an empty result: the ConfigMap exists, the
        # pins source is Ready, and its series exists. Each failure mode below
        # is one branch, so it must not degrade into a permanent alert.
        branches = [
            branch.strip()
            for branch in re.split(r"\bor\b", normalize(self.rules["PinsStale"]["expr"]))
        ]
        self.assertEqual(branches, [
            'absent(kube_configmap_info{ namespace="flux-system", '
            'configmap="home-app-images" })',
            'gotk_resource_info{ customresource_kind="GitRepository", '
            'name="flux-pins", exported_namespace="flux-system", ready!="True" }',
            'absent(gotk_resource_info{ customresource_kind="GitRepository", '
            'name="flux-pins", exported_namespace="flux-system" })',
        ])

    def test_selected_labels_are_emitted_by_kube_state_metrics(self) -> None:
        # A selector on a label kube-state-metrics never emits silently disables
        # the alert (or makes absent() fire forever), so cross-check every
        # matcher against the customResourceState config.
        selectors = re.findall(
            r"gotk_resource_info\{([^}]*)\}", FLUX_RULE_FILE.read_text(encoding="utf-8")
        )
        self.assertTrue(selectors)
        for selector in selectors:
            matchers = dict(re.findall(r'(\w+)\s*!?=\s*"([^"]*)"', selector))
            kind = matchers["customresource_kind"]
            with self.subTest(selector=normalize(selector)):
                self.assertIn(kind, self.resources)
                resource = self.resources[kind]
                self.assertEqual(resource["metricNamePrefix"], "gotk")
                self.assertEqual(resource["metrics"][0]["name"], "resource_info")
                # kube-state-metrics always adds these three kind labels.
                labels = {
                    "customresource_group",
                    "customresource_kind",
                    "customresource_version",
                }
                for metric in resource["metrics"]:
                    labels |= set(metric.get("labelsFromPath", {}))
                    labels |= set(
                        metric.get("each", {}).get("info", {}).get("labelsFromPath", {})
                    )
                self.assertLessEqual(set(matchers), labels)

    def test_custom_resource_state_covers_every_flux_resource(self) -> None:
        self.assertTrue(self.ksm["customResourceState"]["enabled"])
        expected = {
            "Kustomization": ("kustomize.toolkit.fluxcd.io", "v1"),
            "HelmRelease": ("helm.toolkit.fluxcd.io", "v2"),
            "GitRepository": ("source.toolkit.fluxcd.io", "v1"),
            "HelmRepository": ("source.toolkit.fluxcd.io", "v1"),
            "OCIRepository": ("source.toolkit.fluxcd.io", "v1"),
        }
        self.assertEqual(set(self.resources), set(expected))
        for kind, (group, version) in expected.items():
            with self.subTest(kind=kind):
                gvk = self.resources[kind]["groupVersionKind"]
                self.assertEqual((gvk["group"], gvk["version"]), (group, version))

    def test_default_collectors_stay_enabled_for_kube_configmap_info(self) -> None:
        # The upstream recipe switches the pod to --custom-resource-state-only;
        # PinsStale also needs kube_configmap_info, so the default collectors
        # must keep running.
        self.assertNotIn("collectors", self.ksm)
        self.assertNotIn(
            "--custom-resource-state-only=true", self.ksm.get("extraArgs", [])
        )

    def test_rbac_allows_every_configured_flux_resource(self) -> None:
        rules = self.ksm["rbac"]["extraRules"]
        plurals = {
            "Kustomization": "kustomizations",
            "HelmRelease": "helmreleases",
            "GitRepository": "gitrepositories",
            "HelmRepository": "helmrepositories",
            "OCIRepository": "ocirepositories",
        }
        for kind, resource in self.resources.items():
            group = resource["groupVersionKind"]["group"]
            with self.subTest(kind=kind):
                matching = [rule for rule in rules if group in rule["apiGroups"]]
                self.assertTrue(matching, f"no RBAC rule for {group}")
                allowed = {
                    (api_group, plural)
                    for rule in matching
                    for api_group in rule["apiGroups"]
                    for plural in rule["resources"]
                }
                self.assertIn((group, plurals[kind]), allowed)
                self.assertTrue(
                    all(rule["verbs"] == ["list", "watch"] for rule in matching)
                )


class RuleFileSyntaxTests(unittest.TestCase):
    """`promtool check rules` over every PrometheusRule when promtool exists."""

    @unittest.skipUnless(shutil.which("promtool"), "promtool is not installed")
    def test_every_rule_file_is_valid_promql(self) -> None:
        promtool = shutil.which("promtool")
        for path in sorted(RULE_DIR.glob("prometheusrule-*.yaml")):
            with self.subTest(rules=path.name), tempfile.TemporaryDirectory() as directory:
                rendered = Path(directory) / path.name
                rendered.write_text(
                    yaml.safe_dump(
                        yaml.safe_load(path.read_text(encoding="utf-8"))["spec"],
                        sort_keys=False,
                    ),
                    encoding="utf-8",
                )
                result = subprocess.run(
                    [promtool, "check", "rules", str(rendered)],
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
