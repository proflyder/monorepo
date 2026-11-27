import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

import yaml

CHART = Path(__file__).resolve().parent
HERMES = CHART.parent
WORK = HERMES / "work"
ROOT = HERMES.parents[2]
IMAGE = "ghcr.io/proflyder/hermes@sha256:" + "a" * 64
SMOKE = "ghcr.io/proflyder/home-deploy@sha256:" + "b" * 64


def work_values():
    return yaml.safe_load((WORK / "values.yaml").read_text())


def render(values, instance="work"):
    with tempfile.NamedTemporaryFile(mode="w", suffix=".yaml") as handle:
        yaml.safe_dump(values, handle)
        handle.flush()
        result = subprocess.run(
            ["helm", "template", f"hermes-{instance}", str(CHART), "-f", handle.name,
             "--set", f"instance={instance}", "--set", f"image={IMAGE}",
             "--set", f"smoke.image={SMOKE}"],
            capture_output=True, text=True)
    return result


def documents(values, instance="work"):
    result = render(values, instance)
    if result.returncode:
        raise AssertionError(result.stderr)
    return [doc for doc in yaml.safe_load_all(result.stdout) if doc]


def kustomize(path):
    result = subprocess.run(["kubectl", "kustomize", str(path)], check=True,
                            capture_output=True, text=True)
    text = result.stdout.replace("${HERMES_INSTANCE}", "work")
    return [doc for doc in yaml.safe_load_all(text) if doc]


def one(docs, kind, name=None):
    found = [doc for doc in docs if doc["kind"] == kind
             and (name is None or doc["metadata"]["name"] == name)]
    if len(found) != 1:
        raise AssertionError(f"{kind}/{name}: {len(found)}")
    return found[0]


@unittest.skipUnless(shutil.which("helm"), "Helm is required for chart contracts")
class HermesChart(unittest.TestCase):
    def test_names_derive_from_the_instance(self):
        docs = documents(work_values())
        deployment = one(docs, "Deployment")
        self.assertEqual(deployment["metadata"]["name"], "hermes-work")
        claim = one(docs, "PersistentVolumeClaim")
        self.assertEqual(claim["metadata"]["name"], "hermes-work-data")
        self.assertEqual(claim["spec"]["storageClassName"], "local-retain")
        self.assertEqual(claim["metadata"]["annotations"]["helm.sh/resource-policy"], "keep")
        other = documents(work_values(), instance="other")
        self.assertEqual(one(other, "Deployment")["metadata"]["name"], "hermes-other")

    def test_work_pod_is_isolated_single_writer(self):
        pod = one(documents(work_values()), "Deployment")
        spec = pod["spec"]["template"]["spec"]
        self.assertEqual(pod["spec"]["strategy"]["type"], "Recreate")
        self.assertEqual(pod["spec"]["replicas"], 1)
        self.assertFalse(spec["hostNetwork"])
        self.assertFalse(spec["automountServiceAccountToken"])
        self.assertTrue(spec["securityContext"]["runAsNonRoot"])
        gateway = spec["containers"][0]
        self.assertEqual(gateway["image"], IMAGE)
        env = {item["name"]: item for item in gateway["env"]}
        self.assertEqual(env["TELEGRAM_ALLOWED_USERS"]["valueFrom"]["secretKeyRef"]["name"],
                         "hermes-telegram")
        self.assertEqual(env["CLAUDE_CODE_OAUTH_TOKEN"]["valueFrom"]["secretKeyRef"]["name"],
                         "hermes-anthropic")
        for forbidden in ("GATEWAY_ALLOW_ALL_USERS", "TELEGRAM_GROUP_ALLOWED_USERS",
                          "TELEGRAM_GROUP_ALLOWED_CHATS", "TELEGRAM_GUEST_MODE",
                          "HERMES_HA_TOKEN", "HERMES_MA_TOKEN"):
            self.assertNotIn(forbidden, env)

    def test_config_answers_only_the_owner_and_has_no_home_access(self):
        config_map = one(documents(work_values()), "ConfigMap")
        config = yaml.safe_load(config_map["data"]["config.yaml"])
        self.assertEqual(config["unauthorized_dm_behavior"], "ignore")
        telegram = config["platforms"]["telegram"]
        self.assertEqual(telegram["extra"]["unauthorized_dm_behavior"], "ignore")
        self.assertEqual(config["model"]["provider"], "anthropic")
        self.assertFalse(config["auth"]["adopt_external_logins"])
        self.assertNotIn("mcp_servers", config)
        text = config_map["data"]["config.yaml"]
        for marker in ("homeassistant", "music-assistant", "8123", "pairing"):
            self.assertNotIn(marker, text)

    def test_owner_only_guards_fail_the_render(self):
        cases = {
            "pair": lambda v: v["config"].update(unauthorized_dm_behavior="pair"),
            "missing": lambda v: v["config"].pop("unauthorized_dm_behavior"),
            "telegram": lambda v: v["config"]["platforms"]["telegram"]["extra"].update(
                unauthorized_dm_behavior="pair"),
            "allow-all": lambda v: v.update(env={"GATEWAY_ALLOW_ALL_USERS": "true"}),
            "group": lambda v: v["secretEnv"].append(
                {"name": "TELEGRAM_GROUP_ALLOWED_CHATS", "secret": "s", "key": "k"}),
            "no-owner": lambda v: v.update(secretEnv=[
                item for item in v["secretEnv"] if item["name"] != "TELEGRAM_ALLOWED_USERS"]),
        }
        for name, mutate in cases.items():
            with self.subTest(case=name):
                values = work_values()
                mutate(values)
                self.assertNotEqual(render(values).returncode, 0)

    def test_network_policy_denies_by_default_and_allows_public_https(self):
        docs = documents(work_values())
        policies = {doc["metadata"]["name"]: doc["spec"] for doc in docs
                    if doc["kind"] == "NetworkPolicy"}
        deny = policies["hermes-work-default-deny"]
        self.assertEqual(deny["podSelector"], {})
        self.assertEqual(set(deny["policyTypes"]), {"Ingress", "Egress"})
        self.assertNotIn("ingress", deny)
        self.assertNotIn("egress", deny)
        gateway = policies["hermes-work-gateway"]
        self.assertEqual(len(gateway["egress"]), 1)
        block = gateway["egress"][0]["to"][0]["ipBlock"]
        self.assertEqual(block["cidr"], "0.0.0.0/0")
        self.assertIn("192.168.0.0/16", block["except"])
        self.assertIn("10.0.0.0/8", block["except"])
        self.assertEqual(gateway["egress"][0]["ports"], [{"protocol": "TCP", "port": 443}])
        source = gateway["ingress"][0]["from"][0]["podSelector"]["matchLabels"]
        self.assertEqual(source["app.kubernetes.io/name"], "hermes-smoke")
        dns = policies["hermes-work-dns"]["egress"][0]
        self.assertEqual({port["port"] for port in dns["ports"]}, {53})

    def test_mcp_gateway_egress_is_opt_in(self):
        values = work_values()
        values["networkPolicy"] = {"mcpGateway": {
            "enabled": True, "namespace": "hermes-work",
            "podLabels": {"app.kubernetes.io/name": "mcp-gateway"}, "port": 8080}}
        policy = one(documents(values), "NetworkPolicy", "hermes-work-gateway")
        self.assertEqual(len(policy["spec"]["egress"]), 2)
        self.assertEqual(policy["spec"]["egress"][1]["ports"], [{"protocol": "TCP", "port": 8080}])

    def test_smoke_calls_the_instance_api_not_home_assistant(self):
        job = one(documents(work_values()), "Job")
        container = job["spec"]["template"]["spec"]["containers"][0]
        command = container["command"]
        self.assertEqual(command[:3], ["/usr/local/bin/home-deploy", "hermes", "api-smoke"])
        self.assertEqual(command[command.index("--url") + 1], "http://hermes-work:8642")
        self.assertNotIn("HA_URL", str(container))
        volume = job["spec"]["template"]["spec"]["volumes"][0]["secret"]
        self.assertEqual(volume["secretName"], "hermes-api")
        self.assertEqual(job["spec"]["template"]["metadata"]["labels"]["app.kubernetes.io/instance"],
                         "hermes-work")


@unittest.skipUnless(shutil.which("kubectl"), "kubectl kustomize is required")
class WorkOverlay(unittest.TestCase):
    def setUp(self):
        self.docs = kustomize(WORK)

    def test_secrets_come_from_the_namespaced_store_and_own_path(self):
        store = one(self.docs, "SecretStore")
        self.assertEqual(store["metadata"]["namespace"], "hermes-work")
        auth = store["spec"]["provider"]["vault"]["auth"]["kubernetes"]
        self.assertEqual(auth["role"], "hermes-work-eso")
        self.assertEqual(auth["serviceAccountRef"], {"name": "hermes-secrets"})
        secrets = [doc for doc in self.docs if doc["kind"] == "ExternalSecret"]
        self.assertEqual({doc["metadata"]["name"] for doc in secrets},
                         {"hermes-telegram", "hermes-api", "hermes-anthropic"})
        for doc in secrets:
            self.assertEqual(doc["metadata"]["namespace"], "hermes-work")
            self.assertEqual(doc["spec"]["secretStoreRef"], {"name": "openbao", "kind": "SecretStore"})
            for item in doc["spec"]["data"]:
                self.assertTrue(item["remoteRef"]["key"].startswith("k8s/hermes-work/"))

    def test_release_targets_the_instance_namespace(self):
        release = one(self.docs, "HelmRelease")
        spec = release["spec"]
        self.assertEqual(release["metadata"]["name"], "hermes-work")
        self.assertEqual(spec["targetNamespace"], "hermes-work")
        self.assertEqual(spec["chart"]["spec"]["chart"], "./projects/home-assistant/hermes/chart")
        self.assertEqual(spec["values"]["instance"], "work")
        self.assertEqual(spec["upgrade"]["remediation"]["strategy"], "rollback")
        values = one(self.docs, "ConfigMap", "hermes-work-values")
        self.assertEqual(values["metadata"]["namespace"], "flux-system")
        self.assertEqual(yaml.safe_load(values["data"]["values.yaml"]), work_values())

    def test_namespace_enforces_restricted_pod_security(self):
        namespace = one(self.docs, "Namespace")
        self.assertEqual(namespace["metadata"]["name"], "hermes-work")
        self.assertEqual(namespace["metadata"]["labels"]["pod-security.kubernetes.io/enforce"],
                         "restricted")

    def test_values_have_no_flux_substitution_markers(self):
        self.assertNotIn("${", (WORK / "values.yaml").read_text())


if __name__ == "__main__":
    unittest.main()
