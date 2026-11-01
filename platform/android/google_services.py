#!/usr/bin/env python3
"""Generate Android string resources from google-services.json.

The Firebase Gradle plugin normally turns `google-services.json` into
`res/values/google_services.xml` (and a few other resources) at build time.
The Home Android app is built with Bazel only, so this tool does the same
subset of that transformation:

  * `google_app_id`                 <- mobilesdk_app_id
  * `gcm_defaultSenderId`           <- project_info.project_number
  * `google_api_key`                <- client.api_key.current_key
  * `google_crash_reporting_api_key`<- client.api_key.current_key
  * `google_storage_bucket`         <- project_info.storage_bucket
  * `project_id`                    <- project_info.project_id

It also emits the Crashlytics build id that the Crashlytics Gradle plugin
normally writes (`com.google.firebase.crashlytics.mapping_file_id`). The
Crashlytics SDK only needs this string at init: `AppData` reads
`com.google.firebase.crashlytics.mapping_file_id`, falling back to the legacy
`com.crashlytics.android.build_id`, and refuses to start when both are absent.
The value must be stable for one build and change when the build changes, so
it is derived from the app id, version name, and version code (32 hex chars).

The JSON is optional in the repository: without it the output is a valid,
empty values file and a warning goes to stderr, so CI and clean checkouts keep
building. Firebase then stays uninitialized.

Run the unit tests with `python3 -I platform/android/google_services_test.py`.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import xml.sax.saxutils

FIREBASE_RESOURCES = (
    "google_app_id",
    "gcm_defaultSenderId",
    "google_api_key",
    "google_crash_reporting_api_key",
    "google_storage_bucket",
    "project_id",
)

CRASHLYTICS_BUILD_ID_RESOURCE = "com.google.firebase.crashlytics.mapping_file_id"

_EMPTY_VALUES = """<?xml version="1.0" encoding="utf-8"?>
<resources>
    <!-- google-services.json is not linked; Firebase stays uninitialized. -->
</resources>
"""


class ConfigError(Exception):
    """google-services.json does not describe the requested package."""


def _client_for_package(config: dict, package_name: str) -> dict:
    for client in config.get("client", []):
        info = client.get("client_info", {})
        android = info.get("android_client_info", {})
        if android.get("package_name") == package_name:
            return client
    raise ConfigError(
        "google-services.json has no client for package %r" % package_name
    )


def _build_id(app_id: str, version_name: str, version_code: str) -> str:
    material = "\n".join((app_id, version_name, version_code))
    return hashlib.sha256(material.encode("utf-8")).hexdigest()[:32]


def _string_element(name: str, value: str) -> str:
    escaped = xml.sax.saxutils.escape(value, {'"': "&quot;"})
    return '    <string name="%s" translatable="false">%s</string>' % (
        name,
        escaped,
    )


def generate(input_path: str, package_name: str, version_name: str,
             version_code: str) -> str:
    """Return the values XML for one package.

    Raises ConfigError when the JSON is present but does not contain the
    package. An absent or empty file yields an empty values file.
    """
    if not input_path or not os.path.exists(input_path) or os.path.getsize(input_path) == 0:
        return _EMPTY_VALUES

    with open(input_path, encoding="utf-8") as handle:
        config = json.load(handle)

    project_info = config.get("project_info", {})
    client = _client_for_package(config, package_name)
    api_key_entries = client.get("api_key", [])
    api_key = api_key_entries[0].get("current_key", "") if api_key_entries else ""
    app_id = client.get("client_info", {}).get("mobilesdk_app_id", "")

    values = {
        "google_app_id": app_id,
        "gcm_defaultSenderId": project_info.get("project_number", ""),
        "google_api_key": api_key,
        "google_crash_reporting_api_key": api_key,
        "google_storage_bucket": project_info.get("storage_bucket", ""),
        "project_id": project_info.get("project_id", ""),
    }

    lines = ['<?xml version="1.0" encoding="utf-8"?>', "<resources>"]
    for name in FIREBASE_RESOURCES:
        lines.append(_string_element(name, values[name]))
    lines.append(
        _string_element(
            CRASHLYTICS_BUILD_ID_RESOURCE,
            _build_id(app_id, version_name, version_code),
        )
    )
    lines.append("</resources>")
    return "\n".join(lines) + "\n"


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--input", default="", help="path to google-services.json")
    parser.add_argument("--package", required=True, help="Android application id")
    parser.add_argument("--version-name", default="0.1.0")
    parser.add_argument("--version-code", default="1")
    parser.add_argument("--output", required=True)
    args = parser.parse_args(argv)

    if not args.input or not os.path.exists(args.input):
        sys.stderr.write(
            "WARNING: google-services.json is not linked; generating an empty "
            "values file and leaving Firebase uninitialized. Link "
            "platform/android/keys/google-services.json to enable Crashlytics.\n"
        )

    try:
        xml = generate(args.input, args.package, args.version_name, args.version_code)
    except (ConfigError, ValueError) as error:
        sys.stderr.write("ERROR: %s\n" % error)
        return 1

    directory = os.path.dirname(args.output)
    if directory:
        os.makedirs(directory, exist_ok=True)
    with open(args.output, "w", encoding="utf-8") as handle:
        handle.write(xml)
    return 0


if __name__ == "__main__":
    sys.exit(main())
