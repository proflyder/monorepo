"""Static contracts for the Nexus manifests (no cluster needed)."""
import pathlib
import re
import subprocess
import unittest

import yaml

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[3]
VERSIONS = (ROOT / "projects/home/kubernetes/versions.sh").read_text()
FLUX_CLUSTER = ROOT / "projects/home/kubernetes/flux/cluster"


def kustomize(path):
    result = subprocess.run(
        ["kubectl", "kustomize", str(path)],
        check=True,
        capture_output=True,
        text=True,
    )
    return [doc for doc in yaml.safe_load_all(result.stdout) if doc]


def of_kind(docs, kind):
    return [doc for doc in docs if doc.get("kind") == kind]


class Manifests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.docs = kustomize(HERE / "manifests")
        cls.rendered = yaml.safe_dump_all(cls.docs)

    def test_flux_owns_the_nexus_resources(self):
        kustomizations = {
            d["metadata"]["name"]: d
            for d in kustomize(FLUX_CLUSTER)
            if d.get("kind") == "Kustomization"
        }
        self.assertEqual(
            kustomizations["nexus"]["spec"]["path"],
            "./projects/home/kubernetes/nexus/manifests",
        )
        self.assertEqual(
            kustomizations["nexus-provision"]["spec"]["path"],
            "./projects/home/kubernetes/nexus/provision",
        )

    def test_private_values_are_flux_placeholders_only(self):
        left = set(re.findall(r"\$\{([A-Z_]+)\}", self.rendered))
        self.assertEqual(
            left, {"NEXUS_HOST", "NEXUS_PUBLIC_HOST", "NODE_LAN_IP"}
        )

    def test_hosts_use_the_private_suffix_and_a_separate_public_name(self):
        ingress = [d for d in of_kind(self.docs, "IngressRoute")]
        public = next(d for d in ingress if d["metadata"]["name"] == "nexus-public")
        admin = next(d for d in ingress if d["metadata"]["name"] == "nexus-admin")
        public_rule = public["spec"]["routes"][0]
        admin_rule = admin["spec"]["routes"][0]
        self.assertIn("Host(`${NEXUS_HOST}`)", public_rule["match"])
        self.assertIn("Host(`${NEXUS_PUBLIC_HOST}`)", public_rule["match"])
        self.assertIn("PathPrefix(`/repository/`)", public_rule["match"])
        self.assertNotIn("home-allowlist", [m["name"] for m in public_rule["middlewares"]])
        self.assertEqual(public_rule["priority"], 100)
        self.assertIn("Host(`${NEXUS_HOST}`)", admin_rule["match"])
        self.assertIn(
            "home-allowlist", [m["name"] for m in admin_rule["middlewares"]]
        )
        self.assertEqual(admin_rule["priority"], 10)

    def test_public_route_is_body_and_rate_limited(self):
        middlewares = of_kind(self.docs, "Middleware")
        limits = next(d for d in middlewares if d["metadata"]["name"] == "nexus-public-limits")
        self.assertEqual(limits["spec"]["buffering"]["maxRequestBodyBytes"], 8388608)
        self.assertGreater(limits["spec"]["rateLimit"]["average"], 0)
        self.assertGreater(limits["spec"]["rateLimit"]["burst"], 0)

    def test_image_pinned_by_digest_from_versions(self):
        digest = re.search(r"NEXUS_IMAGE_DIGEST=(sha256:[0-9a-f]{64})", VERSIONS).group(1)
        version = re.search(r"NEXUS_IMAGE_VERSION=([0-9.]+)", VERSIONS).group(1)
        self.assertEqual(version, "3.76.1")
        self.assertIn(
            f"docker.io/sonatype/nexus3:{version}@{digest}", self.rendered
        )

    def test_single_writer_statefulset_and_volume(self):
        statefulset = of_kind(self.docs, "StatefulSet")[0]
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
        statefulset = of_kind(self.docs, "StatefulSet")[0]
        pod = statefulset["spec"]["template"]["spec"]
        init = pod["initContainers"][0]
        self.assertEqual(init["volumeMounts"][1]["name"], "seed")
        self.assertEqual(pod["volumes"][0]["secret"]["secretName"], "nexus-secrets")
        external = (HERE / "manifests/external-secrets.yaml").read_text()
        self.assertIn("k8s/nexus/nexus-secrets", external)
        self.assertNotRegex(self.rendered, r"password\s*:\s*[^\s]")

    def test_network_policies_are_narrow(self):
        policies = {d["metadata"]["name"]: d for d in of_kind(self.docs, "NetworkPolicy")}
        ingress = policies["nexus-ingress"]
        egress = policies["nexus-egress"]
        self.assertIn("${NODE_LAN_IP}/32", ingress["spec"]["ingress"][0]["from"][0]["ipBlock"]["cidr"])
        ports = {p["port"] for rule in egress["spec"]["egress"] for p in rule["ports"]}
        self.assertEqual(ports, {53, 80, 443})


class Provisioning(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.docs = kustomize(HERE / "provision")

    def test_job_is_idempotent_and_uses_read_only_user(self):
        job = of_kind(self.docs, "Job")[0]
        container = job["spec"]["template"]["spec"]["containers"][0]
        env = {e["name"]: e for e in container["env"]}
        self.assertEqual(env["NEXUS_ADMIN_PASSWORD"]["valueFrom"]["secretKeyRef"]["key"], "admin_password")
        self.assertEqual(env["NEXUS_CI_PASSWORD"]["valueFrom"]["secretKeyRef"]["key"], "ci_reader_password")

        script = (HERE / "provision/provision.sh").read_text()
        for needle in (
            "ensure_repo maven proxy maven-central",
            "ensure_repo maven group maven-public",
            "ensure_repo raw proxy github",
            "ensure_repo raw proxy github-codeload",
            "ensure_repo npm proxy npmjs",
            "ensure_repo pypi proxy pypi",
            '"enabled":false',
            "nx-repository-view-*-*-read",
            "GET /service/rest/v1/security/roles/ci-reader",
            "GET '/service/rest/v1/security/users?userId=ci-reader'",
        ):
            self.assertIn(needle, script)
        self.assertIn("optional_repo cargo proxy crates", script)
        # Upstream 5xx must not auto-block a repository or poison the cache.
        self.assertIn('"autoBlock":false', script)
        self.assertIn('"retries":3', script)
        self.assertIn('"timeout":60', script)
        self.assertIn('"timeToLive":5', script)
        self.assertNotRegex(script, r"curl[^\n]*\s-[a-zA-Z]*v")
        self.assertNotIn("set -x", script)

    def test_job_reruns_when_the_script_hash_changes(self):
        configmap = of_kind(self.docs, "ConfigMap")[0]
        job = of_kind(self.docs, "Job")[0]
        self.assertRegex(configmap["metadata"]["name"], r"^nexus-provision-[a-z0-9]+$")
        scripts = next(
            v for v in job["spec"]["template"]["spec"]["volumes"] if v["name"] == "scripts"
        )
        self.assertEqual(scripts["configMap"]["name"], configmap["metadata"]["name"])
        script = (HERE / "provision/provision.sh").read_text()
        self.assertEqual(configmap["data"]["provision.sh"], script)
        kustomization = (FLUX_CLUSTER / "nexus-provision.yaml").read_text()
        self.assertIn("force: true", kustomization)
        self.assertIn("dependsOn:", kustomization)

    def test_image_pinned_by_digest_from_versions(self):
        digest = re.search(
            r"NEXUS_PROVISION_IMAGE_DIGEST=(sha256:[0-9a-f]{64})", VERSIONS
        ).group(1)
        version = re.search(r"NEXUS_PROVISION_IMAGE_VERSION=([0-9.]+)", VERSIONS).group(1)
        job = of_kind(self.docs, "Job")[0]
        image = job["spec"]["template"]["spec"]["containers"][0]["image"]
        self.assertEqual(image, f"docker.io/curlimages/curl:{version}@{digest}")


if __name__ == "__main__":
    unittest.main()
