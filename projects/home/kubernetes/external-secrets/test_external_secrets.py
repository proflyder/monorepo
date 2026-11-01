"""Contracts for the OpenBao -> External Secrets Operator migration.

These tests are static: they protect the wiring (store, paths, ownership) so a
later edit cannot silently point a project at a different OpenBao path or let a
deploy script write an application Secret again.
"""
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
    (ROOT / "projects/home/kubernetes/ingress/manifests/external-secret-cloudflare.yaml"):
        {("cert-manager", "cloudflare-api-token"): "k8s/cert-manager/cloudflare-api-token"},
    (ROOT / "projects/home/kubernetes/observability/manifests/external-secret-grafana-admin.yaml"):
        {("monitoring", "grafana-admin"): "k8s/monitoring/grafana-admin"},
    (ROOT / "projects/home/kubernetes/currency-bot/manifests/external-secrets.yaml"):
        {("currency-bot", "currency-bot"): "k8s/currency-bot/currency-bot",
         ("currency-bot", "cloudflared-tunnel"): "k8s/currency-bot/cloudflared-tunnel"},
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

    def test_alertmanager_config_reads_only_the_bot_token_and_chat_ids(self):
        # The one deliberate cross-namespace read: Alertmanager sends through
        # the currency bot, so the bot token keeps a single source of truth.
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
        self.assertEqual(refs, {("k8s/currency-bot/currency-bot", "BOT_TOKEN"),
                                ("k8s/currency-bot/currency-bot", "CHAT_ID")})
        config = yaml.safe_load(spec["target"]["template"]["data"]["alertmanager.yaml"]
                                .replace("{{", "").replace("}}", ""))
        routes = config["route"]["routes"]
        self.assertEqual(config["route"]["receiver"], "null")
        self.assertIn('alertname =~ "Watchdog|InfoInhibitor"', routes[0]["matchers"])
        self.assertEqual(routes[0]["receiver"], "null")
        self.assertEqual(routes[1]["receiver"], "telegram")
        # The message layout comes from the mounted template file.
        raw = spec["target"]["template"]["data"]["alertmanager.yaml"]
        self.assertIn('{{ `{{ template "telegram.home.message" . }}` }}', raw)
        self.assertIn("/etc/alertmanager/configmaps/alertmanager-templates/*.tmpl", raw)
        tmpl = (ROOT / "projects/home/kubernetes/observability/alertmanager/telegram.tmpl").read_text()
        self.assertIn('{{ define "telegram.home.message" -}}', tmpl)
        values = yaml.safe_load((ROOT / "platform/observability/values/prometheus.yaml").read_text())
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
        for section in ("image", "webhook", "certController"):
            block = values["image"] if section == "image" else values[section]["image"]
            self.assertEqual(block["tag"], "__ESO_IMAGE_TAG__")

    def test_deploy_script_applies_store_and_every_manifest(self):
        text = (ESO_DIR / "deploy.sh").read_text()
        self.assertIn("clustersecretstore.yaml", text)
        for path in MANIFESTS:
            relative = path.relative_to(ROOT).as_posix()
            self.assertIn(relative, text)

    def test_scripts_do_not_imperatively_create_application_secrets(self):
        # The only allowed bootstrap is the explicit first-install path in the
        # ingress script, which is gated by --bootstrap-secrets.
        ingress = (ROOT / "projects/home/kubernetes/ingress/deploy.sh").read_text()
        self.assertIn("--bootstrap-secrets", ingress)
        self.assertRegex(ingress, r"if \[\[ \"\$BOOTSTRAP_SECRETS\" == 1 \]\]")
        for rel in ("projects/home/kubernetes/scheduler/deploy.sh",
                    "projects/home/kubernetes/apply-runtime.sh",
                    "projects/home/kubernetes/deploy-observability.sh"):
            text = (ROOT / rel).read_text()
            self.assertNotRegex(text, r"create\s+secret\s+generic")
            self.assertNotRegex(text, r"kubectl[^\n]*\bcreate\b[^\n]*\bsecret\b")

    def test_ci_verifies_eso_readiness_and_never_applies_externalsecrets(self):
        workflow = (ROOT / ".github/workflows/deploy-home.yml").read_text()
        self.assertIn("Verify External Secrets are Ready", workflow)
        self.assertIn("externalsecret/$es", workflow)
        self.assertNotIn("apply -f projects/home-assistant/deploy/manifests", workflow)
        self.assertNotIn("apply -f projects/home/kubernetes/scheduler/manifests/external-secrets", workflow)
        rbac = (ROOT / "projects/home/kubernetes/scheduler/ci/rbac.yaml").read_text()
        self.assertIn("external-secrets.io", rbac)
        self.assertIn('resources: ["externalsecrets"]', rbac)


if __name__ == "__main__":
    unittest.main()
