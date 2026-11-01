#!/usr/bin/env python3
"""Deploy contracts for projects/home/kubernetes/mcp-gateway.

Hermetic: no cluster, no network, no secrets. deploy.sh is exercised with a
stub kubectl that records calls and fails, so only the pre-cluster behaviour is
asserted. The catalog's schema is validated by the gateway's real loader in
`catalog_contract_test.go`; here we check the things that only the deployment
knows: catalog↔RBAC correspondence, alias unambiguity, manifest hardening and
the full placeholder rendering path.
"""
from __future__ import annotations

import os
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import yaml

HERE = Path(__file__).resolve().parent
SCRIPT = HERE / "deploy.sh"
MANIFESTS = HERE / "manifests"
CATALOG = MANIFESTS / "catalog.yaml"
RBAC = MANIFESTS / "rbac.yaml"
CI_RBAC = HERE / "ci" / "rbac.yaml"
DEPLOYMENT = MANIFESTS / "deployment.yaml"
NETWORK = MANIFESTS / "network-policies.yaml"
SERVICEMONITOR = MANIFESTS / "servicemonitor.yaml"
EXTERNAL_SECRETS = MANIFESTS / "external-secrets.yaml"

GOOD_ENV = """# private values
LAN_CIDR=192.0.2.0/24
NODE_LAN_IP="192.0.2.27"
HAOS_IP=192.0.2.28
STATION_IP=192.0.2.13
"""

ALLOWED_CATALOG_PLACEHOLDERS = {"HAOS_IP", "STATION_IP"}

# Namespaces the gateway may look into. kube-system is read-only, so it is not
# derived from the catalog (which only lists repository-managed workloads).
READ_NAMESPACES = {
    "home-assistant",
    "monitoring",
    "currency-bot",
    "openbao",
    "external-secrets",
    "traefik",
    "cert-manager",
    "kube-system",
}

INGESTED_NAMESPACES = {"monitoring", "currency-bot", "openbao", "kube-system"}


def load_docs(path: Path) -> list[dict]:
    return [doc for doc in yaml.safe_load_all(path.read_text()) if doc]


def load_catalog() -> dict:
    return yaml.safe_load(CATALOG.read_text())


def normalize(name: str) -> str:
    lowered = name.lower().replace("ё", "е").strip()
    return re.sub(r"[^0-9a-zа-я]+", " ", lowered).strip()


def roles(path: Path, name: str) -> list[dict]:
    return [
        doc
        for doc in load_docs(path)
        if doc.get("kind") == "Role" and doc.get("metadata", {}).get("name") == name
    ]


class CatalogTests(unittest.TestCase):
    def setUp(self) -> None:
        self.catalog = load_catalog()
        self.services = self.catalog["services"]

    def test_overview_is_present_and_sanitized(self):
        overview = self.catalog.get("overview", "")
        self.assertIn("Proxmox", overview)
        self.assertIn("Hermes", overview)
        # Public files must not carry real addresses or secret-looking values.
        self.assertNotRegex(overview, r"\b(?!192\.0\.2\.)\d{1,3}(?:\.\d{1,3}){3}\b")
        # No credential-looking material (hex/base64 secrets) in a public file.
        self.assertNotRegex(overview, r"\b[0-9a-fA-F]{32,}\b")

    def test_ids_are_unique(self):
        ids = [s["id"] for s in self.services]
        self.assertEqual(len(ids), len(set(ids)), "duplicate catalog ids")

    def test_aliases_are_unambiguous_across_services(self):
        owner: dict[str, str] = {}
        for service in self.services:
            for alias in service["aliases"].get("ru", []) + service["aliases"].get("en", []):
                key = normalize(alias)
                self.assertTrue(key, f"{service['id']}: empty alias")
                if key in owner and owner[key] != service["id"]:
                    self.fail(f"alias {alias!r} is claimed by {owner[key]} and {service['id']}")
                owner[key] = service["id"]

    def test_every_service_has_a_human_title(self):
        for service in self.services:
            title = service.get("title", "")
            self.assertIsInstance(title, str, service["id"])
            self.assertTrue(title.strip(), f"{service['id']}: title must be non-empty")

    def test_group_order_covers_every_service(self):
        order = self.catalog.get("group_order", [])
        self.assertTrue(order)
        self.assertEqual(len(order), len(set(order)), "duplicate group_order entries")
        for service in self.services:
            self.assertIn(service.get("group"), order, f"{service['id']}: group is not in group_order")

    def test_multi_container_workloads_name_their_main_container(self):
        # containers[0] selects the image shown in the report and the default
        # log container; it must be the real main container name of the live
        # workload (verified against the cluster), not a sidecar or a guess.
        by_id = {service["id"]: service for service in self.services}
        for service_id, container in (
            ("prometheus", "prometheus"),
            ("alertmanager", "alertmanager"),
            ("hermes", "gateway"),
            ("prometheus-operator", "kube-prometheus-stack"),
            ("external-secrets-webhook", "webhook"),
            ("cert-manager", "cert-manager-controller"),
        ):
            self.assertEqual(by_id[service_id]["containers"][0], container, service_id)

    def test_external_services_have_a_host_title(self):
        # Never print a raw address in a group header when a host name exists.
        for service in self.services:
            if service["kind"] == "external":
                self.assertTrue(service.get("host_title", "").strip(), service["id"])

    def test_every_k8s_workload_is_restartable_with_warning(self):
        k8s = [s for s in self.services if s["kind"] == "k8s"]
        external = [s for s in self.services if s["kind"] == "external"]
        self.assertTrue(k8s)
        self.assertTrue(external)
        for service in k8s:
            self.assertTrue(service["restart"]["allowed"], service["id"])
            self.assertTrue(service["restart"].get("warning", "").strip(), service["id"])
            self.assertIn(service["workload"]["kind"], {"Deployment", "StatefulSet", "DaemonSet"})
        for service in external:
            self.assertFalse(service["restart"]["allowed"], service["id"])

    def test_self_disrupting_targets(self):
        by_id = {s["id"]: s for s in self.services}
        for service_id in ("hermes", "mcp-gateway"):
            self.assertTrue(by_id[service_id]["restart"].get("self_disrupting"), service_id)

    def test_status_signals_compare(self):
        status_signals = 0
        for service in self.services:
            for signal in service.get("signals", []):
                self.assertTrue(signal.get("meaning", "").strip())
                if not signal.get("status"):
                    continue
                status_signals += 1
                self.assertRegex(
                    signal["promql"],
                    r"[<>]=?|==|!=",
                    f"{service['id']}: a status signal must describe the bad state",
                )
        self.assertGreater(status_signals, 0)

    def test_external_checks_cover_ha_ma_and_station(self):
        by_id = {service["id"]: service for service in self.services}
        for service_id in ("home-assistant", "music-assistant", "station"):
            self.assertTrue(by_id[service_id].get("checks"), service_id)
        # Every check is typed, labelled and type-complete.
        allowed = {"tcp", "http", "ha_state", "ha_template", "ha_unavailable_summary"}
        for service in self.services:
            for check in service.get("checks", []):
                self.assertIn(check["type"], allowed, service["id"])
                self.assertTrue(check.get("label", "").strip(), service["id"])
                if check["type"] == "tcp":
                    self.assertTrue(check.get("host", "").startswith("${"))
                    self.assertTrue(0 < int(check["port"]) <= 65535)
                if check["type"] == "http":
                    self.assertIn("${", check["url"])
                    self.assertIn(check.get("auth", "none"), {"none", "ha"})
                if check["type"] in {"ha_state", "ha_template"}:
                    self.assertTrue(check.get("entity_id") or check.get("template"))
        station = by_id["station"]
        station_types = {check["type"] for check in station["checks"]}
        self.assertTrue({"tcp", "ha_state", "ha_template"} <= station_types)
        # The HTTP check against HA must use the REST token, not an anonymous
        # request that would only prove the login page is reachable.
        ha_http = [check for check in by_id["home-assistant"]["checks"] if check["type"] == "http"]
        self.assertEqual(ha_http[0]["auth"], "ha")

    def test_loki_only_where_alloy_ingests(self):
        for service in self.services:
            if service["logs"]["source"] == "loki":
                self.assertIn(
                    service["namespace"],
                    INGESTED_NAMESPACES,
                    f"{service['id']}: Alloy does not ingest {service['namespace']}",
                )
                self.assertTrue(service["logs"].get("selector"))
            if service["kind"] == "external":
                self.assertEqual(service["logs"]["source"], "none")

    def test_status_signal_labels_match_workload_names(self):
        # kube-state-metrics labels a workload by its real object name. A
        # renamed object (operator-managed monitoring StatefulSets) must update
        # both the catalog workload and the PromQL in the same change.
        label_of = {"Deployment": "deployment", "StatefulSet": "statefulset", "DaemonSet": "daemonset"}
        checked = 0
        for service in self.services:
            workload = service.get("workload") or {}
            name = workload.get("name")
            if not name:
                continue
            label = label_of[workload["kind"]]
            for signal in service.get("signals", []):
                if "kube_" not in signal["promql"]:
                    continue
                checked += 1
                match = re.search(
                    rf'kube_(?:deployment|statefulset|daemonset)_[a-z_]+{{([^}}]*)}}',
                    signal["promql"],
                )
                self.assertIsNotNone(match, f"{service['id']}: unparsable kube signal")
                labels = match.group(1)
                self.assertIn(f'{label}="{name}"', labels, f"{service['id']}: workload and signal diverge")
        self.assertGreater(checked, 0)

    def test_operator_managed_statefulsets_warn_about_a_second_roll(self):
        by_id = {s["id"]: s for s in self.services}
        for service_id, name in (
            ("prometheus", "prometheus-monitoring-prometheus"),
            ("alertmanager", "alertmanager-monitoring-alertmanager"),
        ):
            service = by_id[service_id]
            self.assertEqual(service["workload"], {"kind": "StatefulSet", "name": name})
            self.assertIn("оператор", service["restart"]["warning"].lower(), service_id)


class RbacTests(unittest.TestCase):
    def setUp(self) -> None:
        self.catalog = load_catalog()
        self.read_roles = roles(RBAC, "mcp-gateway-read")
        self.restart_roles = roles(RBAC, "mcp-gateway-restart")
        self.bindings = [
            doc for doc in load_docs(RBAC) if doc.get("kind") == "RoleBinding"
        ]
        self.service_account = next(
            doc
            for doc in load_docs(RBAC)
            if doc.get("kind") == "ServiceAccount"
            and doc["metadata"]["name"] == "mcp-gateway"
        )

    def catalog_targets(self) -> dict[tuple[str, str], set[str]]:
        targets: dict[tuple[str, str], set[str]] = {}
        for service in self.catalog["services"]:
            if service["kind"] != "k8s" or not service["restart"]["allowed"]:
                continue
            key = (service["namespace"], service["workload"]["kind"])
            targets.setdefault(key, set()).add(service["workload"]["name"])
        return targets

    def rbac_targets(self) -> dict[tuple[str, str], set[str]]:
        targets: dict[tuple[str, str], set[str]] = {}
        kinds = {"deployments": "Deployment", "statefulsets": "StatefulSet", "daemonsets": "DaemonSet"}
        for role in self.restart_roles:
            namespace = role["metadata"]["namespace"]
            for rule in role["rules"]:
                for resource in rule["resources"]:
                    kind = kinds[resource]
                    targets[(namespace, kind)] = set(rule.get("resourceNames", []))
        return targets

    def test_service_account_exists(self):
        self.assertEqual(self.service_account["metadata"]["namespace"], "home-assistant")

    def test_read_role_in_every_catalog_namespace_plus_kube_system(self):
        catalog_namespaces = {
            service["namespace"] for service in self.catalog["services"] if service["kind"] == "k8s"
        }
        self.assertEqual(catalog_namespaces | {"kube-system"}, READ_NAMESPACES)
        declared = {role["metadata"]["namespace"] for role in self.read_roles}
        self.assertEqual(declared, READ_NAMESPACES)

    def test_write_resource_names_equal_catalog_restartable_workloads(self):
        # This is the security contract: RBAC cannot grant more than the catalog
        # lists, and a catalog entry without RBAC would fail at runtime.
        self.assertEqual(self.rbac_targets(), self.catalog_targets())

    def test_kube_system_is_read_only(self):
        self.assertNotIn(
            "kube-system",
            {role["metadata"]["namespace"] for role in self.restart_roles},
        )

    def test_restart_role_grants_only_patch(self):
        for role in self.restart_roles:
            for rule in role["rules"]:
                self.assertEqual(rule["verbs"], ["patch"], role["metadata"]["namespace"])
                self.assertTrue(rule.get("resourceNames"), "resourceNames are mandatory")

    def test_read_role_grants_no_write_verbs(self):
        for role in self.read_roles:
            for rule in role["rules"]:
                self.assertTrue(
                    set(rule["verbs"]) <= {"get", "list", "watch"},
                    f"{role['metadata']['namespace']} grants {rule['verbs']}",
                )

    def test_every_role_has_a_binding_for_the_gateway_sa(self):
        bound = {
            (binding["metadata"]["namespace"], binding["roleRef"]["name"])
            for binding in self.bindings
            if any(
                subject.get("name") == "mcp-gateway"
                and subject.get("namespace") == "home-assistant"
                for subject in binding["subjects"]
            )
        }
        declared = {
            (role["metadata"]["namespace"], role["metadata"]["name"])
            for role in self.read_roles + self.restart_roles
        }
        self.assertEqual(declared, bound)

    def test_ci_role_only_adds_policy_monitor_and_readiness_proxy(self):
        # home-assistant/ci-deploy already owns Services/ConfigMaps/Deployments;
        # this extension must not widen that.
        ci_roles = [doc for doc in load_docs(CI_RBAC) if doc.get("kind") == "Role"]
        self.assertEqual(len(ci_roles), 1)
        granted = {
            (rule["apiGroups"][0], resource)
            for rule in ci_roles[0]["rules"]
            for resource in rule["resources"]
        }
        self.assertEqual(
            granted,
            {
                ("networking.k8s.io", "networkpolicies"),
                ("monitoring.coreos.com", "servicemonitors"),
                ("", "services/proxy"),
            },
        )
        self.assertTrue(
            all(
                set(rule["verbs"]) <= {"get", "list", "watch", "create", "update", "patch"}
                for rule in ci_roles[0]["rules"]
            )
        )

    def test_ci_role_allows_exactly_the_deploy_home_readiness_proxy(self):
        # The smoke reads /readyz through the API-server Service proxy, which
        # authorizes `services/proxy` by the full name used in the URL. The
        # rule must cover that name and stay restricted with resourceNames.
        workflow = (
            HERE.parents[3] / ".github" / "workflows" / "deploy-home.yml"
        ).read_text()
        proxied = set(re.findall(r"services/(http:mcp-gateway:[0-9]+)/proxy/readyz", workflow))
        self.assertEqual(proxied, {"http:mcp-gateway:8080"})
        ci_role = next(doc for doc in load_docs(CI_RBAC) if doc.get("kind") == "Role")
        rule = next(
            rule for rule in ci_role["rules"] if rule["resources"] == ["services/proxy"]
        )
        self.assertEqual(rule["apiGroups"], [""])
        self.assertEqual(rule["verbs"], ["get"])
        self.assertTrue(proxied <= set(rule["resourceNames"]))


class ManifestTests(unittest.TestCase):
    def test_catalog_only_uses_renderable_placeholders(self):
        # The catalog is rendered before it becomes a ConfigMap. Only the
        # documented host placeholders may appear; anything else would fail
        # the renderer (or leak into PromQL/check URLs).
        found = set(re.findall(r"\$\{([A-Z0-9_]+)\}", CATALOG.read_text()))
        self.assertEqual(found, ALLOWED_CATALOG_PLACEHOLDERS)

    def test_deployment_is_hardened_and_single_writer(self):
        docs = load_docs(DEPLOYMENT)
        deployment = next(doc for doc in docs if doc["kind"] == "Deployment")
        spec = deployment["spec"]
        self.assertEqual(spec["replicas"], 1)
        self.assertEqual(spec["strategy"]["type"], "RollingUpdate")
        pod = spec["template"]["spec"]
        self.assertEqual(pod["serviceAccountName"], "mcp-gateway")
        self.assertTrue(pod["securityContext"]["runAsNonRoot"])
        self.assertNotEqual(pod["securityContext"].get("runAsUser"), 0)
        container = pod["containers"][0]
        self.assertTrue(container["securityContext"]["readOnlyRootFilesystem"])
        self.assertFalse(container["securityContext"]["allowPrivilegeEscalation"])
        self.assertEqual(container["securityContext"]["capabilities"]["drop"], ["ALL"])
        self.assertIn("limits", container["resources"])
        # Probes: HTTP readiness plus the distroless self-check.
        self.assertEqual(container["readinessProbe"]["httpGet"]["path"], "/readyz")
        self.assertEqual(container["livenessProbe"]["exec"]["command"][0], "/usr/local/bin/mcp-gateway")
        self.assertEqual(container["livenessProbe"]["exec"]["command"][1], "-healthcheck")

    def test_deployment_env_wiring(self):
        docs = load_docs(DEPLOYMENT)
        container = next(doc for doc in docs if doc["kind"] == "Deployment")["spec"]["template"][
            "spec"
        ]["containers"][0]
        env = {entry["name"]: entry.get("value") for entry in container["env"]}
        self.assertEqual(env["MCP_GROUPS"], "infra.read,infra.fix,infra.report")
        self.assertEqual(env["MCP_TIMEZONE"], "Asia/Almaty")
        self.assertEqual(env["MCP_AUTH_TOKEN_FILE"], "/run/secrets/mcp-gateway-auth/token")
        self.assertEqual(env["MCP_INFRA_CATALOG_FILE"], "/etc/mcp-gateway/catalog.yaml")
        self.assertEqual(env["MCP_PROMETHEUS_URL"], "http://monitoring-prometheus.monitoring.svc:9090")
        self.assertEqual(env["MCP_ALERTMANAGER_URL"], "http://monitoring-alertmanager.monitoring.svc:9093")
        self.assertEqual(env["MCP_LOKI_URL"], "http://loki.monitoring.svc:3100")
        self.assertEqual(env["MCP_HA_URL"], "http://${HAOS_IP}:8123")
        self.assertEqual(env["MCP_HA_TOKEN_FILE"], "/run/secrets/mcp-gateway-ha/token")
        self.assertEqual(env["TELEGRAM_BOT_TOKEN_FILE"], "/run/secrets/mcp-gateway-telegram/token")
        self.assertEqual(env["TELEGRAM_CHAT_ID_FILE"], "/run/secrets/mcp-gateway-telegram/chat_id")
        # Secrets only as files; no secret value and no *_TOKEN env.
        self.assertNotIn("TELEGRAM_BOT_TOKEN", env)
        self.assertNotIn("MCP_AUTH_TOKEN", env)
        self.assertNotIn("MCP_HA_TOKEN", env)
        volumes = {volume["name"]: volume for volume in container_volumes(docs)}
        self.assertEqual(volumes["catalog"]["configMap"]["name"], "mcp-gateway-catalog")
        auth_items = volumes["auth"]["secret"]["items"]
        self.assertEqual(auth_items, [{"key": "token", "path": "token"}])
        telegram_items = {(item["key"], item["path"]) for item in volumes["telegram"]["secret"]["items"]}
        self.assertEqual(telegram_items, {("token", "token"), ("chat_id", "chat_id")})
        ha_items = volumes["ha"]["secret"]["items"]
        self.assertEqual(ha_items, [{"key": "token", "path": "token"}])

    def test_service_is_cluster_ip_only(self):
        docs = load_docs(DEPLOYMENT)
        services = [doc for doc in docs if doc["kind"] == "Service"]
        self.assertEqual(len(services), 1)
        self.assertEqual(services[0]["spec"]["type"], "ClusterIP")
        self.assertEqual(services[0]["spec"]["ports"][0]["port"], 8080)
        self.assertEqual(services[0]["spec"]["ports"][0]["name"], "http")

    def test_network_policy_allows_hermes_host_network_and_monitoring(self):
        docs = load_docs(NETWORK)
        ingress = next(doc for doc in docs if doc["metadata"]["name"] == "mcp-gateway-ingress")
        sources = ingress["spec"]["ingress"][0]["from"]
        self.assertIn({"ipBlock": {"cidr": "${NODE_LAN_IP}/32"}}, sources)
        self.assertIn({"ipBlock": {"cidr": "${LAN_CIDR}"}}, sources)
        # Hermes is hostNetwork: there is no pod identity to select, so the
        # monitoring scrape comes from a namespace selector instead.
        scrape = ingress["spec"]["ingress"][1]["from"]
        self.assertEqual(
            scrape[0]["namespaceSelector"]["matchLabels"]["kubernetes.io/metadata.name"],
            "monitoring",
        )
        egress = next(doc for doc in docs if doc["metadata"]["name"] == "mcp-gateway-egress")
        ports = {
            port["port"]
            for rule in egress["spec"]["egress"]
            for port in rule["ports"]
        }
        self.assertTrue({6443, 9090, 9093, 3100, 443, 53} <= ports)
        # External catalog checks: HA REST/MA and the station's ports, all on
        # the LAN CIDR (host IPs are rendered at deploy time).
        self.assertTrue({8123, 8095, 6053, 8928, 8099} <= ports)
        lan_rule = next(
            rule
            for rule in egress["spec"]["egress"]
            if {"ipBlock": {"cidr": "${LAN_CIDR}"}} in rule["to"]
        )
        lan_ports = {port["port"] for port in lan_rule["ports"]}
        self.assertTrue({8123, 8095, 6053, 8928, 8099} <= lan_ports)

    def test_servicemonitor_selects_the_gateway_service(self):
        monitor = yaml.safe_load(SERVICEMONITOR.read_text())
        self.assertEqual(monitor["spec"]["selector"]["matchLabels"], {"app.kubernetes.io/name": "mcp-gateway"})
        self.assertEqual(monitor["spec"]["endpoints"][0]["path"], "/metrics")
        self.assertEqual(monitor["spec"]["endpoints"][0]["port"], "http")

    def test_external_secrets_read_the_expected_openbao_keys(self):
        docs = load_docs(EXTERNAL_SECRETS)
        by_name = {doc["metadata"]["name"]: doc for doc in docs}
        auth = by_name["mcp-gateway-auth"]["spec"]["data"]
        self.assertEqual(
            auth,
            [
                {
                    "secretKey": "token",
                    "remoteRef": {"key": "k8s/home-assistant/mcp-gateway-auth", "property": "token"},
                }
            ],
        )
        telegram = {
            entry["secretKey"]: entry["remoteRef"]
            for entry in by_name["mcp-gateway-telegram"]["spec"]["data"]
        }
        self.assertEqual(telegram["token"]["key"], "k8s/home-assistant/hermes-environment")
        self.assertEqual(telegram["token"]["property"], "TELEGRAM_BOT_TOKEN")
        self.assertEqual(telegram["chat_id"]["property"], "TELEGRAM_ALLOWED_USERS")
        # HA REST checks reuse the scheduler bridge's long-lived token.
        ha = by_name["mcp-gateway-ha"]["spec"]["data"]
        self.assertEqual(
            ha,
            [
                {
                    "secretKey": "token",
                    "remoteRef": {"key": "k8s/home-assistant/ha-bridge-ha", "property": "token"},
                }
            ],
        )
        for doc in docs:
            self.assertEqual(doc["spec"]["target"]["creationPolicy"], "Owner")
            self.assertEqual(doc["spec"]["target"]["deletionPolicy"], "Retain")

    def test_example_env_matches_required_keys(self):
        example = (HERE / "mcp-gateway.env.example").read_text()
        keys = {
            line.split("=", 1)[0].strip()
            for line in example.splitlines()
            if line.strip() and not line.startswith("#")
        }
        self.assertEqual(keys, {"LAN_CIDR", "NODE_LAN_IP", "HAOS_IP", "STATION_IP"})
        values = dict(
            line.split("=", 1)
            for line in example.splitlines()
            if line.strip() and not line.startswith("#")
        )
        # Placeholder values only: TEST-NET addresses, never a real LAN.
        for value in values.values():
            self.assertTrue(
                value.strip().strip('"').startswith(("192.0.2.",)),
                f"example value is not a TEST-NET placeholder: {value}",
            )


def container_volumes(docs: list[dict]) -> list[dict]:
    deployment = next(doc for doc in docs if doc["kind"] == "Deployment")
    return deployment["spec"]["template"]["spec"]["volumes"]


class RenderTests(unittest.TestCase):
    def test_render_substitutes_the_gateway_image(self):
        out = Path(tempfile.mkdtemp(prefix="mcp-render-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(out, ignore_errors=True))
        env = out / "mcp-gateway.env"
        env.write_text(GOOD_ENV)
        image = "ghcr.io/example/mcp-gateway@sha256:" + "0" * 64
        res = subprocess.run(
            [
                sys.executable,
                str(HERE / "render.py"),
                "--env",
                str(env),
                "--out",
                str(out),
                "--set",
                "BUILD_ID=test-build",
                "--set",
                f"MCP_GATEWAY_IMAGE={image}",
                str(DEPLOYMENT),
                str(NETWORK),
                str(SERVICEMONITOR),
                str(CATALOG),
            ],
            text=True,
            capture_output=True,
            timeout=60,
        )
        self.assertEqual(res.returncode, 0, res.stderr)
        rendered = (out / "deployment.yaml").read_text()
        self.assertIn(image, rendered)
        self.assertNotIn("${MCP_GATEWAY_IMAGE}", rendered)
        network = (out / "network-policies.yaml").read_text()
        self.assertIn("192.0.2.27/32", network)
        self.assertNotIn("${NODE_LAN_IP}", network)
        rendered_deployment = (out / "deployment.yaml").read_text()
        self.assertIn("http://192.0.2.28:8123", rendered_deployment)
        self.assertNotIn("${HAOS_IP}", rendered_deployment)
        catalog = (out / "catalog.yaml").read_text()
        self.assertIn("http://192.0.2.28:8123/api/", catalog)
        self.assertIn("192.0.2.13", catalog)
        self.assertNotIn("${", catalog)

    def test_unknown_placeholder_is_rejected(self):
        tmp = Path(tempfile.mkdtemp(prefix="mcp-render-bad-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(tmp, ignore_errors=True))
        env = tmp / "mcp-gateway.env"
        env.write_text(GOOD_ENV)
        bad = tmp / "bad.yaml"
        bad.write_text("kind: ConfigMap\nmetadata:\n  name: ${NOT_ALLOWED}\n")
        res = subprocess.run(
            [sys.executable, str(HERE / "render.py"), "--env", str(env), "--out", str(tmp / "out"), str(bad)],
            text=True,
            capture_output=True,
            timeout=60,
        )
        self.assertNotEqual(res.returncode, 0)
        self.assertIn("not allowed", res.stderr)


class DeployScriptTests(unittest.TestCase):
    def run_deploy(
        self,
        env_text: str,
        kubectl_script: str | None = None,
        build: bool = False,
    ) -> subprocess.CompletedProcess:
        tmp = Path(tempfile.mkdtemp(prefix="mcp-deploy-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(tmp, ignore_errors=True))
        env_file = tmp / "mcp-gateway.env"
        env_file.write_text(env_text)
        kubectl = tmp / "kubectl"
        kubectl.write_text(
            kubectl_script
            or '#!/bin/sh\necho "kubectl $*" >> "$KUBECTL_LOG"\nexit 3\n'
        )
        kubectl.chmod(0o755)
        bazel = tmp / "bazel"
        bazel.write_text('#!/bin/sh\necho "bazel $*" >> "$BAZEL_LOG"\nexit 0\n')
        bazel.chmod(0o755)
        env = os.environ.copy()
        env.pop("BAZELISK", None)
        env.update(
            {
                "PRIVATE_ENV": str(env_file),
                "KUBECTL": str(kubectl),
                "KUBECTL_LOG": str(tmp / "kubectl.log"),
                "BAZEL_LOG": str(tmp / "bazel.log"),
                "RENDER_DIR": str(tmp / "rendered"),
                "PATH": f"{tmp}:{env['PATH']}",
            }
        )
        if not build:
            env["MCP_GATEWAY_IMAGE"] = "ghcr.io/example/mcp-gateway@sha256:" + "0" * 64
        args = ["bash", str(SCRIPT)] + ([] if build else ["--skip-build"])
        res = subprocess.run(args, env=env, text=True, capture_output=True, timeout=60)
        res.kubectl_log = tmp / "kubectl.log"
        res.bazel_log = tmp / "bazel.log"
        res.rendered = tmp / "rendered"
        return res

    def test_valid_values_render_and_reach_the_cluster(self):
        res = self.run_deploy(GOOD_ENV)
        self.assertNotIn("are required", res.stderr)
        self.assertTrue((res.rendered / "deployment.yaml").is_file(), res.stderr)
        self.assertTrue(res.kubectl_log.exists(), res.stderr)

    def test_missing_network_values_are_rejected_before_kubectl(self):
        res = self.run_deploy("LAN_CIDR=192.0.2.0/24\n")
        self.assertNotEqual(res.returncode, 0)
        self.assertIn("LAN_CIDR, NODE_LAN_IP, HAOS_IP and STATION_IP are required", res.stderr)
        self.assertFalse(res.kubectl_log.exists())

    def test_unknown_only_group_is_rejected(self):
        env = os.environ.copy()
        env["ONLY"] = "scheduler"
        res = subprocess.run(
            ["bash", str(SCRIPT), "--skip-build"],
            env={**env, "PRIVATE_ENV": "/nonexistent"},
            text=True,
            capture_output=True,
            timeout=60,
        )
        self.assertNotEqual(res.returncode, 0)
        self.assertIn("selects no mcp-gateway workload", res.stderr)

    def test_missing_service_account_fails_closed(self):
        stub = """#!/bin/sh
echo "kubectl $*" >> "$KUBECTL_LOG"
case "$*" in
  "get namespace home-assistant -o name")
    echo 'Error from server (Forbidden): namespaces "home-assistant" is forbidden' >&2; exit 1 ;;
  "-n home-assistant get serviceaccount mcp-gateway")
    echo 'Error from server (NotFound): serviceaccounts "mcp-gateway" not found' >&2; exit 1 ;;
esac
exit 3
"""
        res = self.run_deploy(GOOD_ENV, stub)
        self.assertNotEqual(res.returncode, 0)
        self.assertIn("missing ServiceAccount home-assistant/mcp-gateway", res.stderr)
        log = res.kubectl_log.read_text()
        self.assertNotIn("apply -f", log)

    def test_forbidden_service_account_read_is_not_a_failure(self):
        # CI cannot read ServiceAccounts in home-assistant; the deploy must
        # continue to the ExternalSecret check instead of dying here.
        stub = """#!/bin/sh
echo "kubectl $*" >> "$KUBECTL_LOG"
case "$*" in
  "get namespace home-assistant -o name")
    echo 'Error from server (Forbidden): namespaces "home-assistant" is forbidden' >&2; exit 1 ;;
  "-n home-assistant get serviceaccount mcp-gateway")
    echo 'Error from server (Forbidden): serviceaccounts "mcp-gateway" is forbidden' >&2; exit 1 ;;
esac
exit 3
"""
        res = self.run_deploy(GOOD_ENV, stub)
        self.assertIn("assuming admin-managed home-assistant/mcp-gateway", res.stdout)
        self.assertIn("missing ExternalSecret", res.stderr)

    def test_missing_namespace_is_not_created(self):
        stub = """#!/bin/sh
echo "kubectl $*" >> "$KUBECTL_LOG"
case "$*" in
  "get namespace home-assistant -o name")
    echo 'Error from server (NotFound): namespaces "home-assistant" not found' >&2; exit 1 ;;
esac
exit 3
"""
        res = self.run_deploy(GOOD_ENV, stub)
        self.assertNotEqual(res.returncode, 0)
        self.assertIn("apply projects/home/kubernetes/scheduler/manifests/namespace.yaml", res.stderr)
        self.assertNotIn("namespace.yaml\"", res.kubectl_log.read_text())

    def test_build_invokes_the_bazel_binary_from_path(self):
        res = self.run_deploy(GOOD_ENV, build=True)
        self.assertTrue(res.bazel_log.exists(), res.stderr)
        self.assertIn(
            "build //solutions/mcp-gateway:mcp-gateway_image",
            res.bazel_log.read_text(),
        )


if __name__ == "__main__":
    unittest.main(verbosity=2)
