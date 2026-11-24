import json
import pathlib
import re
import stat
import sys
import tempfile
import unittest

import yaml

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[3]


def documents(name):
    text = (HERE / "manifests" / name).read_text()
    return [doc for doc in yaml.safe_load_all(text) if doc]


def deny_prefixes():
    return json.loads((HERE / "manifests" / "deny.json").read_text())["prefixes"]


def policy_rules():
    text = (HERE / "policy.hcl").read_text()
    rules = {}
    for match in re.finditer(r'path "([^"]+)" \{(.*?)\}', text, re.S):
        capabilities = re.findall(r'"([a-z]+)"', match.group(2))
        rules[match.group(1)] = set(capabilities)
    return rules


class Config(unittest.TestCase):
    def test_catalog_copy_matches_the_workstation_catalog(self):
        self.assertEqual(
            (HERE / "manifests" / "catalog.json").read_text(),
            (ROOT / "platform" / "workstation" / "catalog.json").read_text(),
        )

    def test_config_map_carries_catalog_and_deny_files(self):
        kustomization = yaml.safe_load(
            (HERE / "manifests" / "kustomization.yaml").read_text())
        generators = kustomization.get("configMapGenerator", [])
        self.assertEqual(len(generators), 1)
        self.assertEqual(generators[0]["name"], "secrets-gateway-config")
        self.assertEqual(sorted(generators[0]["files"]),
                         ["catalog.json", "deny.json"])


class Manifests(unittest.TestCase):
    def test_service_account_and_deployment_are_unprivileged(self):
        docs = documents("secrets-gateway.yaml")
        accounts = [doc for doc in docs if doc["kind"] == "ServiceAccount"]
        deployments = [doc for doc in docs if doc["kind"] == "Deployment"]
        services = [doc for doc in docs if doc["kind"] == "Service"]
        volumes = [doc for doc in docs if doc["kind"] == "PersistentVolumeClaim"]
        self.assertEqual(len(accounts), 1)
        self.assertEqual(accounts[0]["metadata"]["name"], "secrets-gateway")
        deployment = deployments[0]
        spec = deployment["spec"]["template"]["spec"]
        self.assertEqual(spec["serviceAccountName"], "secrets-gateway")
        self.assertFalse(spec["automountServiceAccountToken"])
        projected = {
            volume["name"]: volume
            for volume in spec["volumes"]
            if "projected" in volume
        }["kube-api-access"]
        token = projected["projected"]["sources"][0]["serviceAccountToken"]
        self.assertEqual(token["audience"], "secrets-gateway")
        self.assertEqual(token["path"], "token")
        container = spec["containers"][0]
        self.assertEqual(container["image"], "${SECRETS_GATEWAY_IMAGE}")
        self.assertFalse(container["securityContext"]["allowPrivilegeEscalation"])
        self.assertTrue(container["securityContext"]["readOnlyRootFilesystem"])
        self.assertEqual(container["securityContext"]["capabilities"]["drop"], ["ALL"])
        env = {item["name"]: item.get("value") for item in container["env"]}
        self.assertNotIn("SECRETS_GATEWAY_TOTP_FILE", env)
        self.assertEqual(env["SECRETS_GATEWAY_DEVICE_PATH"],
                         "k8s/home-assistant/secrets-gateway-device")
        self.assertEqual(env["SECRETS_GATEWAY_SERVICE_TOKEN_FILE"],
                         "/run/secrets/secrets-gateway/service_token")
        self.assertEqual(env["SECRETS_GATEWAY_SA_TOKEN_FILE"],
                         "/var/run/secrets/tokens/token")
        self.assertEqual(env["SECRETS_GATEWAY_OPENBAO_ROLE"], "secrets-gateway")
        self.assertEqual(env["SECRETS_GATEWAY_CATALOG_FILE"],
                         "/etc/secrets-gateway/catalog.json")
        self.assertEqual(env["SECRETS_GATEWAY_DENY_FILE"],
                         "/etc/secrets-gateway/deny.json")
        self.assertEqual(env["SECRETS_GATEWAY_STATE_FILE"],
                         "/state/step-up.json")
        self.assertEqual(services[0]["spec"]["type"], "ClusterIP")
        self.assertEqual(services[0]["spec"]["ports"][0]["port"], 8096)
        self.assertEqual(len(volumes), 1)
        self.assertEqual(volumes[0]["metadata"]["name"], "secrets-gateway-state")

    def test_external_secret_reads_only_the_service_token(self):
        docs = documents("external-secrets.yaml")
        self.assertEqual(len(docs), 1)
        spec = docs[0]["spec"]
        self.assertEqual(spec["target"]["name"], "secrets-gateway")
        self.assertEqual(spec["target"]["creationPolicy"], "Owner")
        self.assertEqual(spec["target"]["deletionPolicy"], "Retain")
        refs = {(item["remoteRef"]["key"], item["remoteRef"]["property"])
                for item in spec["data"]}
        self.assertEqual(refs, {
            ("k8s/home-assistant/scheduler-auth", "service_token"),
        })

    def test_network_policy_limits_ingress_and_egress(self):
        docs = documents("network-policies.yaml")
        policies = {doc["metadata"]["name"]: doc for doc in docs}
        ingress = policies["secrets-gateway-ingress"]["spec"]
        egress = policies["secrets-gateway-egress"]["spec"]
        self.assertEqual(ingress["policyTypes"], ["Ingress"])
        self.assertEqual(egress["policyTypes"], ["Egress"])
        sources = ingress["ingress"][0]["from"]
        self.assertTrue(any(
            source.get("podSelector", {}).get("matchLabels", {})
            .get("app.kubernetes.io/name") == "mcp-gateway"
            for source in sources
        ))
        ports = {
            port["port"]
            for rule in egress["egress"]
            for port in rule.get("ports", [])
        }
        self.assertEqual(ports, {53, 8095, 8098, 8200})
        rendered = yaml.safe_dump(egress)
        self.assertIn("kubernetes.io/metadata.name: openbao", rendered)
        self.assertNotIn("443", rendered)

    def test_kustomization_patches_the_image_and_keeps_state(self):
        text = (HERE / "manifests" / "kustomization.yaml").read_text()
        self.assertIn("${SECRETS_GATEWAY_IMAGE}", text)
        self.assertIn("value: ${SECRETS_GATEWAY_IMAGE}", text)
        self.assertIn("secrets-gateway-state", text)


class Policy(unittest.TestCase):
    def test_policy_allows_only_kv_v2_and_denies_core_paths(self):
        rules = policy_rules()
        self.assertEqual(rules["secret/data/*"], {"read", "update", "delete"})
        self.assertEqual(rules["secret/metadata/*"], {"read", "list"})
        self.assertEqual(rules["secret/metadata"], {"list"})
        expected = set()
        for prefix in deny_prefixes():
            for kind in ("data", "metadata"):
                expected.add(f"secret/{kind}/{prefix}")
                expected.add(f"secret/{kind}/{prefix}/*")
        actual = {
            path for path, capabilities in rules.items()
            if "deny" in capabilities
            and path.startswith(("secret/data/", "secret/metadata/"))
        }
        self.assertEqual(actual, expected)
        for path in ("secret/destroy/*", "secret/delete/*",
                     "secret/undelete/*", "sys/*", "auth/*", "identity/*"):
            self.assertIn("deny", rules[path], path)

    def test_policy_reaches_the_device_state_path_but_denies_the_core(self):
        rules = policy_rules()

        def matches(rule, path):
            if rule.endswith("*"):
                return path.startswith(rule[:-1])
            return rule == path

        for kind in ("data", "metadata"):
            target = f"secret/{kind}/k8s/home-assistant/secrets-gateway-device"
            denied = {
                rule for rule, capabilities in rules.items()
                if "deny" in capabilities and matches(rule, target)
            }
            self.assertEqual(denied, set(), target)
        for kind in ("data", "metadata"):
            target = f"secret/{kind}/k8s/home-assistant/secrets-gateway"
            denied = {
                rule for rule, capabilities in rules.items()
                if "deny" in capabilities and matches(rule, target)
            }
            self.assertTrue(denied, target)

    def test_policy_grants_soft_delete_only(self):
        rules = policy_rules()
        self.assertIn("delete", rules["secret/data/*"])
        for path, capabilities in rules.items():
            if path.startswith("secret/metadata"):
                self.assertNotIn("delete", capabilities, path)
        text = (HERE / "policy.hcl").read_text()
        for capability in ("destroy", "undelete", "sudo"):
            self.assertNotIn(f'"{capability}"', text)
        self.assertIn('path "secret/destroy/*"', text)
        self.assertIn('path "secret/undelete/*"', text)


if __name__ == "__main__":
    unittest.main()
