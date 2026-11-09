"""Static contracts for the Traefik/cert-manager ingress manifests.

No cluster and no rendering binary are needed: these checks guard the public
manifest shape and the Flux substitution contract. The private values are
injected from the `flux-settings` Secret by the `ingress-routes` Kustomization.
"""
import pathlib
import re
import unittest

import yaml

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[3]
ROUTES = HERE / "routes"
ALLOWED_PLACEHOLDERS = {"HOME_DOMAIN", "LAN_CIDR", "HAOS_ADDRESS"}


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
        cls.route = by_name(load_all(ROUTES / "routes.yaml"), "IngressRoute", "intercom")

    def test_host_follows_home_naming(self):
        rule = self.route["spec"]["routes"][0]
        self.assertEqual(rule["match"], "Host(`intercom.${HOME_DOMAIN}`)")
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
        values = yaml.safe_load((HERE / "traefik" / "values.yaml").read_text(encoding="utf-8"))
        timeouts = values["ports"]["websecure"]["transport"]["respondingTimeouts"]
        self.assertEqual(timeouts["writeTimeout"], "0s")
        self.assertEqual(timeouts["idleTimeout"], "1800s")
        self.assertIn("readTimeout", timeouts)


class IntercomNetworkPolicy(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.policy = by_name(
            load_all(ROUTES / "intercom-networkpolicy.yaml"), "NetworkPolicy", "intercom-ingress"
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


class Routes(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.routes = load_all(ROUTES / "routes.yaml")
        cls.hosts = {
            route["metadata"]["name"]: route["spec"]["routes"][0]["match"]
            for route in cls.routes
        }

    def test_every_shared_route_uses_the_home_suffix(self):
        self.assertEqual(self.hosts, {
            "home-assistant": "Host(`ha.${HOME_DOMAIN}`)",
            "grafana": "Host(`grafana.${HOME_DOMAIN}`)",
            "scheduler": "Host(`alarms.${HOME_DOMAIN}`)",
            "currency-bot": "Host(`currency.${HOME_DOMAIN}`)",
            "ha-control": "Host(`control.${HOME_DOMAIN}`)",
            "intercom": "Host(`intercom.${HOME_DOMAIN}`)",
        })

    def test_only_documented_placeholders_remain(self):
        text = "\n".join(path.read_text() for path in ROUTES.glob("*.yaml"))
        left = set(re.findall(r"\$\{([A-Z_]+)\}", text))
        self.assertEqual(left, ALLOWED_PLACEHOLDERS)

    def test_wildcard_certificate_matches_the_suffix(self):
        certificate = by_name(load_all(ROUTES / "certificate.yaml"), "Certificate", "home-wildcard")
        self.assertEqual(certificate["metadata"]["namespace"], "traefik")
        self.assertEqual(certificate["spec"]["dnsNames"], ["*.${HOME_DOMAIN}"])
        self.assertEqual(certificate["spec"]["secretName"], "home-wildcard-tls")
        self.assertEqual(certificate["spec"]["issuerRef"]["name"], "letsencrypt-prod")

    def test_issuers_use_the_eso_managed_cloudflare_secret(self):
        issuers = load_all(ROUTES / "issuers.yaml")
        self.assertEqual({i["metadata"]["name"] for i in issuers},
                         {"letsencrypt-staging", "letsencrypt-prod"})
        for issuer in issuers:
            ref = issuer["spec"]["acme"]["solvers"][0]["dns01"]["cloudflare"]["apiTokenSecretRef"]
            self.assertEqual(ref, {"name": "cloudflare-api-token", "key": "api-token"})


if __name__ == "__main__":
    unittest.main()
