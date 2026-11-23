import os
import pathlib
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "platform/workstation/with-kube-tunnel.sh"


class KubeTunnelGuardTests(unittest.TestCase):
    def run_guard(self, kubeconfig=None, server=None):
        with tempfile.TemporaryDirectory() as directory:
            bin_dir = pathlib.Path(directory) / "bin"
            bin_dir.mkdir()
            nc = bin_dir / "nc"
            nc.write_text("#!/bin/sh\nexit 0\n")
            nc.chmod(0o755)
            env = dict(os.environ)
            env.pop("KUBECONFIG", None)
            env["PATH"] = f"{bin_dir}:{env['PATH']}"
            if kubeconfig is not None:
                env["KUBECONFIG"] = kubeconfig
            config = pathlib.Path(directory) / "kubeconfig"
            if server is not None:
                config.write_text(
                    "apiVersion: v1\nkind: Config\nclusters:\n"
                    "- cluster:\n"
                    f"    server: {server}\n"
                    "  name: home\n"
                    "contexts:\n"
                    "- context:\n"
                    "    cluster: home\n"
                    "    user: home\n"
                    "  name: home\n"
                    "current-context: home\n"
                    "users:\n"
                    "- name: home\n"
                    "  user: {}\n"
                )
            return subprocess.run(
                [str(SCRIPT), "true"],
                env=env,
                text=True,
                capture_output=True,
                check=False,
            )

    def test_a_missing_kubeconfig_is_refused(self):
        result = self.run_guard()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("KUBECONFIG", result.stderr)

    def test_a_missing_file_is_refused(self):
        result = self.run_guard(kubeconfig="/does/not/exist")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("does not exist", result.stderr)

    def test_a_foreign_server_is_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "kubeconfig"
            path.write_text(
                "apiVersion: v1\nkind: Config\nclusters:\n"
                "- cluster:\n    server: https://10.0.0.1:6443\n  name: home\n"
                "contexts:\n- context:\n    cluster: home\n    user: home\n  name: home\n"
                "current-context: home\nusers:\n- name: home\n  user: {}\n"
            )
            result = self.run_guard(kubeconfig=str(path))
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("refusing kubeconfig server", result.stderr)

    def test_the_local_tunnel_server_is_accepted_without_network(self):
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "kubeconfig"
            path.write_text(
                "apiVersion: v1\nkind: Config\nclusters:\n"
                "- cluster:\n    server: https://127.0.0.1:16443\n  name: home\n"
                "contexts:\n- context:\n    cluster: home\n    user: home\n  name: home\n"
                "current-context: home\nusers:\n- name: home\n  user: {}\n"
            )
            result = self.run_guard(kubeconfig=str(path))
            self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
