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

import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[4]
RULE_DIR = Path(__file__).with_name("rules")
RULE_FILE = RULE_DIR / "prometheusrule-backups.yaml"
ALERTMANAGER_RULE_FILE = RULE_DIR / "prometheusrule-alertmanager.yaml"
FLUX_RULE_FILE = RULE_DIR / "prometheusrule-flux.yaml"
STATION_RULE_FILE = RULE_DIR / "prometheusrule-station.yaml"
STATION_DASHBOARDS = ROOT / "projects/station/observability/chart/dashboards"
HA_RUNTIME_MONITORS = (
    ROOT / "projects/home-assistant/observability/chart/templates/monitors.yaml"
)
HERMES_CONFIG_ALERTS = {
    "HermesConfigDrift",
    "HermesConfigCheckFailing",
    "HermesConfigDriftExporterDown",
}
PULSAR_METRICS_CATALOG = (
    ROOT
    / "projects/station/pulsar/satellite/crates/pulsar-metrics/src/catalog.rs"
)
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
REQUIRED_STATION_ALERTS = {
    "StationDown",
    "StationCpuHot",
    "StationMemoryHigh",
    "StationDiskFilling",
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

    def test_external_scrape_targets_are_substituted_from_secrets(self) -> None:
        values = yaml.safe_load(
            (Path(__file__).with_name("values") / "prometheus.yaml").read_text(encoding="utf-8")
        )
        configs = {
            config["job_name"]: config
            for config in values["prometheus"]["prometheusSpec"]["additionalScrapeConfigs"]
        }
        self.assertEqual(set(configs), {"pve-node", "station"})
        # The real host:port stays out of Git.
        self.assertEqual(
            configs["pve-node"]["static_configs"][0]["targets"], ["${PVE_NODE_TARGET}"]
        )
        self.assertEqual(
            configs["station"]["static_configs"][0]["targets"], ["${STATION_IP}:9101"]
        )
        self.assertEqual(configs["station"]["metrics_path"], "/metrics")

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


class StationRuleTests(unittest.TestCase):
    """The station is an external Pulsar target, not a Kubernetes workload."""

    @classmethod
    def setUpClass(cls) -> None:
        cls.document = yaml.safe_load(STATION_RULE_FILE.read_text(encoding="utf-8"))
        cls.rules = [
            rule
            for group in cls.document["spec"]["groups"]
            for rule in group["rules"]
        ]

    def test_manifest_contract(self) -> None:
        self.assertEqual(self.document["apiVersion"], "monitoring.coreos.com/v1")
        self.assertEqual(self.document["kind"], "PrometheusRule")
        self.assertEqual(self.document["metadata"]["namespace"], "monitoring")
        self.assertEqual(self.document["metadata"]["name"], "home-station-alerts")
        kustomization = yaml.safe_load((RULE_DIR / "kustomization.yaml").read_text(encoding="utf-8"))
        self.assertIn(STATION_RULE_FILE.name, kustomization["resources"])

    def test_all_required_alerts_are_present(self) -> None:
        self.assertEqual({rule["alert"] for rule in self.rules}, REQUIRED_STATION_ALERTS)

    def test_alerts_are_warning_with_russian_telegram_annotations(self) -> None:
        for rule in self.rules:
            with self.subTest(alert=rule["alert"]):
                self.assertEqual(rule["labels"]["severity"], "warning")
                self.assertEqual(rule["labels"]["component"], "station")
                self.assertTrue(rule["annotations"]["summary"].strip())
                self.assertTrue(rule["annotations"]["description"].strip())
                self.assertNotIn("\t", rule["expr"])

    def test_thresholds_and_timers_match_the_documented_contract(self) -> None:
        by_name = {rule["alert"]: rule for rule in self.rules}
        self.assertEqual(by_name["StationDown"]["for"], "5m")
        self.assertIn('job="station"', by_name["StationDown"]["expr"])
        self.assertIn("absent(", by_name["StationDown"]["expr"])
        self.assertEqual(by_name["StationCpuHot"]["for"], "10m")
        self.assertEqual(normalize(by_name["StationCpuHot"]["expr"]),
                         "pulsar_cpu_temperature_celsius > 85")
        self.assertEqual(by_name["StationMemoryHigh"]["for"], "10m")
        self.assertEqual(
            normalize(by_name["StationMemoryHigh"]["expr"]),
            "pulsar_memory_available_bytes / pulsar_memory_total_bytes < 0.10",
        )
        disk = normalize(by_name["StationDiskFilling"]["expr"])
        self.assertIn('pulsar_filesystem_free_bytes{mount="/data"}', disk)
        self.assertIn("0.15", disk)
        self.assertIn("predict_linear(", disk)
        self.assertIn("86400", disk)


class StationMetricsContractTests(unittest.TestCase):
    """Every metric named by an alert or a dashboard exists in the Pulsar catalog."""

    @classmethod
    def setUpClass(cls) -> None:
        text = PULSAR_METRICS_CATALOG.read_text(encoding="utf-8")
        cls.catalog = set(re.findall(r'"(pulsar_[a-z0-9_]+)"', text))
        cls.referenced = set()
        sources = [STATION_RULE_FILE] + sorted(STATION_DASHBOARDS.glob("*.json"))
        for path in sources:
            cls.referenced |= set(re.findall(r"\b(pulsar_[a-z0-9_]+)\b", path.read_text(encoding="utf-8")))

    def test_catalog_is_not_empty(self) -> None:
        self.assertGreater(len(self.catalog), 30)

    def test_every_referenced_metric_is_exported(self) -> None:
        def base(name: str) -> str:
            for suffix in ("_bucket", "_sum", "_count"):
                if name.endswith(suffix):
                    return name[: -len(suffix)]
            return name

        missing = sorted(name for name in self.referenced if base(name) not in self.catalog)
        self.assertEqual(missing, [])

    def test_dashboards_are_provisioned_into_the_station_folder(self) -> None:
        chart = ROOT / "projects/station/observability/chart"
        template = (chart / "templates/dashboards.yaml").read_text(encoding="utf-8")
        self.assertIn("grafana_folder", template)
        self.assertIn("grafana_dashboard", template)
        values = yaml.safe_load((chart / "values.yaml").read_text(encoding="utf-8"))
        self.assertEqual(values["folder"], "Колонка")

    def test_dashboards_use_the_station_folder_and_prometheus(self) -> None:
        paths = sorted(STATION_DASHBOARDS.glob("*.json"))
        self.assertEqual({p.stem for p in paths}, {"station-hardware", "station-pulsar"})
        for path in paths:
            document = json.loads(path.read_text(encoding="utf-8"))
            with self.subTest(dashboard=path.name):
                self.assertTrue(document["uid"].startswith("station-"))
                self.assertIn('"uid": "prometheus"', json.dumps(document))
                self.assertTrue(document["panels"])


class HermesConfigAlertTests(unittest.TestCase):
    """Drift alerts and scrape target in the HA runtime chart."""

    @classmethod
    def setUpClass(cls) -> None:
        cls.docs = [doc for doc in yaml.safe_load_all(
            HA_RUNTIME_MONITORS.read_text(encoding="utf-8")) if doc]
        cls.rules = {
            rule["alert"]: rule
            for doc in cls.docs if doc["kind"] == "PrometheusRule"
            for group in doc["spec"]["groups"] for rule in group["rules"]
        }

    def test_drift_exporter_is_scraped(self) -> None:
        monitors = [doc for doc in self.docs if doc["kind"] == "ServiceMonitor"]
        monitor = next(doc for doc in monitors
                       if doc["metadata"]["name"] == "hermes-config-drift")
        self.assertEqual(monitor["spec"]["namespaceSelector"],
                         {"matchNames": ["home-assistant"]})
        self.assertEqual(monitor["spec"]["selector"]["matchLabels"],
                         {"app.kubernetes.io/name": "hermes-config-drift"})
        self.assertEqual(monitor["spec"]["endpoints"], [{
            "port": "metrics", "honorLabels": True, "interval": "60s",
        }])

    def test_required_alerts_are_present(self) -> None:
        self.assertTrue(HERMES_CONFIG_ALERTS.issubset(self.rules))

    def test_drift_alerts_are_warnings_with_russian_text(self) -> None:
        for name in HERMES_CONFIG_ALERTS:
            with self.subTest(alert=name):
                rule = self.rules[name]
                self.assertEqual(rule["labels"]["severity"], "warning")
                self.assertEqual(rule["labels"]["component"], "hermes")
                self.assertEqual(rule["for"], "15m")
                self.assertRegex(rule["annotations"]["summary"], r"[А-Яа-я]")
                self.assertRegex(rule["annotations"]["description"], r"[А-Яа-я]")

    def test_expressions_match_the_exporter_contract(self) -> None:
        self.assertEqual(normalize(self.rules["HermesConfigDrift"]["expr"]),
                         "max(hermes_config_drift_files) > 0")
        self.assertEqual(normalize(self.rules["HermesConfigCheckFailing"]["expr"]),
                         "min(hermes_config_check_success) == 0")
        self.assertEqual(
            normalize(self.rules["HermesConfigDriftExporterDown"]["expr"]),
            "absent(hermes_config_check_success) or "
            "(time() - max(hermes_config_last_check_timestamp_seconds)) > 1200",
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
