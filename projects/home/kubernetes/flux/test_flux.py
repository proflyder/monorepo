"""Static contracts for the Flux GitOps bundle (no cluster needed)."""
import pathlib
import re
import subprocess
import unittest

import yaml

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[3]
NEXUS = ROOT / "projects/home/kubernetes/nexus"
VERSIONS = (ROOT / "projects/home/kubernetes/versions.sh").read_text()

CONTROLLERS = {"source-controller", "kustomize-controller", "helm-controller"}
PRIVATE_VARS = {"NEXUS_HOST", "NEXUS_PUBLIC_HOST", "NODE_LAN_IP"}


def kustomize(path):
    result = subprocess.run(
        ["kubectl", "kustomize", str(path)],
        check=True,
        capture_output=True,
        text=True,
    )
    return [doc for doc in yaml.safe_load_all(result.stdout) if doc]


def docs_from(path):
    return [doc for doc in yaml.safe_load_all(path.read_text()) if doc]


def of_kind(docs, kind):
    return [doc for doc in docs if doc.get("kind") == kind]


class Toolkit(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.docs = kustomize(HERE)
        cls.deployments = {d["metadata"]["name"]: d for d in of_kind(cls.docs, "Deployment")}

    def test_only_the_three_controllers_are_deployed(self):
        self.assertEqual(set(self.deployments), CONTROLLERS)
        text = (HERE / "gotk-components.yaml").read_text()
        self.assertNotIn("notification-controller:v", text)
        self.assertNotIn("image-reflector-controller:v", text)
        self.assertNotIn("image-automation-controller:v", text)
        self.assertNotIn("source-watcher:v", text)

    def test_images_are_pinned_to_the_reviewed_release(self):
        for name, deployment in self.deployments.items():
            images = [
                container["image"]
                for container in deployment["spec"]["template"]["spec"]["containers"]
            ]
            self.assertEqual(len(images), 1, name)
            self.assertRegex(images[0], rf"^ghcr\.io/fluxcd/{re.escape(name)}:v\d+\.\d+\.\d+$")
            self.assertNotIn(":latest", images[0])

    def test_controllers_fit_a_small_node(self):
        for name, deployment in self.deployments.items():
            resources = deployment["spec"]["template"]["spec"]["containers"][0]["resources"]
            limits = resources["limits"]
            requests = resources["requests"]
            self.assertEqual(limits["memory"], "512Mi", name)
            self.assertIn(limits["cpu"], {"500m", "1000m"}, name)
            self.assertLessEqual(int(requests["memory"].rstrip("Mi")), 128, name)
            self.assertLessEqual(int(requests["cpu"].rstrip("m")), 200, name)

    def test_required_crds_are_present(self):
        crds = {doc["metadata"]["name"] for doc in of_kind(self.docs, "CustomResourceDefinition")}
        for name in (
            "gitrepositories.source.toolkit.fluxcd.io",
            "ocirepositories.source.toolkit.fluxcd.io",
            "helmrepositories.source.toolkit.fluxcd.io",
            "helmcharts.source.toolkit.fluxcd.io",
            "kustomizations.kustomize.toolkit.fluxcd.io",
            "helmreleases.helm.toolkit.fluxcd.io",
        ):
            self.assertIn(name, crds)
        self.assertFalse(any("notification" in name for name in crds))


class Cluster(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.docs = kustomize(HERE / "cluster")

    def test_network_policy_is_narrow(self):
        policies = {d["metadata"]["name"]: d for d in of_kind(self.docs, "NetworkPolicy")}
        ingress = policies["flux-system-ingress"]
        egress = policies["flux-system-egress"]
        self.assertEqual(ingress["spec"]["policyTypes"], ["Ingress"])
        self.assertNotIn("ingress", ingress["spec"])

        ports = {
            port["port"]
            for rule in egress["spec"]["egress"]
            for port in rule.get("ports", [])
        }
        self.assertTrue({53, 22, 443, 6443}.issubset(ports))
        dns = egress["spec"]["egress"][0]
        self.assertEqual(
            dns["to"][0]["namespaceSelector"]["matchLabels"]["kubernetes.io/metadata.name"],
            "kube-system",
        )
        rendered = yaml.safe_dump(egress)
        self.assertIn("${NODE_LAN_IP}/32", rendered)
        self.assertIn("140.82.112.0/20", rendered)

    def test_nexus_kustomizations_use_the_git_source(self):
        kustomizations = {
            d["metadata"]["name"]: d for d in of_kind(self.docs, "Kustomization")
        }
        self.assertEqual(set(kustomizations), {"nexus", "nexus-provision"})
        for name, k in kustomizations.items():
            self.assertEqual(k["spec"]["sourceRef"]["kind"], "GitRepository")
            self.assertEqual(k["spec"]["sourceRef"]["name"], "flux-system")
            self.assertTrue(k["spec"]["prune"], name)

        nexus = kustomizations["nexus"]
        self.assertEqual(
            nexus["spec"]["postBuild"]["substituteFrom"],
            [{"kind": "Secret", "name": "flux-settings"}],
        )
        self.assertEqual(nexus["spec"]["path"], "./projects/home/kubernetes/nexus/manifests")
        provision = kustomizations["nexus-provision"]
        self.assertEqual(provision["spec"]["path"], "./projects/home/kubernetes/nexus/provision")
        self.assertTrue(provision["spec"]["force"])
        self.assertEqual(provision["spec"]["dependsOn"], [{"name": "nexus"}])


class Sync(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.docs = docs_from(HERE / "sync.yaml")

    def test_git_repository_is_read_only_and_on_develop(self):
        repo = of_kind(self.docs, "GitRepository")[0]
        self.assertEqual(repo["metadata"]["namespace"], "flux-system")
        self.assertEqual(repo["spec"]["url"], "ssh://git@github.com/proflyder/monorepo")
        self.assertEqual(repo["spec"]["ref"], {"branch": "develop"})
        self.assertEqual(repo["spec"]["secretRef"]["name"], "flux-system")

    def test_root_kustomization_reconciles_cluster_dir(self):
        root = of_kind(self.docs, "Kustomization")[0]
        self.assertEqual(root["metadata"]["name"], "flux-system")
        self.assertEqual(root["spec"]["path"], "./projects/home/kubernetes/flux/cluster")
        self.assertEqual(
            root["spec"]["postBuild"]["substituteFrom"],
            [{"kind": "Secret", "name": "flux-settings"}],
        )


class Secrets(unittest.TestCase):
    def test_external_secrets_read_openbao_paths(self):
        text = (HERE / "external-secrets.yaml").read_text()
        self.assertIn("k8s/flux-system/git-deploy-key", text)
        self.assertIn("k8s/flux-system/settings", text)
        docs = docs_from(HERE / "external-secrets.yaml")
        targets = {
            d["metadata"]["name"]: d["spec"]["target"]["name"]
            for d in of_kind(docs, "ExternalSecret")
        }
        self.assertEqual(targets, {
            "flux-git-deploy-key": "flux-system",
            "flux-settings": "flux-settings",
        })
        self.assertNotRegex(text, r"BEGIN (OPENSSH|RSA|EC) PRIVATE KEY")
        self.assertNotRegex(text, r"password\s*:\s*\S")

    def test_catalog_and_docs_name_the_new_paths(self):
        catalog = (ROOT / "platform/workstation/catalog.json").read_text()
        secrets_doc = (ROOT / "docs/SECRETS.md").read_text()
        for path in (
            "k8s/flux-system/git-deploy-key",
            "k8s/flux-system/settings",
        ):
            self.assertIn(path, catalog)
            self.assertIn(path, secrets_doc)


class NexusManifests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.docs = kustomize(NEXUS / "manifests")
        cls.rendered = yaml.safe_dump_all(cls.docs)

    def test_private_values_are_the_only_placeholders(self):
        left = set(re.findall(r"\$\{([A-Z_]+)\}", self.rendered))
        self.assertEqual(left, PRIVATE_VARS)

    def test_images_are_pinned_from_versions(self):
        digest = re.search(r"NEXUS_IMAGE_DIGEST=(sha256:[0-9a-f]{64})", VERSIONS).group(1)
        version = re.search(r"NEXUS_IMAGE_VERSION=([0-9.]+)", VERSIONS).group(1)
        images = {
            container["image"]
            for d in of_kind(self.docs, "StatefulSet")
            for container in d["spec"]["template"]["spec"]["containers"]
            + d["spec"]["template"]["spec"]["initContainers"]
        }
        self.assertEqual(images, {f"docker.io/sonatype/nexus3:{version}@{digest}"})

    def test_no_secret_value_is_committed(self):
        for path in (NEXUS / "manifests").glob("*.yaml"):
            text = path.read_text()
            self.assertNotRegex(text, r"BEGIN (OPENSSH|RSA|EC) PRIVATE KEY", path.name)
            self.assertNotRegex(text, r"password\s*:\s*\S", path.name)


class Provisioning(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.docs = kustomize(NEXUS / "provision")

    def test_job_mounts_the_generated_config_map(self):
        configmap = of_kind(self.docs, "ConfigMap")[0]
        self.assertRegex(configmap["metadata"]["name"], r"^nexus-provision-[a-z0-9]+$")
        job = of_kind(self.docs, "Job")[0]
        self.assertEqual(job["metadata"]["name"], "nexus-provision")
        volumes = job["spec"]["template"]["spec"]["volumes"]
        scripts = next(v for v in volumes if v["name"] == "scripts")
        self.assertEqual(scripts["configMap"]["name"], configmap["metadata"]["name"])
        self.assertIn("ensure_repo maven group maven-public", configmap["data"]["provision.sh"])

    def test_image_pin_matches_versions(self):
        digest = re.search(
            r"NEXUS_PROVISION_IMAGE_DIGEST=(sha256:[0-9a-f]{64})", VERSIONS
        ).group(1)
        version = re.search(r"NEXUS_PROVISION_IMAGE_VERSION=([0-9.]+)", VERSIONS).group(1)
        job = of_kind(self.docs, "Job")[0]
        image = job["spec"]["template"]["spec"]["containers"][0]["image"]
        self.assertEqual(image, f"docker.io/curlimages/curl:{version}@{digest}")

    def test_script_hash_changes_with_content(self):
        # The generator hash is content-addressed, so editing provision.sh
        # yields a different ConfigMap name (and therefore Job spec).
        first = of_kind(self.docs, "ConfigMap")[0]["metadata"]["name"]
        self.assertNotEqual(first, "nexus-provision")


if __name__ == "__main__":
    unittest.main()
