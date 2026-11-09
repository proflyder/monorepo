"""Static contracts for the Traefik/cert-manager ingress templates.

No cluster and no rendering binary are needed: these checks guard the public
template shape that the private deploy renders.
"""
import pathlib
import unittest

import yaml

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[3]
MANIFESTS = HERE / "manifests"


def load_all(path):
    return [doc for doc in yaml.safe_load_all(path.read_text(encoding="utf-8")) if doc]


def by_name(documents, kind, name):
    for document in documents:
        if document.get("kind") == kind and document.get("metadata", {}).get("name") == name:
            return document
    raise AssertionError(f"{kind}/{name} not found")


class IntercomRoute(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.route = by_name(load_all(MANIFESTS / "routes.yaml"), "IngressRoute", "intercom")

    def test_host_follows_home_naming(self):
        rule = self.route["spec"]["routes"][0]
        self.assertEqual(rule["match"], "Host(`${INTERCOM_HOST}`)")
        self.assertEqual(rule["middlewares"], [{"name": "home-allowlist"}])

    def test_targets_the_intercom_service(self):
        service = self.route["spec"]["routes"][0]["services"][0]
        self.assertEqual(service["name"], "intercom")
        self.assertEqual(service["namespace"], "home-assistant")
        self.assertEqual(service["port"], 8099)

    def test_streaming_response_is_flushed(self):
        service = self.route["spec"]["routes"][0]["services"][0]
        self.assertEqual(service["responseForwarding"]["flushInterval"], "100ms")

    def test_uses_the_wildcard_certificate(self):
        self.assertEqual(self.route["spec"]["tls"]["secretName"], "home-wildcard-tls")


class StreamingTimeouts(unittest.TestCase):
    def test_websecure_entrypoint_allows_long_lived_streams(self):
        values = yaml.safe_load((HERE / "values" / "traefik.yaml").read_text(encoding="utf-8"))
        timeouts = values["ports"]["websecure"]["transport"]["respondingTimeouts"]
        self.assertEqual(timeouts["writeTimeout"], "0s")
        self.assertEqual(timeouts["idleTimeout"], "1800s")
        self.assertIn("readTimeout", timeouts)


class IntercomNetworkPolicy(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.policy = by_name(
            load_all(MANIFESTS / "intercom-networkpolicy.yaml"), "NetworkPolicy", "intercom-ingress"
        )

    def test_selects_the_intercom_pod(self):
        self.assertEqual(self.policy["metadata"]["namespace"], "home-assistant")
        self.assertEqual(
            self.policy["spec"]["podSelector"]["matchLabels"]["app.kubernetes.io/name"], "intercom"
        )

    def test_allows_traefik_on_the_node_and_the_pod_network(self):
        sources = self.policy["spec"]["ingress"][0]["from"]
        cidrs = {source["ipBlock"]["cidr"] for source in sources}
        self.assertEqual(cidrs, {"${LAN_CIDR}", "10.42.0.0/16"})
        ports = self.policy["spec"]["ingress"][0]["ports"]
        self.assertEqual(ports, [{"protocol": "TCP", "port": 8099}])

    def test_nodeport_fallback_is_kept(self):
        service = by_name(
            load_all(ROOT / "projects/home/kubernetes/intercom/manifests/intercom.yaml"),
            "Service",
            "intercom",
        )
        self.assertEqual(service["spec"]["type"], "NodePort")
        self.assertEqual(service["spec"]["ports"][0]["nodePort"], "${INTERCOM_NODE_PORT}")


class DeployWiring(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.deploy = (HERE / "deploy.sh").read_text(encoding="utf-8")
        cls.verify = (HERE / "verify.sh").read_text(encoding="utf-8")

    def test_intercom_host_is_rendered(self):
        self.assertIn('INTERCOM_HOST="intercom.$HOME_DOMAIN"', self.deploy)
        self.assertIn('--set "INTERCOM_HOST=$INTERCOM_HOST"', self.deploy)

    def test_network_policy_is_applied(self):
        self.assertIn("intercom-networkpolicy.yaml", self.deploy)

    def test_dns_records_are_created_for_intercom(self):
        self.assertIn("cloudflare_dns.py", self.deploy)
        self.assertIn('"$CURRENCY_HOST" "$INTERCOM_HOST"', self.deploy)
        self.assertIn('"$CLOUDFLARE_API_TOKEN_FILE" "$host" "$NODE_IP"', self.deploy)
        self.assertIn("--skip-dns", self.deploy)

    def test_verify_checks_the_internal_name(self):
        self.assertIn("intercom", self.verify)
        self.assertIn("ListEndpoints", self.verify)


if __name__ == "__main__":
    unittest.main()
