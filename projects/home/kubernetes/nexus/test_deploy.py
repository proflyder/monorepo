"""Static contracts for the Nexus deploy (no cluster needed)."""
import os
import pathlib
import re
import subprocess
import tempfile
import unittest

import yaml

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[3]
VERSIONS = (ROOT / "projects/home/kubernetes/versions.sh").read_text()


def render(extra_env=None, drop=()):
    out = tempfile.mkdtemp()
    env = dict(
        os.environ,
        HOME_DOMAIN="home.example.com",
        LAN_CIDR="192.0.2.0/24",
        NODE_IP="192.0.2.27",
        INGRESS_PRIVATE_ENV="/nonexistent",
    )
    env.pop("KUBECONFIG", None)
    env.pop("NEXUS_PUBLIC_HOST", None)
    env.update(extra_env or {})
    for key in drop:
        env.pop(key, None)
    subprocess.run(
        ["bash", str(HERE / "deploy.sh"), "--render-only", out],
        check=True,
        env=env,
        capture_output=True,
        text=True,
    )
    return {p.name: p.read_text() for p in pathlib.Path(out).iterdir()}


def docs(text):
    return [d for d in yaml.safe_load_all(text) if d]


class Render(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.files = render()

    def test_all_templates_render_without_placeholders(self):
        self.assertEqual(
            sorted(self.files),
            sorted(
                [
                    "namespace.yaml",
                    "external-secrets.yaml",
                    "network-policies.yaml",
                    "nexus.yaml",
                    "provisioning.yaml",
                    "ingressroute.yaml",
                    "provision.sh",
                ]
            ),
        )
        for name, text in self.files.items():
            self.assertNotRegex(text, r"\$\{[A-Z_]+\}", name)

    def test_hosts_use_the_private_suffix_and_a_separate_public_name(self):
        routes = docs(self.files["ingressroute.yaml"])
        ingress = [d for d in routes if d["kind"] == "IngressRoute"]
        public = next(d for d in ingress if d["metadata"]["name"] == "nexus-public")
        admin = next(d for d in ingress if d["metadata"]["name"] == "nexus-admin")
        public_rule = public["spec"]["routes"][0]
        admin_rule = admin["spec"]["routes"][0]
        self.assertIn("Host(`nexus.home.example.com`)", public_rule["match"])
        self.assertIn("Host(`ci-nexus.home.example.com`)", public_rule["match"])
        self.assertIn("PathPrefix(`/repository/`)", public_rule["match"])
        self.assertNotIn("home-allowlist", public_rule["middlewares"])
        self.assertEqual(public_rule["priority"], 100)
        self.assertIn("Host(`nexus.home.example.com`)", admin_rule["match"])
        self.assertIn(
            "home-allowlist", [m["name"] for m in admin_rule["middlewares"]]
        )
        self.assertEqual(admin_rule["priority"], 10)

    def test_public_route_is_body_and_rate_limited(self):
        middlewares = [
            d
            for d in docs(self.files["ingressroute.yaml"])
            if d["kind"] == "Middleware"
        ]
        limits = next(d for d in middlewares if d["metadata"]["name"] == "nexus-public-limits")
        self.assertEqual(limits["spec"]["buffering"]["maxRequestBodyBytes"], 8388608)
        self.assertGreater(limits["spec"]["rateLimit"]["average"], 0)
        self.assertGreater(limits["spec"]["rateLimit"]["burst"], 0)

    def test_image_pinned_by_digest_from_versions(self):
        digest = re.search(r"NEXUS_IMAGE_DIGEST=(sha256:[0-9a-f]{64})", VERSIONS).group(1)
        version = re.search(r"NEXUS_IMAGE_VERSION=([0-9.]+)", VERSIONS).group(1)
        self.assertEqual(version, "3.76.1")
        self.assertIn(f"docker.io/sonatype/nexus3:{version}@{digest}", self.files["nexus.yaml"])

    def test_single_writer_statefulset_and_volume(self):
        specs = docs(self.files["nexus.yaml"])
        statefulset = next(d for d in specs if d["kind"] == "StatefulSet")
        self.assertEqual(statefulset["spec"]["replicas"], 1)
        self.assertEqual(statefulset["spec"]["serviceName"], "nexus")
        claim = statefulset["spec"]["volumeClaimTemplates"][0]
        self.assertEqual(claim["metadata"]["name"], "data")
        self.assertEqual(claim["spec"]["storageClassName"], "local-retain")
        self.assertEqual(claim["spec"]["resources"]["requests"]["storage"], "50Gi")
        pod = statefulset["spec"]["template"]["spec"]
        self.assertEqual(len(pod["containers"]), 1)
        container = pod["containers"][0]
        self.assertEqual(container["resources"]["limits"]["memory"], "2500Mi")
        self.assertIn("-Xms1024m", container["env"][0]["value"])
        self.assertIn("-Xmx1024m", container["env"][0]["value"])
        self.assertIn("MaxDirectMemorySize=1024m", container["env"][0]["value"])
        self.assertEqual(
            container["readinessProbe"]["httpGet"]["path"],
            "/service/rest/v1/status/writable",
        )
        self.assertEqual(
            container["livenessProbe"]["httpGet"]["path"],
            "/service/rest/v1/status",
        )

    def test_admin_password_is_seeded_from_eso_without_baked_secrets(self):
        statefulset = next(d for d in docs(self.files["nexus.yaml"]) if d["kind"] == "StatefulSet")
        pod = statefulset["spec"]["template"]["spec"]
        init = pod["initContainers"][0]
        self.assertEqual(init["volumeMounts"][1]["name"], "seed")
        self.assertEqual(pod["volumes"][0]["secret"]["secretName"], "nexus-secrets")
        self.assertIn("k8s/nexus/nexus-secrets", self.files["external-secrets.yaml"])
        self.assertNotRegex(self.files["nexus.yaml"], r"password\s*:\s*[^\s]")

    def test_provisioning_job_is_idempotent_and_uses_read_only_user(self):
        job = next(d for d in docs(self.files["provisioning.yaml"]) if d["kind"] == "Job")
        container = job["spec"]["template"]["spec"]["containers"][0]
        env = {e["name"]: e for e in container["env"]}
        self.assertEqual(env["NEXUS_ADMIN_PASSWORD"]["valueFrom"]["secretKeyRef"]["key"], "admin_password")
        self.assertEqual(env["NEXUS_CI_PASSWORD"]["valueFrom"]["secretKeyRef"]["key"], "ci_reader_password")
        script = self.files["provision.sh"]
        for needle in (
            "ensure_repo maven proxy maven-central",
            "ensure_repo maven group maven-public",
            "ensure_repo raw proxy github",
            "ensure_repo raw proxy github-codeload",
            "ensure_repo npm proxy npmjs",
            "ensure_repo pypi proxy pypi",
            '"enabled":false',
            "nx-repository-view-*-*-read",
            'GET /service/rest/v1/security/roles/ci-reader',
            'GET \'/service/rest/v1/security/users?userId=ci-reader\'',
        ):
            self.assertIn(needle, script)
        # A failed optional format (cargo) must not fail the whole run.
        self.assertIn("optional_repo cargo proxy crates", script)
        # No verbose curl and no shell tracing: credentials stay out of logs.
        self.assertNotRegex(script, r"curl[^\n]*\s-[a-zA-Z]*v")
        self.assertNotIn("set -x", script)

    def test_network_policies_are_narrow(self):
        policies = docs(self.files["network-policies.yaml"])
        ingress = next(d for d in policies if d["metadata"]["name"] == "nexus-ingress")
        egress = next(d for d in policies if d["metadata"]["name"] == "nexus-egress")
        self.assertIn("192.0.2.27/32", ingress["spec"]["ingress"][0]["from"][0]["ipBlock"]["cidr"])
        ports = {p["port"] for rule in egress["spec"]["egress"] for p in rule["ports"]}
        self.assertEqual(ports, {53, 80, 443})

    def test_missing_private_values_fail(self):
        with self.assertRaises(subprocess.CalledProcessError):
            render({"HOME_DOMAIN": ""})
        with self.assertRaises(subprocess.CalledProcessError):
            render(drop=("NODE_IP",))


if __name__ == "__main__":
    unittest.main()
