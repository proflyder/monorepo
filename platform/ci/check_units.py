#!/usr/bin/env python3
"""Validate the CI unit map and prove no source/test file falls out of CI.

Checks:
1. schema: unique ids, known platforms, non-empty paths, required sections;
2. android units declare Bazel roots and every test/build target lives under
   one of those roots;
3. `depends_on` edges point at existing units;
4. every tracked test/source/build file is owned by at least one unit path or
   an explicit invalidation rule, so a new test cannot silently stop running.

Run locally with `python3 platform/ci/check_units.py`; CI runs it in the
quality job and `make test` runs it through the unit tests.
"""
from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

from affected import Config, load_config, path_matches

DEFAULT_UNITS = Path(__file__).resolve().with_name("units.yml")
KNOWN_PLATFORMS = {"go", "android", "python", "quality"}

# Suffixes/names that must be routed somewhere: compiled sources, tests, build
# files and shell entry points.
OWNED_SUFFIXES = (".go", ".py", ".kt", ".kts", ".sh", ".bzl")
OWNED_NAMES = ("BUILD.bazel",)


def tracked_files(root: Path) -> list[str]:
    result = subprocess.run(
        ["git", "-C", str(root), "ls-files", "-z"],
        capture_output=True,
        check=True,
    )
    return [item for item in result.stdout.decode("utf-8").split("\0") if item]


def needs_owner(path: str) -> bool:
    name = path.rsplit("/", 1)[-1]
    if name in OWNED_NAMES:
        return True
    return name.endswith(OWNED_SUFFIXES)


def validate_schema(config: Config, root: Path | None = None) -> list[str]:
    root = root or Path(__file__).resolve().parents[2]
    problems: list[str] = []
    ids = [unit.id for unit in config.units]
    duplicates = sorted({item for item in ids if ids.count(item) > 1})
    if duplicates:
        problems.append(f"duplicate unit ids: {duplicates}")

    for rule in config.invalidations:
        prefix = f"invalidation {rule.paths}"
        if not rule.paths:
            problems.append(f"{prefix}: no paths declared")
        if not rule.platforms:
            problems.append(f"{prefix}: no platforms declared")
        for platform in rule.platforms:
            if platform != "*" and platform not in KNOWN_PLATFORMS:
                problems.append(f"{prefix}: unknown platform {platform!r}")
        if not isinstance(rule.deploy, bool):
            problems.append(f"{prefix}: deploy must be a bool")

    for unit in config.units:
        prefix = f"unit {unit.id}"
        if unit.platform not in KNOWN_PLATFORMS:
            problems.append(f"{prefix}: unknown platform {unit.platform!r}")
        if not unit.paths:
            problems.append(f"{prefix}: no paths declared")
        for pattern in unit.paths:
            if pattern in ("*", "**", "**/*"):
                problems.append(f"{prefix}: path pattern {pattern!r} is too broad")
        for dep in unit.depends_on:
            if dep not in ids:
                problems.append(f"{prefix}: depends_on unknown unit {dep!r}")
        if unit.deploy_images and not unit.deploy_workloads:
            problems.append(f"{prefix}: deploy.images without deploy.workloads")
        if unit.deploy_workloads and not unit.deploy_images:
            problems.append(f"{prefix}: deploy.workloads without deploy.images")
        if unit.deploy_images and unit.deploy_builder not in ("bazel", "docker"):
            problems.append(f"{prefix}: unknown deploy.builder {unit.deploy_builder!r}")
        if unit.deploy_images:
            if not unit.deploy_namespace:
                problems.append(f"{prefix}: deploy.namespace is required")
            if not unit.deploy_script:
                problems.append(f"{prefix}: deploy.script is required")
            elif not (root / unit.deploy_script).is_file():
                problems.append(f"{prefix}: deploy.script does not exist: {unit.deploy_script}")
            if not unit.deploy_env_key or not unit.deploy_env_file:
                problems.append(
                    f"{prefix}: deploy.env_key and deploy.env_file are required "
                    "(OpenBao secret/ci/deploy-home key and local filename)"
                )
            if not unit.deploy_externalsecrets:
                problems.append(f"{prefix}: deploy.externalsecrets is required")
            if unit.deploy_builder == "bazel" and not unit.deploy_target_root.startswith("//"):
                problems.append(
                    f"{prefix}: deploy.target_root must be a Bazel package (//pkg)"
                )
        if unit.platform == "go":
            if not unit.go_packages:
                problems.append(f"{prefix}: go unit without packages")
        elif unit.platform == "android":
            if not unit.bazel_roots:
                problems.append(f"{prefix}: android unit without bazel.roots")
            for target in unit.test_targets + unit.build_targets:
                if not any(
                    target.startswith(bazel_root + ":") or target.startswith(bazel_root + "/")
                    for bazel_root in unit.bazel_roots
                ):
                    problems.append(f"{prefix}: target {target} is outside bazel.roots")
            if not unit.test_targets:
                problems.append(f"{prefix}: android unit without test_targets")
        elif unit.platform == "python":
            if not unit.suites and not unit.compile_dirs:
                problems.append(f"{prefix}: python unit without suites or compile dirs")
            for suite in unit.suites:
                if not (root / suite).is_dir():
                    problems.append(f"{prefix}: suite directory missing: {suite}")
                elif not list((root / suite).glob("test_*.py")):
                    problems.append(
                        f"{prefix}: suite {suite} has no test_*.py; use python.compile for "
                        "syntax-only directories"
                    )
            for directory in unit.compile_dirs:
                if not (root / directory).is_dir():
                    problems.append(f"{prefix}: compile directory missing: {directory}")
    return problems


def coverage_problems(config: Config, files: list[str]) -> list[str]:
    owned_patterns = [pattern for unit in config.units for pattern in unit.paths]
    owned_patterns += [pattern for rule in config.invalidations for pattern in rule.paths]
    uncovered = [
        path for path in files if needs_owner(path) and not path_matches(path, owned_patterns)
    ]
    if not uncovered:
        return []
    lines = [
        f"{len(uncovered)} tracked file(s) are not owned by any unit in platform/ci/units.yml:"
    ]
    lines.extend(f"  - {path}" for path in uncovered[:40])
    if len(uncovered) > 40:
        lines.append(f"  ... and {len(uncovered) - 40} more")
    lines.append("Add the path to a unit (or an invalidation rule) so CI keeps running its tests.")
    return lines


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default=str(Path(__file__).resolve().parents[2]))
    parser.add_argument("--units", default=str(DEFAULT_UNITS))
    parser.add_argument("--quiet", action="store_true")
    args = parser.parse_args(argv)

    root = Path(args.root).resolve()
    config = load_config(Path(args.units))
    problems = validate_schema(config, root) + coverage_problems(config, tracked_files(root))
    if problems:
        for problem in problems:
            print(problem, file=sys.stderr)
        return 1
    if not args.quiet:
        counts = ", ".join(
            f"{platform}={len(config.by_platform(platform))}" for platform in sorted(KNOWN_PLATFORMS)
        )
        print(f"units OK ({counts}); every tracked source/test file is routed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
