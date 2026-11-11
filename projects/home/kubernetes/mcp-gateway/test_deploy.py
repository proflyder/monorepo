#!/usr/bin/env python3
"""Deploy contracts for projects/home/kubernetes/mcp-gateway.

Hermetic: no cluster, no network, no secrets. Flux applies the manifests, so
these tests check them directly. The catalog's schema is validated by the
gateway's real Rust loader in
`solutions/mcp-gateway/crates/mcp-gateway/tests/deploy_contract.rs`; here we
check the things that only the deployment knows: catalog↔RBAC correspondence,
alias unambiguity, manifest hardening and the documented placeholders.
"""
from __future__ import annotations

import re
import unittest
from pathlib import Path

import yaml

HERE = Path(__file__).resolve().parent
MANIFESTS = HERE / "manifests"
CATALOG = MANIFESTS / "catalog.yaml"
BB_TARGETS = MANIFESTS / "bb-targets.yaml"
RBAC = MANIFESTS / "rbac.yaml"
DEPLOYMENT = MANIFESTS / "deployment.yaml"
NETWORK = MANIFESTS / "network-policies.yaml"
SERVICEMONITOR = MANIFESTS / "servicemonitor.yaml"
EXTERNAL_SECRETS = MANIFESTS / "external-secrets.yaml"

ALLOWED_CATALOG_PLACEHOLDERS = {"HAOS_IP", "STATION_IP", "PVE_IP", "VPN_CT_IP", "BB_CT_IP"}
ALLOWED_BB_TARGET_PLACEHOLDERS = {"BB_PROJECT_STATION_LAB", "BB_PROJECT_ENGLISH_NOTES"}

# Workloads the gateway may read but never restart: K3s' own components and
# the Flux bootstrap.
READ_ONLY_NAMESPACES = {"kube-system", "flux-system"}

INGESTED_NAMESPACES = {"monitoring", "currency-bot", "openbao", "kube-system"}


def load_docs(path: Path) -> list[dict]:
    return [doc for doc in yaml.safe_load_all(path.read_text()) if doc]


def load_catalog() -> dict:
    return yaml.safe_load(CATALOG.read_text())


def load_bb_targets() -> dict:
    return yaml.safe_load(BB_TARGETS.read_text())


def normalize(name: str) -> str:
    lowered = name.lower().replace("ё", "е").strip()
    return re.sub(r"[^0-9a-zа-я]+", " ", lowered).strip()


def roles(path: Path, name: str) -> list[dict]:
    return [
        doc
        for doc in load_docs(path)
        if doc.get("kind") == "Role" and doc.get("metadata", {}).get("name") == name
    ]


def cluster_role(path: Path, name: str) -> dict:
    return next(
        doc
        for doc in load_docs(path)
        if doc.get("kind") == "ClusterRole" and doc.get("metadata", {}).get("name") == name
    )


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
            self.assertIn(service["workload"]["kind"], {"Deployment", "StatefulSet", "DaemonSet"})
            if service["namespace"] in READ_ONLY_NAMESPACES:
                # K3s and Flux own these; the gateway must never write here.
                self.assertFalse(service["restart"]["allowed"], service["id"])
                continue
            self.assertTrue(service["restart"]["allowed"], service["id"])
            self.assertTrue(service["restart"].get("warning", "").strip(), service["id"])
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

    def test_external_checks_cover_ha_ma_station_and_platform_hosts(self):
        by_id = {service["id"]: service for service in self.services}
        for service_id in ("home-assistant", "music-assistant", "station", "proxmox", "vpn", "bb-host"):
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


class BBTargetTests(unittest.TestCase):
    def test_only_renderable_placeholders(self):
        found = set(re.findall(r"\$\{([A-Z0-9_]+)\}", BB_TARGETS.read_text()))
        self.assertEqual(found, ALLOWED_BB_TARGET_PLACEHOLDERS)

    def test_targets_have_aliases_and_a_provider(self):
        targets = load_bb_targets()["targets"]
        self.assertTrue(targets)
        seen = set()
        for entry in targets:
            self.assertTrue(entry["project"].startswith("${"))
            self.assertTrue(entry["aliases"], entry["project"])
            for alias in entry["aliases"]:
                key = normalize(alias)
                self.assertTrue(key, f"{entry['project']}: empty alias")
                self.assertNotIn(key, seen, f"duplicate alias {alias}")
                seen.add(key)
            # The personal profile is fixed; a provider pin must be explicit so
            # the gateway never silently picks a Pro model.
            self.assertEqual(entry.get("provider"), "claude-code")
            self.assertTrue(entry.get("title_prefix", "").strip())

    def test_aliases_do_not_name_work_projects(self):
        # The allowlist must not expose work project names: Hermes resolves
        # names only among personal targets, and a work alias would be
        # misleading.
        aliases = [alias.lower() for entry in load_bb_targets()["targets"] for alias in entry["aliases"]]
        for alias in aliases:
            self.assertNotIn("work", alias)
            self.assertNotIn("рабоч", alias)


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

    def test_read_grant_is_one_cluster_role_bound_to_the_gateway(self):
        role = cluster_role(RBAC, "mcp-gateway-read")
        bindings = [
            doc
            for doc in load_docs(RBAC)
            if doc.get("kind") == "ClusterRoleBinding"
            and doc.get("metadata", {}).get("name") == "mcp-gateway-read"
        ]
        self.assertEqual(len(bindings), 1)
        self.assertEqual(bindings[0]["roleRef"]["name"], role["metadata"]["name"])
        self.assertEqual(
            bindings[0]["subjects"],
            [{"kind": "ServiceAccount", "name": "mcp-gateway", "namespace": "home-assistant"}],
        )
        self.assertEqual(self.read_roles, [])

    def test_cluster_read_role_covers_workloads_pods_and_flux(self):
        role = cluster_role(RBAC, "mcp-gateway-read")
        granted = {
            (group, resource)
            for rule in role["rules"]
            for group in rule["apiGroups"]
            for resource in rule["resources"]
        }
        for resource in ("pods", "pods/log", "events", "services"):
            self.assertIn(("", resource), granted)
        for resource in ("deployments", "statefulsets", "daemonsets", "replicasets"):
            self.assertIn(("apps", resource), granted)
        self.assertIn(("kustomize.toolkit.fluxcd.io", "kustomizations"), granted)
        self.assertIn(("helm.toolkit.fluxcd.io", "helmreleases"), granted)

    def test_write_resource_names_equal_catalog_restartable_workloads(self):
        # This is the security contract: RBAC cannot grant more than the catalog
        # lists, and a catalog entry without RBAC would fail at runtime.
        self.assertEqual(self.rbac_targets(), self.catalog_targets())

    def test_platform_namespaces_are_read_only(self):
        restart_namespaces = {role["metadata"]["namespace"] for role in self.restart_roles}
        self.assertFalse(READ_ONLY_NAMESPACES & restart_namespaces)

    def test_restart_role_grants_only_patch(self):
        for role in self.restart_roles:
            for rule in role["rules"]:
                self.assertEqual(rule["verbs"], ["patch"], role["metadata"]["namespace"])
                self.assertTrue(rule.get("resourceNames"), "resourceNames are mandatory")

    def test_read_role_grants_no_write_verbs(self):
        for role in [cluster_role(RBAC, "mcp-gateway-read")]:
            for rule in role["rules"]:
                self.assertTrue(
                    set(rule["verbs"]) <= {"get", "list"},
                    f"{role['metadata']['name']} grants {rule['verbs']}",
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
            for role in self.restart_roles
        }
        self.assertEqual(declared, bound)


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
        self.assertEqual(env["MCP_GROUPS"], "infra.read,infra.fix,infra.report,bb.read,bb.act")
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
        self.assertEqual(env["MCP_BB_SSH_ADDR"], "${BB_CT_IP}:22")
        self.assertEqual(env["MCP_BB_SSH_USER"], "bb")
        self.assertEqual(env["MCP_BB_SSH_KEY_FILE"], "/run/secrets/mcp-gateway-bb-ssh/id_ed25519")
        self.assertEqual(env["MCP_BB_KNOWN_HOSTS_FILE"], "/run/secrets/mcp-gateway-bb-ssh/known_hosts")
        self.assertEqual(env["MCP_BB_TARGETS_FILE"], "/etc/mcp-gateway/bb-targets.yaml")
        self.assertEqual(env["MCP_BB_DISPATCHER_THREAD_ID"], "${BB_DISPATCHER_THREAD_ID}")
        self.assertEqual(env["MCP_VOICE_TOKEN_FILE"], "/run/secrets/mcp-gateway-voice/token")
        # Secrets only as files; no secret value and no *_TOKEN env.
        self.assertNotIn("TELEGRAM_BOT_TOKEN", env)
        self.assertNotIn("MCP_AUTH_TOKEN", env)
        self.assertNotIn("MCP_HA_TOKEN", env)
        self.assertNotIn("MCP_VOICE_TOKEN", env)
        volumes = {volume["name"]: volume for volume in container_volumes(docs)}
        self.assertEqual(volumes["catalog"]["configMap"]["name"], "mcp-gateway-catalog")
        catalog_items = {(item["key"], item["path"]) for item in volumes["catalog"]["configMap"]["items"]}
        self.assertEqual(catalog_items, {("catalog.yaml", "catalog.yaml"), ("bb-targets.yaml", "bb-targets.yaml")})
        auth_items = volumes["auth"]["secret"]["items"]
        self.assertEqual(auth_items, [{"key": "token", "path": "token"}])
        telegram_items = {(item["key"], item["path"]) for item in volumes["telegram"]["secret"]["items"]}
        self.assertEqual(telegram_items, {("token", "token"), ("chat_id", "chat_id")})
        ha_items = volumes["ha"]["secret"]["items"]
        self.assertEqual(ha_items, [{"key": "token", "path": "token"}])
        bb_items = {(item["key"], item["path"]) for item in volumes["bb-ssh"]["secret"]["items"]}
        self.assertEqual(bb_items, {("id_ed25519", "id_ed25519"), ("known_hosts", "known_hosts")})
        self.assertEqual(volumes["bb-ssh"]["secret"]["defaultMode"], 0o400)
        voice_items = volumes["voice"]["secret"]["items"]
        self.assertEqual(voice_items, [{"key": "token", "path": "token"}])

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
        self.assertTrue({6443, 9090, 9093, 3100, 443, 53, 22} <= ports)
        # External catalog checks: HA REST/MA and the station's ports, all on
        # the LAN CIDR (host IPs are rendered at deploy time).
        self.assertTrue({8123, 8095, 6053, 8928, 8099, 8006} <= ports)
        bb_rule = next(
            rule
            for rule in egress["spec"]["egress"]
            if {"ipBlock": {"cidr": "${BB_CT_IP}/32"}} in rule["to"]
        )
        self.assertEqual({port["port"] for port in bb_rule["ports"]}, {22})
        lan_rule = next(
            rule
            for rule in egress["spec"]["egress"]
            if {"ipBlock": {"cidr": "${LAN_CIDR}"}} in rule["to"]
        )
        lan_ports = {port["port"] for port in lan_rule["ports"]}
        self.assertTrue({8123, 8095, 6053, 8928, 8099, 8006} <= lan_ports)

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
        bb_ssh = {
            entry["secretKey"]: entry["remoteRef"]
            for entry in by_name["mcp-gateway-bb-ssh"]["spec"]["data"]
        }
        self.assertEqual(bb_ssh["id_ed25519"]["key"], "k8s/home-assistant/mcp-gateway-bb-ssh")
        self.assertEqual(bb_ssh["id_ed25519"]["property"], "id_ed25519")
        self.assertEqual(bb_ssh["known_hosts"]["property"], "known_hosts")
        voice = by_name["mcp-gateway-voice"]["spec"]["data"]
        self.assertEqual(
            voice,
            [
                {
                    "secretKey": "token",
                    "remoteRef": {"key": "k8s/home-assistant/mcp-gateway-voice", "property": "token"},
                }
            ],
        )
        for doc in docs:
            self.assertEqual(doc["spec"]["target"]["creationPolicy"], "Owner")
            self.assertEqual(doc["spec"]["target"]["deletionPolicy"], "Retain")


def container_volumes(docs: list[dict]) -> list[dict]:
    deployment = next(doc for doc in docs if doc["kind"] == "Deployment")
    return deployment["spec"]["template"]["spec"]["volumes"]


if __name__ == "__main__":
    unittest.main(verbosity=2)
