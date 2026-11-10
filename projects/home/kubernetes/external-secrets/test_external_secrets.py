"""Contracts for the OpenBao -> External Secrets Operator migration.

These tests are static: they protect the wiring (store, paths, ownership) so a
later edit cannot silently point a project at a different OpenBao path or let a
deploy script write an application Secret again.
"""
import re
import unittest
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[4]
ESO_DIR = ROOT / "projects/home/kubernetes/external-secrets"
MANIFESTS = {
    (ROOT / "projects/home/kubernetes/scheduler/manifests/external-secrets.yaml"):
        {("home-assistant", "scheduler-auth"): "k8s/home-assistant/scheduler-auth",
         ("home-assistant", "ha-bridge-ha"): "k8s/home-assistant/ha-bridge-ha"},
    (ROOT / "projects/home-assistant/deploy/manifests/external-secrets.yaml"):
        {("home-assistant", "hermes-environment"): "k8s/home-assistant/hermes-environment"},
    (ROOT / "projects/home/kubernetes/ingress/routes/external-secret-cloudflare.yaml"):
        {("cert-manager", "cloudflare-api-token"): "k8s/cert-manager/cloudflare-api-token"},
    (ROOT / "projects/home/kubernetes/observability/manifests/external-secret-grafana-admin.yaml"):
        {("monitoring", "grafana-admin"): "k8s/monitoring/grafana-admin"},
    (ROOT / "projects/home/kubernetes/currency-bot/manifests/external-secrets.yaml"):
        {("currency-bot", "currency-bot"): "k8s/currency-bot/currency-bot",
         ("currency-bot", "cloudflared-tunnel"): "k8s/currency-bot/cloudflared-tunnel"},
    (ROOT / "projects/home/kubernetes/wakegrant/manifests/external-secrets.yaml"):
        {("home-assistant", "wakegrant"): "k8s/home-assistant/wakegrant"},
    (ROOT / "projects/home/kubernetes/vaultwarden/manifests/external-secrets.yaml"):
        {("vaultwarden", "vaultwarden-admin"): "k8s/vaultwarden/vaultwarden-admin"},
}


class ExternalSecretsContracts(unittest.TestCase):
    def test_every_external_secret_targets_openbao_with_safe_ownership(self):
        for path, expected in MANIFESTS.items():
            with self.subTest(path=path.name):
                docs = [d for d in yaml.safe_load_all(path.read_text()) if d]
                found = {}
                for doc in docs:
                    self.assertEqual(doc["kind"], "ExternalSecret")
                    self.assertEqual(doc["apiVersion"], "external-secrets.io/v1")
                    target = doc["spec"]["target"]
                    ns = doc["metadata"]["namespace"]
                    name = doc["metadata"]["name"]
                    # Same name/namespace/keys: consumers need no change.
                    self.assertEqual(target["name"], name)
                    self.assertEqual(doc["spec"]["secretStoreRef"]["name"], "openbao")
                    self.assertEqual(doc["spec"]["secretStoreRef"]["kind"], "ClusterSecretStore")
                    self.assertEqual(doc["spec"]["refreshInterval"], "1h")
                    # Adopt the existing Secret in place; keep it on removal.
                    self.assertEqual(target["creationPolicy"], "Owner")
                    self.assertEqual(target["deletionPolicy"], "Retain")
                    extract = doc["spec"]["dataFrom"][0]["extract"]["key"]
                    # One OpenBao entry per Secret: k8s/<namespace>/<name>.
                    self.assertEqual(extract, f"k8s/{ns}/{name}")
                    found[(ns, name)] = extract
                self.assertEqual(found, expected)

    def test_alertmanager_config_reads_the_hermes_bot_and_owner_chat(self):
        # The one deliberate cross-namespace read: Alertmanager alerts and
        # Hermes chat share one dedicated bot, so both the token and the owner
        # chat id come from k8s/home-assistant/hermes-environment.
        path = ROOT / "projects/home/kubernetes/observability/manifests/external-secret-alertmanager-telegram.yaml"
        docs = [d for d in yaml.safe_load_all(path.read_text()) if d]
        self.assertEqual(len(docs), 1)
        spec = docs[0]["spec"]
        self.assertEqual(docs[0]["metadata"]["namespace"], "monitoring")
        self.assertEqual(spec["secretStoreRef"], {"name": "openbao", "kind": "ClusterSecretStore"})
        self.assertEqual(spec["target"]["creationPolicy"], "Owner")
        self.assertEqual(spec["target"]["deletionPolicy"], "Retain")
        self.assertNotIn("dataFrom", spec)
        refs = {(d["remoteRef"]["key"], d["remoteRef"]["property"]) for d in spec["data"]}
        self.assertEqual(refs, {("k8s/home-assistant/hermes-environment", "TELEGRAM_BOT_TOKEN"),
                                ("k8s/home-assistant/hermes-environment", "TELEGRAM_ALLOWED_USERS"),
                                ("k8s/monitoring/heartbeat", "url"),
                                ("k8s/monitoring/heartbeat", "token")})
        config = yaml.safe_load(spec["target"]["template"]["data"]["alertmanager.yaml"]
                                .replace("{{", "").replace("}}", ""))
        routes = config["route"]["routes"]
        self.assertEqual(config["route"]["receiver"], "null")
        # Watchdog goes to the external dead-man switch, not to Telegram.
        self.assertEqual(routes[0]["receiver"], "heartbeat")
        self.assertEqual(routes[0]["matchers"], ['alertname = "Watchdog"'])
        self.assertEqual(routes[0]["group_wait"], "0s")
        self.assertEqual(routes[0]["group_interval"], "1m")
        self.assertEqual(routes[0]["repeat_interval"], "5m")
        # InfoInhibitor stays suppressed; it must not reach the heartbeat.
        self.assertEqual(routes[1]["receiver"], "null")
        self.assertEqual(routes[1]["matchers"], ['alertname = "InfoInhibitor"'])
        self.assertEqual(routes[2]["receiver"], "telegram")
        receivers = {receiver["name"]: receiver for receiver in config["receivers"]}
        webhook = receivers["heartbeat"]["webhook_configs"][0]
        self.assertIs(webhook["send_resolved"], False)
        # A bounded attempt lets Alertmanager retry within group_interval; the
        # 0.34 http_config has no keep-alive switch, so this is the mitigation.
        self.assertEqual(webhook["timeout"], "10s")
        self.assertNotIn("keep_alive", webhook["http_config"])
        self.assertEqual(webhook["http_config"]["authorization"]["type"], "Bearer")
        raw = spec["target"]["template"]["data"]["alertmanager.yaml"]
        self.assertIn("{{ .heartbeat_url | trim }}", raw)
        self.assertIn("{{ .heartbeat_token | trim }}", raw)
        self.assertNotIn('Watchdog|InfoInhibitor', raw)
        # The message layout comes from the mounted template file.
        self.assertIn('{{ `{{ template "telegram.home.message" . }}` }}', raw)
        self.assertIn("/etc/alertmanager/configmaps/alertmanager-templates/*.tmpl", raw)
        # The owner id is the first allow-list entry and doubles as the chat id.
        self.assertIn('index (splitList "," .chat_ids) 0', raw)
        tmpl = (ROOT / "projects/home/kubernetes/observability/alertmanager/telegram.tmpl").read_text()
        self.assertIn('{{ define "telegram.home.message" -}}', tmpl)
        # Each alert carries a fingerprint prefix so a Telegram reply can be
        # resolved by mcp-gateway's infra_alert.
        self.assertIn('{{ .Fingerprint | reReplaceAll "^(.{12}).*$" "$1" }}', tmpl)
        self.assertIn("ответьте на это сообщение", tmpl)
        values = yaml.safe_load(
            (ROOT / "projects/home/kubernetes/observability/values/prometheus.yaml").read_text()
        )
        self.assertEqual(values["alertmanager"]["alertmanagerSpec"]["configMaps"],
                         ["alertmanager-templates"])

    def test_cluster_secret_store_uses_in_cluster_service_and_kubernetes_auth(self):
        docs = [d for d in yaml.safe_load_all(
            (ESO_DIR / "manifests/clustersecretstore.yaml").read_text()) if d]
        self.assertEqual(len(docs), 1)
        store = docs[0]
        self.assertEqual(store["kind"], "ClusterSecretStore")
        provider = store["spec"]["provider"]["vault"]
        self.assertEqual(provider["server"], "http://openbao.openbao.svc:8200")
        self.assertEqual(provider["path"], "secret")
        self.assertEqual(provider["version"], "v2")
        auth = provider["auth"]["kubernetes"]
        self.assertEqual(auth["mountPath"], "kubernetes")
        self.assertEqual(auth["role"], "external-secrets")
        self.assertEqual(auth["serviceAccountRef"],
                         {"name": "external-secrets", "namespace": "external-secrets"})

    def test_versions_are_pinned_by_digest(self):
        versions = (ROOT / "projects/home/kubernetes/versions.sh").read_text()
        for key in ("EXTERNAL_SECRETS_CHART_VERSION", "EXTERNAL_SECRETS_IMAGE_VERSION",
                    "EXTERNAL_SECRETS_IMAGE_DIGEST"):
            self.assertRegex(versions, rf"(?m)^{key}=\S+$")
        self.assertRegex(versions, r"(?m)^EXTERNAL_SECRETS_IMAGE_DIGEST=sha256:[0-9a-f]{64}$")

    def test_eso_values_pin_the_image_and_keep_one_replica(self):
        values = yaml.safe_load((ESO_DIR / "values/external-secrets.yaml").read_text())
        self.assertTrue(values["installCRDs"])
        self.assertEqual(values["replicaCount"], 1)
        self.assertEqual(values["serviceAccount"]["name"], "external-secrets")
        versions = (ROOT / "projects/home/kubernetes/versions.sh").read_text()
        tag = re.search(r"(?m)^EXTERNAL_SECRETS_IMAGE_VERSION=(\S+)$", versions).group(1)
        digest = re.search(r"(?m)^EXTERNAL_SECRETS_IMAGE_DIGEST=(sha256:[0-9a-f]{64})$",
                           versions).group(1)
        for section in ("image", "webhook", "certController"):
            block = values["image"] if section == "image" else values[section]["image"]
            # The literal in the values file must match the reviewed pin.
            self.assertEqual(block["tag"], f"{tag}@{digest}")

    def test_operator_and_store_are_owned_by_flux(self):
        kustomization = yaml.safe_load((ESO_DIR / "kustomization.yaml").read_text())
        resources = set(kustomization["resources"])
        self.assertEqual(resources, {
            "manifests/namespace.yaml",
            "manifests/helmrepository.yaml",
            "manifests/helmrelease.yaml",
            "manifests/clustersecretstore.yaml",
        })
        release = yaml.safe_load((ESO_DIR / "manifests/helmrelease.yaml").read_text())
        self.assertEqual(release["metadata"]["name"], "external-secrets")
        self.assertEqual(release["spec"]["targetNamespace"], "external-secrets")
        self.assertEqual(release["spec"]["chart"]["spec"]["version"],
                         re.search(r"(?m)^EXTERNAL_SECRETS_CHART_VERSION=(\S+)$",
                                   (ROOT / "projects/home/kubernetes/versions.sh").read_text()).group(1))

    def test_deploy_script_only_applies_application_secrets(self):
        # The operator, the store and infrastructure ExternalSecrets are
        # Flux-owned; the manual script must not touch them any more.
        text = (ESO_DIR / "deploy.sh").read_text()
        self.assertNotIn("helm upgrade", text)
        self.assertNotIn("clustersecretstore.yaml", text)
        for path in MANIFESTS:
            if path.name.startswith("external-secret-cloudflare"):
                continue
            relative = path.relative_to(ROOT).as_posix()
            if "vaultwarden" in relative or "observability" in relative or "ingress" in relative:
                self.assertNotIn(relative, text)
                continue
            self.assertIn(relative, text)

    def test_scripts_do_not_imperatively_create_application_secrets(self):
        for rel in ("projects/home/kubernetes/scheduler/deploy.sh",
                    "projects/home/kubernetes/apply-runtime.sh",
                    "projects/home/kubernetes/external-secrets/deploy.sh",
                    "projects/home/kubernetes/vaultwarden/sync_openbao.py"):
            text = (ROOT / rel).read_text()
            self.assertNotRegex(text, r"create\s+secret\s+generic")
            self.assertNotRegex(text, r"kubectl[^\n]*\bcreate\b[^\n]*\bsecret\b")

    def test_infrastructure_secrets_are_declared_next_to_their_flux_unit(self):
        # The infra ExternalSecrets are applied by Flux, not by deploy.sh.
        for rel in ("projects/home/kubernetes/ingress/routes/external-secret-cloudflare.yaml",
                    "projects/home/kubernetes/observability/manifests/external-secret-grafana-admin.yaml",
                    "projects/home/kubernetes/observability/manifests/external-secret-alertmanager-telegram.yaml",
                    "projects/home/kubernetes/vaultwarden/manifests/external-secrets.yaml"):
            self.assertTrue((ROOT / rel).is_file(), rel)

    def test_flux_owns_externalsecrets_and_ci_never_applies_them(self):
        # `deploy-home` no longer applies manifests at all: it only pins the
        # image digests, so ESO readiness is a Flux health check now.
        workflow = (ROOT / ".github/workflows/deploy-home.yml").read_text()
        self.assertNotIn("Verify External Secrets are Ready", workflow)
        self.assertNotIn("environment: Home", workflow)
        self.assertNotIn("secret/ci/deploy-home", workflow)
        self.assertNotIn("export KUBECONFIG", workflow)
        self.assertNotIn("apply -f projects/home-assistant/deploy/manifests", workflow)
        self.assertNotIn("apply -f projects/home/kubernetes/scheduler/manifests/external-secrets", workflow)


if __name__ == "__main__":
    unittest.main()
