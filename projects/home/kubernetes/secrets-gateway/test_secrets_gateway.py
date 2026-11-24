import os
import pathlib
import re
import stat
import sys
import tempfile
import unittest
import urllib.parse

import yaml

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import totp_init


def documents(name):
    text = (HERE / "manifests" / name).read_text()
    return [doc for doc in yaml.safe_load_all(text) if doc]


class TotpSecret(unittest.TestCase):
    def test_known_base32_vector(self):
        secret = totp_init.base32_encode(b"12345678901234567890")
        expected = "".join(
            ("GEZD", "GNBV", "GY3T", "QOJQ", "GEZD", "GNBV", "GY3T", "QOJQ"))
        self.assertEqual(secret, expected)

    def test_generated_secrets_are_base32_and_not_printed(self):
        secret = totp_init.generate_secret()
        self.assertEqual(len(secret), 32)
        self.assertTrue(set(secret) <= set(totp_init.ALPHABET))
        self.assertNotEqual(secret, totp_init.generate_secret())

    def test_otpauth_uri_has_the_expected_parameters(self):
        uri = totp_init.otpauth_uri("ABC234", "Home", "owner")
        parsed = urllib.parse.urlparse(uri)
        self.assertEqual(parsed.scheme, "otpauth")
        self.assertEqual(parsed.netloc, "totp")
        self.assertEqual(parsed.path, "/Home:owner")
        query = urllib.parse.parse_qs(parsed.query)
        self.assertEqual(query["secret"], ["ABC234"])
        self.assertEqual(query["issuer"], ["Home"])
        self.assertEqual(query["digits"], ["6"])
        self.assertEqual(query["period"], ["30"])
        self.assertEqual(query["algorithm"], ["SHA1"])

    def test_plan_is_idempotent_until_rotation(self):
        updated, secret = totp_init.plan({}, rotate=False)
        self.assertIsNotNone(secret)
        again, secret_again = totp_init.plan(updated, rotate=False)
        self.assertIsNone(secret_again)
        self.assertEqual(again, updated)
        rotated, secret_rotated = totp_init.plan(updated, rotate=True)
        self.assertIsNotNone(secret_rotated)
        self.assertNotEqual(rotated["totp_secret"], updated["totp_secret"])

    def test_the_uri_file_is_private(self):
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "nested" / "totp.txt"
            totp_init.write_private(path, "otpauth://totp/x\n")
            self.assertEqual(path.read_text(), "otpauth://totp/x\n")
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)
            self.assertEqual(stat.S_IMODE(path.parent.stat().st_mode), 0o700)


class Manifests(unittest.TestCase):
    def test_service_account_and_deployment_are_unprivileged(self):
        docs = documents("secrets-gateway.yaml")
        accounts = [doc for doc in docs if doc["kind"] == "ServiceAccount"]
        deployments = [doc for doc in docs if doc["kind"] == "Deployment"]
        services = [doc for doc in docs if doc["kind"] == "Service"]
        self.assertEqual(len(accounts), 1)
        self.assertEqual(accounts[0]["metadata"]["name"], "secrets-gateway")
        deployment = deployments[0]
        spec = deployment["spec"]["template"]["spec"]
        self.assertEqual(spec["serviceAccountName"], "secrets-gateway")
        self.assertTrue(spec["automountServiceAccountToken"])
        container = spec["containers"][0]
        self.assertEqual(container["image"], "${SECRETS_GATEWAY_IMAGE}")
        self.assertFalse(container["securityContext"]["allowPrivilegeEscalation"])
        self.assertTrue(container["securityContext"]["readOnlyRootFilesystem"])
        self.assertEqual(container["securityContext"]["capabilities"]["drop"], ["ALL"])
        env = {item["name"]: item.get("value") for item in container["env"]}
        self.assertEqual(env["SECRETS_GATEWAY_TOTP_FILE"],
                         "/run/secrets/secrets-gateway/totp_secret")
        self.assertEqual(env["SECRETS_GATEWAY_SERVICE_TOKEN_FILE"],
                         "/run/secrets/secrets-gateway/service_token")
        self.assertEqual(env["SECRETS_GATEWAY_OPENBAO_ROLE"], "secrets-gateway")
        self.assertEqual(services[0]["spec"]["type"], "ClusterIP")
        self.assertEqual(services[0]["spec"]["ports"][0]["port"], 8096)

    def test_external_secret_reads_the_service_token_and_totp(self):
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
            ("k8s/home-assistant/secrets-gateway", "totp_secret"),
        })

    def test_network_policy_limits_ingress_and_egress(self):
        docs = documents("network-policies.yaml")
        policies = {doc["metadata"]["name"]: doc for doc in docs}
        ingress = policies["secrets-gateway-ingress"]["spec"]
        egress = policies["secrets-gateway-egress"]["spec"]
        self.assertEqual(ingress["policyTypes"], ["Ingress"])
        self.assertEqual(egress["policyTypes"], ["Egress"])
        ports = {
            port["port"]
            for rule in egress["egress"]
            for port in rule.get("ports", [])
        }
        self.assertEqual(ports, {53, 8095, 8098, 8200})
        rendered = yaml.safe_dump(egress)
        self.assertIn("kubernetes.io/metadata.name: openbao", rendered)
        self.assertNotIn("443", rendered)

    def test_kustomization_patches_the_image(self):
        text = (HERE / "manifests" / "kustomization.yaml").read_text()
        self.assertIn("${SECRETS_GATEWAY_IMAGE}", text)
        self.assertIn("value: ${SECRETS_GATEWAY_IMAGE}", text)


class Policy(unittest.TestCase):
    def test_policy_allows_only_kv_v2_and_denies_core_paths(self):
        text = (HERE / "policy.hcl").read_text()
        self.assertIn('path "secret/data/*"', text)
        self.assertIn('path "secret/metadata/*"', text)
        self.assertIn('path "secret/metadata"', text)
        self.assertNotIn('"destroy"', text)
        for path in ("secret/data/infra/*", "secret/data/ci/*",
                     "secret/data/k8s/flux-system/*",
                     "secret/data/k8s/home-assistant/secrets-gateway",
                     "secret/destroy/*", "sys/*"):
            with self.subTest(path=path):
                block = re.search(
                    rf'path "{re.escape(path)}" \{{(.*?)\}}', text, re.S)
                self.assertIsNotNone(block, path)
                self.assertIn('capabilities = ["deny"]', block.group(1))

    def test_policy_grants_no_delete_or_destroy_capability(self):
        text = (HERE / "policy.hcl").read_text()
        for capability in ("delete", "destroy", "sudo"):
            self.assertNotIn(f'"{capability}"', text)


if __name__ == "__main__":
    unittest.main()
