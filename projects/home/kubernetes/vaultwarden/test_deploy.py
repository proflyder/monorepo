"""Static contracts for the Vaultwarden deploy (no cluster needed)."""
import os
import pathlib
import re
import subprocess
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[3]
VERSIONS = (ROOT / "projects/home/kubernetes/versions.sh").read_text()


def render(extra_env=None):
    out = tempfile.mkdtemp()
    env = dict(os.environ, HOME_DOMAIN="home.example.com", LAN_CIDR="192.0.2.0/24",
               NODE_IP="192.0.2.27", INGRESS_PRIVATE_ENV="/nonexistent")
    env.pop("KUBECONFIG", None)
    env.update(extra_env or {})
    subprocess.run(["bash", str(HERE / "deploy.sh"), "--render-only", out],
                   check=True, env=env, capture_output=True, text=True)
    return {p.name: p.read_text() for p in pathlib.Path(out).iterdir()}


class Render(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.files = render()

    def test_all_manifests_rendered_without_placeholders(self):
        self.assertEqual(sorted(self.files), sorted(
            ["namespace.yaml", "external-secrets.yaml", "network-policies.yaml",
             "vaultwarden.yaml", "ingressroute.yaml"]))
        for name, text in self.files.items():
            self.assertNotRegex(text, r"\$\{[A-Z_]+\}", name)

    def test_host_follows_home_naming(self):
        self.assertIn("Host(`passwords.home.example.com`)", self.files["ingressroute.yaml"])
        self.assertIn("https://passwords.home.example.com", self.files["vaultwarden.yaml"])
        self.assertIn("home-allowlist", self.files["ingressroute.yaml"])

    def test_image_pinned_by_digest_from_versions(self):
        digest = re.search(r"VAULTWARDEN_IMAGE_DIGEST=(sha256:[0-9a-f]{64})", VERSIONS).group(1)
        self.assertIn(f"image: docker.io/vaultwarden/server:", self.files["vaultwarden.yaml"])
        self.assertIn(f"@{digest}", self.files["vaultwarden.yaml"])

    def test_closed_registration_and_secret_from_eso(self):
        text = self.files["vaultwarden.yaml"]
        self.assertRegex(text, r'name: SIGNUPS_ALLOWED\n\s+value: "false"')
        self.assertRegex(text, r'name: DISABLE_ICON_DOWNLOAD\n\s+value: "true"')
        self.assertRegex(text, r"name: ADMIN_TOKEN\n\s+valueFrom:\n\s+secretKeyRef:")
        self.assertIn("key: k8s/vaultwarden/vaultwarden-admin", self.files["external-secrets.yaml"])

    def test_hardened_single_writer(self):
        text = self.files["vaultwarden.yaml"]
        for needle in ("type: Recreate", "runAsNonRoot: true", "readOnlyRootFilesystem: true",
                       "storageClassName: local-retain", "claimName: vaultwarden-data"):
            self.assertIn(needle, text)

    def test_egress_is_dns_only(self):
        egress = self.files["network-policies.yaml"].split("name: vaultwarden-egress")[1]
        self.assertNotIn("0.0.0.0/0", egress)
        self.assertEqual(sorted(set(re.findall(r"port: (\d+)", egress))), ["53"])

    def test_missing_private_values_fail(self):
        with self.assertRaises(subprocess.CalledProcessError):
            render({"HOME_DOMAIN": ""})


if __name__ == "__main__":
    unittest.main()
