#!/usr/bin/env python3
"""build-and-push.sh: the prebuilt-digest path used by deploy-home (DECISIONS 36).

Hermetic: no Docker daemon, no registry and no network. `docker`, `bazel` and
`git` are replaced by stubs that record a call and fail, so any build/push for a
prebuilt image is a hard test failure.
"""
from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
SCRIPT = HERE / "images" / "build-and-push.sh"


class PrebuiltDigestTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp(prefix="build-push-"))
        self.addCleanup(lambda: shutil.rmtree(self.tmp, ignore_errors=True))
        self.bin = self.tmp / "bin"
        self.bin.mkdir()
        self.calls = self.tmp / "calls.log"
        for tool in ("docker", "bazel", "git"):
            stub = self.bin / tool
            stub.write_text(
                "#!/bin/sh\n"
                f'echo "{tool} $*" >> "{self.calls}"\n'
                "exit 1\n"
            )
            stub.chmod(0o755)
        self.digest_file = self.tmp / "digests"

    def run_script(self, images: list[str], extra_env: dict[str, str]) -> subprocess.CompletedProcess:
        env = os.environ.copy()
        env.pop("BAZELISK", None)
        env.update({
            "PATH": f"{self.bin}:{env['PATH']}",
            "DIGEST_FILE": str(self.digest_file),
            "IMAGE_TAG": "0123456789abcdef0123456789abcdef01234567",
        })
        env.update(extra_env)
        return subprocess.run(
            ["bash", str(SCRIPT), *images],
            env=env,
            text=True,
            capture_output=True,
            timeout=60,
        )

    def digests(self) -> dict[str, str]:
        if not self.digest_file.exists():
            return {}
        return dict(
            line.split("=", 1)
            for line in self.digest_file.read_text().splitlines()
            if line.strip()
        )

    def test_prebuilt_digests_are_recorded_without_building_or_pushing(self):
        hermes = "sha256:" + "a" * 64
        voice = "sha256:" + "b" * 64
        res = self.run_script(
            ["hermes", "voice-proxy"],
            {
                "PREBUILT_DIGESTS": f"hermes={hermes},voice-proxy={voice}",
            },
        )
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertFalse(self.calls.exists(), "a prebuilt image was rebuilt or pushed")
        self.assertEqual(self.digests(), {"hermes": hermes, "voice-proxy": voice})
        self.assertIn("using prebuilt", res.stdout)

    def test_prebuilt_digest_file_is_accepted(self):
        hermes = "sha256:" + "c" * 64
        voice = "sha256:" + "d" * 64
        digests = self.tmp / "promoted"
        digests.write_text(f"# promoted\nhermes={hermes}\nvoice-proxy={voice}\n")
        res = self.run_script(
            ["hermes", "voice-proxy"], {"PREBUILT_DIGESTS_FILE": str(digests)}
        )
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertEqual(self.digests(), {"hermes": hermes, "voice-proxy": voice})

    def test_malformed_digest_fails_before_any_build(self):
        res = self.run_script(
            ["hermes"], {"PREBUILT_DIGESTS": "hermes=sha256:nothex"}
        )
        self.assertNotEqual(res.returncode, 0)
        self.assertIn("malformed prebuilt digest", res.stderr)
        self.assertFalse(self.calls.exists())
        self.assertFalse(self.digest_file.exists())

    def test_duplicate_digest_fails_closed(self):
        digest = "sha256:" + "e" * 64
        res = self.run_script(
            ["hermes"],
            {"PREBUILT_DIGESTS": f"hermes={digest},hermes={digest}"},
        )
        self.assertNotEqual(res.returncode, 0)
        self.assertIn("duplicate prebuilt digest", res.stderr)
        self.assertFalse(self.calls.exists())

    def test_unknown_image_is_rejected_even_with_a_prebuilt_digest(self):
        res = self.run_script(
            ["not-an-image"],
            {"PREBUILT_DIGESTS": "not-an-image=sha256:" + "f" * 64},
        )
        self.assertNotEqual(res.returncode, 0)
        self.assertIn("unknown image", res.stderr)
        self.assertFalse(self.calls.exists())


if __name__ == "__main__":
    unittest.main(verbosity=2)
