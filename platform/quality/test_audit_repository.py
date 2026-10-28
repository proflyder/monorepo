#!/usr/bin/env python3
"""Boundary tests for the reviewed public-name allowances in the audit scanner.

Sample strings are built by concatenation so this file never contains the
personal name as a literal and cannot flag itself.
"""
from __future__ import annotations

import unittest
from pathlib import Path

from audit_repository import scan_text, sanitize_approved_tokens

NAME = "prof" + "lyder"
DOMAIN = NAME + ".dev"
PACKAGE = "dev." + NAME + ".home"
SOURCE_PATH = "dev/" + NAME + "/home"
REPO = NAME + "/pulsar"
LEGACY_HOME = "/Users/stanislav" + "plotnikov"


class ApprovedPublicTests(unittest.TestCase):
    def test_exact_public_domain_is_accepted(self):
        for text in (
            f"Сайт: https://{DOMAIN}",
            f"Домен {DOMAIN} используется для приложения.",
            f"link http://{DOMAIN}/path?q=1",
        ):
            with self.subTest(text=text):
                self.assertEqual(scan_text("docs/example.md", text), [])

    def test_android_namespace_and_subpackages_are_accepted(self):
        for text in (
            f"applicationId = \"{PACKAGE}\"",
            f"package {PACKAGE}.ui",
            f"import {PACKAGE}.config.AppConfig",
        ):
            with self.subTest(text=text):
                self.assertEqual(scan_text("projects/mobile/android/app/build.gradle", text), [])

    def test_source_and_resource_paths_are_accepted(self):
        for text in (
            f"src/main/java/{SOURCE_PATH}/MainActivity.kt",
            f"res/{SOURCE_PATH}/values/strings.xml",
        ):
            with self.subTest(text=text):
                self.assertEqual(scan_text("projects/mobile/android/README.md", text), [])

    def test_sanitizer_replaces_only_approved_tokens(self):
        line = f"{PACKAGE} {DOMAIN} {SOURCE_PATH} {REPO}"
        cleaned = sanitize_approved_tokens(line)
        self.assertNotIn(NAME, cleaned)
        self.assertIn("<public-namespace>", cleaned)
        self.assertIn("<public-domain>", cleaned)
        self.assertIn("<public-source-path>", cleaned)
        self.assertIn("<public-repo>", cleaned)

    def test_exact_public_repo_slug_is_accepted(self):
        for text in (
            f"`UPDATE_REPO={REPO}`,",
            f"const UPDATE_REPO: &str = \"{REPO}\";",
            f"https://github.com/{REPO}",
        ):
            with self.subTest(text=text):
                self.assertEqual(scan_text("projects/station/upstream/pulsar-rust/RICARDO.md", text), [])


class LookalikeTests(unittest.TestCase):
    def assert_flagged(self, text: str):
        errors = scan_text("docs/example.md", text)
        self.assertTrue(errors, f"expected a finding for {text!r}")

    def test_bare_personal_name_is_rejected(self):
        self.assert_flagged(f"Автор {NAME} пишет код.")

    def test_unrelated_com_domain_is_rejected(self):
        self.assert_flagged(f"https://{NAME}.com")

    def test_longer_host_is_rejected(self):
        self.assert_flagged(f"https://{DOMAIN}.example")

    def test_prefixed_host_is_rejected(self):
        self.assert_flagged(f"https://evil{NAME}.dev")

    def test_subdomain_is_rejected(self):
        self.assert_flagged(f"https://sub.{DOMAIN}")

    def test_longer_identifier_is_rejected(self):
        self.assert_flagged(f"package dev.{NAME}.homepage")

    def test_prefixed_namespace_is_rejected(self):
        self.assert_flagged(f"package foo.dev.{NAME}.home")

    def test_longer_source_directory_is_rejected(self):
        self.assert_flagged(f"path {SOURCE_PATH}page/Main.kt")

    def test_prefixed_source_directory_is_rejected(self):
        self.assert_flagged(f"path x{SOURCE_PATH}/Main.kt")

    def test_legacy_private_path_is_rejected(self):
        self.assert_flagged(f"{LEGACY_HOME}/work")

    def test_other_repository_of_handle_is_rejected(self):
        self.assert_flagged(f"{NAME}/other")

    def test_longer_repository_name_is_rejected(self):
        self.assert_flagged(f"{REPO}-private")
        self.assert_flagged(f"{REPO}2")
        self.assert_flagged(f"{REPO}.private")
        self.assert_flagged(f"{REPO}.secret")
        self.assert_flagged(f"{REPO}.git")

    def test_prefixed_repository_path_is_rejected(self):
        self.assert_flagged(f"https://github.com/evil{REPO}")

    def test_mixed_public_repo_and_token_is_rejected(self):
        token = "ghp_" + "c" * 36
        self.assert_flagged(f"{REPO} token={token}")

    def test_mixed_public_repo_and_private_key_is_rejected(self):
        key_header = "-----BEGIN " + "PRIVATE KEY-----"
        self.assert_flagged(f"{REPO} {key_header}")

    def test_mixed_approved_namespace_and_private_marker_is_rejected(self):
        self.assert_flagged(f"{PACKAGE} {LEGACY_HOME}")

    def test_mixed_approved_domain_and_token_is_rejected(self):
        token = "ghp_" + "a" * 36
        self.assert_flagged(f"{DOMAIN} token={token}")

    def test_mixed_approved_path_and_private_key_is_rejected(self):
        key_header = "-----BEGIN " + "PRIVATE KEY-----"
        self.assert_flagged(f"{SOURCE_PATH}/key {key_header}")


class ExistingRuleTests(unittest.TestCase):
    def test_private_key_and_api_key_patterns_still_fire(self):
        key_header = "-----BEGIN " + "OPENSSH PRIVATE KEY-----"
        self.assertTrue(scan_text("x", key_header))
        self.assertTrue(scan_text("x", "export KEY=sk-" + "b" * 30))

    def test_line_numbers_are_reported(self):
        text = f"{PACKAGE}\n{NAME}\n"
        errors = scan_text("docs/example.md", text)
        self.assertEqual(errors, ["credential pattern: docs/example.md:2"])

    def test_scanner_source_does_not_flag_itself(self):
        source = Path(__file__).with_name("audit_repository.py").read_text(encoding="utf-8")
        self.assertEqual(scan_text("platform/quality/audit_repository.py", source), [])


if __name__ == "__main__":
    unittest.main(verbosity=2)
