#!/usr/bin/env python3
"""Static contract tests for the OCI distribution scripts.

`publish-oci.sh` runs in CI on `develop` and `fetch.sh` runs on the operator's
machine, so this suite pins the safe shape without network access: a semver
`VERSION`, immutable content on an existing tag, ORAS/attestation checks on
fetch, and no home-specific endpoint baked into the solution.

Run: python3 -m unittest discover -s solutions/windows-logon-provider
"""
from __future__ import annotations

import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
VERSION = HERE / "VERSION"
PUBLISH = HERE / "publish-oci.sh"
FETCH = HERE / "fetch.sh"
INSTALL = HERE / "install.ps1"


class VersionTests(unittest.TestCase):
    def test_version_is_semver(self):
        text = VERSION.read_text(encoding="utf-8").strip()
        self.assertRegex(text, r"^\d+\.\d+\.\d+$")


class ReproducibleBuildTests(unittest.TestCase):
    def test_link_omits_the_pe_timestamp(self):
        # Immutable OCI tags need identical bytes for identical sources; the PE
        # TimeDateStamp was the only varying field.
        build = (HERE / "build.sh").read_text(encoding="utf-8")
        self.assertIn("-Wl,--no-insert-timestamp", build)


class PublishScriptTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.text = PUBLISH.read_text(encoding="utf-8")

    def test_publishes_the_documented_repository_and_tags(self):
        self.assertIn("ghcr.io/proflyder/windows-logon-provider", self.text)
        self.assertIn('sha_tag="sha-${commit:0:12}"', self.text)

    def test_existing_tag_with_different_content_fails(self):
        self.assertIn("check_existing_tag", self.text)
        self.assertIn("cmp -s", self.text)
        self.assertIn("already exists with different content; bump VERSION", self.text)

    def test_files_and_media_types_match_the_artifact_contract(self):
        for name in (
            "RicardoKompLogon.dll",
            "cp-selftest.exe",
            "hmac-selftest.exe",
            "SHA256SUMS",
        ):
            self.assertIn(name, self.text)
        self.assertIn("application/vnd.microsoft.portable-executable", self.text)
        self.assertIn("SHA256SUMS:text/plain", self.text)

    def test_artifact_type_owner_comes_from_the_registry_path(self):
        # No second literal of the public account name: the type is derived.
        self.assertIn('owner="${REPOSITORY#*/}"', self.text)
        self.assertIn(".windows-logon-provider.v1", self.text)

    def test_annotations_carry_source_version_and_toolchain(self):
        for annotation in (
            "org.opencontainers.image.version",
            "org.opencontainers.image.revision",
            "org.opencontainers.image.source",
            "toolchain=llvm-mingw-20261020",
        ):
            self.assertIn(annotation, self.text)

    def test_checksums_are_verified_before_publishing(self):
        self.assertIn("sha256sum -c --quiet SHA256SUMS", self.text)
        self.assertIn("shasum -a 256 -c --quiet SHA256SUMS", self.text)
        self.assertLess(self.text.index("sha256sum -c --quiet"), self.text.index("oras push"))

    def test_publish_exposes_the_resolved_digest_as_a_step_output(self):
        self.assertIn("oras resolve", self.text)
        self.assertIn('echo "digest=$digest"', self.text)
        self.assertIn('>> "$GITHUB_OUTPUT"', self.text)


class FetchScriptTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.text = FETCH.read_text(encoding="utf-8")

    def test_accepts_a_version_or_a_digest(self):
        self.assertIn("sha256:*)", self.text)
        self.assertIn("@$ref", self.text)

    def test_verifies_checksums_and_provenance(self):
        self.assertIn("oras pull", self.text)
        self.assertIn("sha256sum -c --quiet SHA256SUMS", self.text)
        self.assertIn("shasum -a 256 -c --quiet SHA256SUMS", self.text)
        self.assertIn("gh attestation verify", self.text)
        self.assertIn('--owner "$owner"', self.text)

    def test_prints_the_resolved_digest(self):
        self.assertIn("oras resolve", self.text)
        self.assertIn("resolved digest:", self.text)


class SolutionBoundaryTests(unittest.TestCase):
    def test_install_defaults_are_not_home_specific(self):
        text = INSTALL.read_text(encoding="utf-8-sig")
        # ServiceUrl, Machine and UserName are required project inputs.
        for name in ("ServiceUrl", "Machine", "UserName"):
            self.assertIn(f"[Parameter(Mandatory = $true)]\n    [string]${name}", text)
        # The home NodePort and machine name must not be defaults here.
        self.assertNotIn("30130", text)
        self.assertNotIn("'komp'", text)

    def test_docs_and_install_surface_have_no_home_endpoint(self):
        # Protocol vectors intentionally use the documented home example; the
        # reusable install/docs surface must only carry generic placeholders.
        for path in (PUBLISH, FETCH, INSTALL, HERE / "README.md"):
            text = path.read_text(encoding="utf-8", errors="ignore")
            self.assertNotIn("192.0.2.27:30130", text, path.name)
            self.assertNotIn("'komp'", text, path.name)


if __name__ == "__main__":
    unittest.main()
