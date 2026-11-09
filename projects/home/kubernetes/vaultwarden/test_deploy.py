"""Static contracts for the Vaultwarden manifests (no cluster needed).

The manifests are Flux-rendered; only `NODE_LAN_IP`, `LAN_CIDR` and
`HOME_DOMAIN` remain as substitutable placeholders.
"""
import pathlib
import re
import unittest

import yaml

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[3]
VERSIONS = (ROOT / "projects/home/kubernetes/versions.sh").read_text()
MANIFESTS = HERE / "manifests"
PLACEHOLDERS = {"NODE_LAN_IP", "LAN_CIDR", "HOME_DOMAIN"}


def load(name):
    return [doc for doc in yaml.safe_load_all((MANIFESTS / name).read_text()) if doc]


class Manifests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.files = {path.name: path.read_text() for path in MANIFESTS.glob("*.yaml")}
        cls.workload = load("vaultwarden.yaml")

    def test_only_documented_placeholders_remain(self):
        left = set()
        for text in self.files.values():
            left |= set(re.findall(r"\$\{([A-Z_]+)\}", text))
        self.assertEqual(left, PLACEHOLDERS)

    def test_host_follows_home_naming(self):
        route = next(d for d in load("ingressroute.yaml") if d["kind"] == "IngressRoute")
        self.assertEqual(route["spec"]["routes"][0]["match"],
                         "Host(`passwords.${HOME_DOMAIN}`)")
        self.assertEqual(route["spec"]["routes"][0]["middlewares"],
                         [{"name": "home-allowlist"}])
        env = {e["name"]: e.get("value") for e in self._deployment()["spec"]["template"]
               ["spec"]["containers"][0]["env"]}
        self.assertEqual(env["DOMAIN"], "https://passwords.${HOME_DOMAIN}")

    def _deployment(self):
        return next(d for d in self.workload if d["kind"] == "Deployment")

    def test_image_pinned_by_digest_from_versions(self):
        digest = re.search(r"VAULTWARDEN_IMAGE_DIGEST=(sha256:[0-9a-f]{64})", VERSIONS).group(1)
        version = re.search(r"VAULTWARDEN_IMAGE_VERSION=([0-9.]+)", VERSIONS).group(1)
        image = self._deployment()["spec"]["template"]["spec"]["containers"][0]["image"]
        self.assertEqual(image, f"docker.io/vaultwarden/server:{version}@{digest}")

    def test_closed_registration_and_secret_from_eso(self):
        env = {e["name"]: e for e in self._deployment()["spec"]["template"]["spec"]
               ["containers"][0]["env"]}
        self.assertEqual(env["SIGNUPS_ALLOWED"]["value"], "false")
        self.assertEqual(env["DISABLE_ICON_DOWNLOAD"]["value"], "true")
        self.assertEqual(env["ADMIN_TOKEN"]["valueFrom"]["secretKeyRef"]["name"],
                         "vaultwarden-admin")
        eso = load("external-secrets.yaml")[0]
        self.assertEqual(eso["spec"]["dataFrom"][0]["extract"]["key"],
                         "k8s/vaultwarden/vaultwarden-admin")

    def test_hardened_single_writer(self):
        deployment = self._deployment()
        self.assertEqual(deployment["spec"]["strategy"]["type"], "Recreate")
        pod = deployment["spec"]["template"]["spec"]
        self.assertTrue(pod["securityContext"]["runAsNonRoot"])
        container = pod["containers"][0]
        self.assertTrue(container["securityContext"]["readOnlyRootFilesystem"])
        pvc = next(d for d in self.workload if d["kind"] == "PersistentVolumeClaim")
        self.assertEqual(pvc["spec"]["storageClassName"], "local-retain")
        self.assertEqual(pvc["metadata"]["annotations"]["kustomize.toolkit.fluxcd.io/prune"],
                         "disabled")

    def test_egress_is_dns_only(self):
        egress = next(d for d in load("network-policies.yaml")
                      if d["metadata"]["name"] == "vaultwarden-egress")
        text = yaml.safe_dump(egress)
        self.assertNotIn("0.0.0.0/0", text)
        self.assertEqual(sorted(set(re.findall(r"port: (\d+)", text))), ["53"])


if __name__ == "__main__":
    unittest.main()
