#!/usr/bin/env python3
"""Run Android Lint over the Bazel modules of the Home Android client.

The Bazel rule (`android_lint` in lint.bzl) writes one JSON record per module
with the runfiles-relative paths of its sources, resources and manifest. This
script turns those records into the `project.xml` layout descriptor understood
by the standalone Android Lint CLI and runs lint once over the whole project,
so cross-module resource usage is analysed as a single build rather than as
unrelated modules.

The lint tool is not a Bazel input: it ships with the Android SDK
`cmdline-tools` package. The Bazel rule sets `ANDROID_HOME`/`ANDROID_SDK_ROOT`
in the test environment; CI installs the pinned `cmdline-tools;23.0`.
"""

import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET

MIN_SDK = "26"
TARGET_SDK = "36"
COMPILE_SDK = "36"
VERSION_CODE = "1"
VERSION_NAME = "0.1.0"

ANDROID_NS = "http://schemas.android.com/apk/res/android"


def runfiles_root():
    test_srcdir = os.environ.get("TEST_SRCDIR")
    test_workspace = os.environ.get("TEST_WORKSPACE")
    if not test_srcdir or not test_workspace:
        sys.exit("TEST_SRCDIR/TEST_WORKSPACE are not set; run through bazel test")
    return os.path.join(test_srcdir, test_workspace)


def find_lint():
    override = os.environ.get("ANDROID_LINT")
    if override:
        return override

    homes = [
        os.environ.get("ANDROID_HOME"),
        os.environ.get("ANDROID_SDK_ROOT"),
    ]
    candidates = []
    for home in homes:
        if not home:
            continue
        # Version-pinned package installed by _android.yml; keep it first.
        candidates.append(os.path.join(home, "cmdline-tools", "23.0", "bin", "lint"))
        candidates.append(os.path.join(home, "cmdline-tools", "latest", "bin", "lint"))
        cmdline = os.path.join(home, "cmdline-tools")
        if os.path.isdir(cmdline):
            for entry in sorted(os.listdir(cmdline), reverse=True):
                candidates.append(os.path.join(cmdline, entry, "bin", "lint"))
        candidates.append(os.path.join(home, "tools", "bin", "lint"))

    for candidate in candidates:
        if os.path.isfile(candidate) and os.access(candidate, os.X_OK):
            return candidate

    sys.exit(
        "Android Lint was not found. Install the pinned SDK package "
        "`cmdline-tools;23.0` (sdkmanager --install \"cmdline-tools;23.0\") "
        "or set ANDROID_LINT to the lint executable."
    )


def staged_generated_source(module, path, workdir):
    """Copy a generated source into a directory matching its package.

    Lint warns when a generated file's path does not match its package (the
    ViewBinding generator emits files into the Bazel package, not the Kotlin
    package tree). Staging a copy keeps the analysis output clean without
    changing build outputs.
    """
    with open(path, encoding="utf-8") as handle:
        text = handle.read()
    match = re.search(r"^package\s+([\w.]+)", text, re.MULTILINE)
    if not match:
        return path
    safe_name = "".join(c if c.isalnum() or c in "._-" else "_" for c in module["name"])
    dest_dir = os.path.join(workdir, "generated", safe_name, *match.group(1).split("."))
    os.makedirs(dest_dir, exist_ok=True)
    dest = os.path.join(dest_dir, os.path.basename(path))
    shutil.copyfile(path, dest)
    return dest


def merged_manifest(module, workdir):
    """Write a merged manifest carrying the SDK levels Bazel configures.

    Library source manifests are intentionally empty; the application manifest
    declares minSdk/targetSdk. Lint reads SDK levels from the *merged* manifest,
    so every module gets one. The application module also needs the version
    fields that `android_binary` injects through manifest_values.
    """
    package = module.get("package") or ""
    manifest = ET.Element("manifest", {"package": package})
    ET.SubElement(
        manifest,
        "uses-sdk",
        {"{%s}minSdkVersion" % ANDROID_NS: MIN_SDK, "{%s}targetSdkVersion" % ANDROID_NS: TARGET_SDK},
    )
    if module.get("app"):
        root_attrs = manifest.attrib
        root_attrs["{%s}versionCode" % ANDROID_NS] = VERSION_CODE
        root_attrs["{%s}versionName" % ANDROID_NS] = VERSION_NAME

    safe_name = "".join(c if c.isalnum() or c in "._-" else "_" for c in module["name"])
    path = os.path.join(workdir, safe_name + "-merged-manifest.xml")
    ET.ElementTree(manifest).write(path, encoding="utf-8", xml_declaration=True)
    return path


def main():
    if len(sys.argv) >= 2:
        records_path = sys.argv[1]
    elif os.environ.get("LINT_MODULES"):
        records_path = os.environ["LINT_MODULES"]
    else:
        sys.exit("usage: lint_runner.py <modules.jsonl> [lint.xml] (or set LINT_MODULES)")
    config_path = sys.argv[2] if len(sys.argv) >= 3 else None

    root = runfiles_root()
    if not os.path.isabs(records_path):
        records_path = os.path.join(root, records_path)

    with open(records_path, encoding="utf-8") as handle:
        modules = [json.loads(line) for line in handle if line.strip()]

    # Keep only modules that actually belong to the project graph. Empty
    # records would make lint treat Maven jars as modules.
    names = {module["name"] for module in modules}

    tmp = os.environ.get("TEST_TMPDIR")
    if not tmp:
        tmp = tempfile.mkdtemp(prefix="lint-")
    os.makedirs(tmp, exist_ok=True)

    project = ET.Element("project", {"android": "true", "incomplete": "false"})
    ET.SubElement(project, "root", {"dir": root})

    for module in modules:
        attrs = {
            "name": module["name"],
            "android": "true" if module.get("android") else "false",
            "compile-sdk-version": COMPILE_SDK,
            "javaLanguage": "17",
            "kotlinLanguage": "2.4",
        }
        if module.get("android"):
            attrs["library"] = "false" if module.get("app") else "true"
        element = ET.SubElement(project, "module", attrs)

        generated = set(module.get("generated_srcs", []))
        for src in module.get("srcs", []):
            path = os.path.join(root, src)
            attrs = {"file": path}
            if src in generated:
                attrs["generated"] = "true"
                attrs["file"] = staged_generated_source(module, path, tmp)
            ET.SubElement(element, "src", attrs)
        for resource in module.get("resources", []):
            ET.SubElement(element, "resource", {"file": os.path.join(root, resource)})
        if module.get("manifest"):
            ET.SubElement(element, "manifest", {"file": os.path.join(root, module["manifest"])})
            ET.SubElement(element, "merged-manifest", {"file": merged_manifest(module, tmp)})
        for jar in module.get("jars", []):
            jar_path = os.path.join(root, jar)
            if os.path.exists(jar_path):
                ET.SubElement(element, "classpath", {"jar": jar_path})
        for dep in module.get("deps", []):
            if dep in names:
                ET.SubElement(element, "dep", {"module": dep})

    project_path = os.path.join(tmp, "project.xml")
    ET.ElementTree(project).write(project_path, encoding="utf-8", xml_declaration=True)

    config = config_path or os.path.join(root, "projects/mobile/android/lint.xml")
    lint = find_lint()
    command = [
        lint,
        "--project",
        project_path,
        "--compile-sdk-version",
        COMPILE_SDK,
        "--sdk-home",
        os.environ.get("ANDROID_HOME") or os.environ.get("ANDROID_SDK_ROOT") or "",
        "--config",
        config,
        "--exitcode",
        "-Werror",
        "--text",
        "stdout",
        "--quiet",
    ]
    # --sdk-home may be empty when lint finds the SDK itself; drop the empty
    # argument pair rather than passing an invalid path.
    if not command[command.index("--sdk-home") + 1]:
        index = command.index("--sdk-home")
        del command[index:index + 2]

    env = os.environ.copy()
    env.setdefault("ANDROID_USER_HOME", os.path.join(tmp, "android-user"))
    # Keep lint's analytics settings inside the sandbox (user.home is read-only there).
    env.setdefault("LINT_OPTS", "-Duser.home=" + tmp)
    result = subprocess.run(command, check=False, env=env)
    return result.returncode


if __name__ == "__main__":
    sys.exit(main())
