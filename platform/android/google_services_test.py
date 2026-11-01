#!/usr/bin/env python3
"""Tests for google_services.py. Run: python3 -I platform/android/google_services_test.py"""

import json
import os
import re
import subprocess
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
GENERATOR = os.path.join(HERE, "google_services.py")

CONFIG = {
    "project_info": {
        "project_number": "1000000000000",
        "project_id": "example-project",
        "storage_bucket": "example-project.firebasestorage.app",
    },
    "client": [
        {
            "client_info": {
                "mobilesdk_app_id": "1:1000000000000:android:0123456789abcdef",
                "android_client_info": {"package_name": "dev.example.other"},
            },
            "api_key": [{"current_key": "ExampleKeyOtherClient"}],
            "oauth_client": [],
        },
        {
            "client_info": {
                "mobilesdk_app_id": "1:1000000000000:android:fedcba9876543210",
                "android_client_info": {"package_name": "dev.example.home"},
            },
            "api_key": [{"current_key": "ExampleKeyHomeClient"}],
            "oauth_client": [],
        },
    ],
    "configuration_version": "1",
}


def run_generator(config=None, package="dev.example.home", version_name="0.1.0",
                  version_code="1"):
    with tempfile.TemporaryDirectory() as tmp:
        input_path = os.path.join(tmp, "google-services.json")
        if config is not None:
            with open(input_path, "w", encoding="utf-8") as handle:
                json.dump(config, handle)
        output_path = os.path.join(tmp, "res", "values", "google_services.xml")
        args = [sys.executable, "-I", GENERATOR, "--package", package,
                "--version-name", version_name, "--version-code", version_code,
                "--output", output_path]
        if config is not None:
            args.extend(["--input", input_path])
        result = subprocess.run(args, capture_output=True, text=True)
        xml = ""
        if os.path.exists(output_path):
            with open(output_path, encoding="utf-8") as handle:
                xml = handle.read()
        return result, xml


def string_value(xml, name):
    match = re.search(
        r'<string name="%s" translatable="false">([^<]*)</string>' % re.escape(name),
        xml,
    )
    return match.group(1) if match else None


class GoogleServicesTest(unittest.TestCase):
    def test_selects_requested_package_and_emits_gradle_equivalent_strings(self):
        result, xml = run_generator(CONFIG)
        self.assertEqual(result.returncode, 0, result.stderr)

        self.assertEqual(
            string_value(xml, "google_app_id"),
            "1:1000000000000:android:fedcba9876543210",
        )
        self.assertEqual(string_value(xml, "gcm_defaultSenderId"), "1000000000000")
        self.assertEqual(string_value(xml, "google_api_key"), "ExampleKeyHomeClient")
        self.assertEqual(
            string_value(xml, "google_crash_reporting_api_key"),
            "ExampleKeyHomeClient",
        )
        self.assertEqual(
            string_value(xml, "google_storage_bucket"),
            "example-project.firebasestorage.app",
        )
        self.assertEqual(string_value(xml, "project_id"), "example-project")
        self.assertNotIn("ExampleKeyOtherClient", xml)

    def test_emits_crashlytics_build_id(self):
        result, xml = run_generator(CONFIG)
        self.assertEqual(result.returncode, 0, result.stderr)
        build_id = string_value(
            xml, "com.google.firebase.crashlytics.mapping_file_id"
        )
        self.assertIsNotNone(build_id)
        self.assertRegex(build_id, r"^[0-9a-f]{32}$")

    def test_build_id_is_deterministic_and_changes_with_version(self):
        _, first = run_generator(CONFIG, version_code="1", version_name="0.1.0")
        _, second = run_generator(CONFIG, version_code="1", version_name="0.1.0")
        _, third = run_generator(CONFIG, version_code="2", version_name="0.1.0")
        name = "com.google.firebase.crashlytics.mapping_file_id"
        self.assertEqual(string_value(first, name), string_value(second, name))
        self.assertNotEqual(string_value(first, name), string_value(third, name))

    def test_missing_package_is_an_error(self):
        result, xml = run_generator(CONFIG, package="dev.example.absent")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("no client for package", result.stderr)
        self.assertEqual(xml, "")

    def test_absent_config_warns_and_emits_no_firebase_strings(self):
        result, xml = run_generator(config=None)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("WARNING", result.stderr)
        for name in (
            "google_app_id",
            "gcm_defaultSenderId",
            "google_api_key",
            "google_crash_reporting_api_key",
            "google_storage_bucket",
            "project_id",
            "com.google.firebase.crashlytics.mapping_file_id",
        ):
            self.assertNotIn(name, xml)
        self.assertIn("<resources>", xml)

    def test_absent_file_with_explicit_input_warns(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = os.path.join(tmp, "google_services.xml")
            result = subprocess.run(
                [sys.executable, "-I", GENERATOR, "--package", "dev.example.home",
                 "--input", os.path.join(tmp, "missing.json"), "--output", out],
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("WARNING", result.stderr)


if __name__ == "__main__":
    unittest.main()
