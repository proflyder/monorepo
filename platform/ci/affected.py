#!/usr/bin/env python3
"""Compute the CI units affected by a change range.

Routing is declarative for paths (`platform/ci/units.yml`) and dependency
based for code:

* **go** - `go list -deps` of each unit's packages. A change in a shared
  package (for example `packages/proto`) selects every unit that imports it.
* **android** - reverse dependencies from the Bazel loading graph
  (`bazel query --output=xml`). Rule labels are mapped back to unit names by
  `bazel.roots`, so a change in `packages/kotlin` selects only the
  applications that actually depend on it. This needs no JDK and no Android
  SDK because it never analyses targets.
* **python** - path rules plus optional declarative `depends_on`.
* **quality** - always runs.

Any failure to compute a dependency closure degrades to "every unit of that
platform" (never to "nothing"), as required for a required-check gate.

Usage:

    platform/ci/affected.py --base origin/develop --head HEAD --format text
    platform/ci/affected.py --base "$BASE_SHA" --format github
    platform/ci/affected.py --changed-file solutions/x/y.go --format json

Exit status is 0 unless the range cannot be resolved at all.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import xml.etree.ElementTree as ET
from dataclasses import dataclass, field
from pathlib import Path

DEFAULT_UNITS = Path(__file__).resolve().with_name("units.yml")
PLATFORMS = ("go", "android", "python", "quality")


# --------------------------------------------------------------------------
# Configuration
# --------------------------------------------------------------------------
@dataclass
class Unit:
    id: str
    platform: str
    title: str
    paths: list[str]
    bazel_roots: list[str] = field(default_factory=list)
    test_targets: list[str] = field(default_factory=list)
    build_targets: list[str] = field(default_factory=list)
    artifact: str = ""
    artifact_name: str = ""
    go_module: str = "."
    go_packages: list[str] = field(default_factory=list)
    go_build: list[str] = field(default_factory=list)
    go_race: bool = False
    suites: list[str] = field(default_factory=list)
    compile_dirs: list[str] = field(default_factory=list)
    depends_on: list[str] = field(default_factory=list)
    # Deployable units own container images and Kubernetes workloads. An empty
    # `deploy_images` means "this unit is not deployed by deploy-home".
    deploy_images: list[str] = field(default_factory=list)
    deploy_workloads: list[str] = field(default_factory=list)
    deploy_builder: str = "bazel"
    # Bazel package that owns the `<image>_image`/`<image>_push` targets.
    deploy_target_root: str = ""
    # Kubernetes namespace, deploy entrypoint and the OpenBao `secret/ci/
    # deploy-home` key/file that carries the private deploy-time env.
    deploy_namespace: str = ""
    deploy_script: str = ""
    deploy_env_key: str = ""
    deploy_env_file: str = ""
    # ExternalSecrets that must be Ready before the deploy touches workloads.
    deploy_externalsecrets: list[str] = field(default_factory=list)


@dataclass
class Invalidation:
    paths: list[str]
    platforms: list[str]


@dataclass
class Config:
    units: list[Unit]
    invalidations: list[Invalidation]
    deploy_control_units: list[str] = field(default_factory=list)
    deploy_control_paths: list[str] = field(default_factory=list)

    def by_platform(self, platform: str) -> list[Unit]:
        return [u for u in self.units if u.platform == platform]

    def get(self, unit_id: str) -> Unit:
        for unit in self.units:
            if unit.id == unit_id:
                return unit
        raise KeyError(unit_id)


def load_config(path: Path) -> Config:
    import yaml  # PyYAML is pinned in requirements-test.txt

    data = yaml.safe_load(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict) or data.get("version") != 1:
        raise ValueError(f"{path}: expected a version: 1 mapping")
    units: list[Unit] = []
    for raw in data.get("units") or []:
        bazel = raw.get("bazel") or {}
        go = raw.get("go") or {}
        python = raw.get("python") or {}
        deploy = raw.get("deploy") or {}
        units.append(
            Unit(
                id=raw["id"],
                platform=raw["platform"],
                title=raw.get("title", raw["id"]),
                paths=list(raw.get("paths") or []),
                bazel_roots=list(bazel.get("roots") or []),
                test_targets=list(bazel.get("test_targets") or []),
                build_targets=list(bazel.get("build_targets") or []),
                artifact=bazel.get("artifact", ""),
                artifact_name=bazel.get("artifact_name", ""),
                go_module=go.get("module", "."),
                go_packages=list(go.get("packages") or []),
                go_build=list(go.get("build") or []),
                go_race=bool(go.get("race", False)),
                suites=list(python.get("suites") or []),
                compile_dirs=list(python.get("compile") or []),
                depends_on=list(raw.get("depends_on") or []),
                deploy_images=list(deploy.get("images") or []),
                deploy_workloads=list(deploy.get("workloads") or []),
                deploy_builder=str(deploy.get("builder") or "bazel"),
                deploy_target_root=str(deploy.get("target_root") or ""),
                deploy_namespace=str(deploy.get("namespace") or ""),
                deploy_script=str(deploy.get("script") or ""),
                deploy_env_key=str(deploy.get("env_key") or ""),
                deploy_env_file=str(deploy.get("env_file") or ""),
                deploy_externalsecrets=list(deploy.get("externalsecrets") or []),
            )
        )
    invalidations = [
        Invalidation(paths=list(item.get("paths") or []), platforms=list(item.get("platforms") or []))
        for item in data.get("invalidations") or []
    ]
    deploy_control = data.get("deploy_control") or {}
    return Config(
        units=units,
        invalidations=invalidations,
        deploy_control_units=list(deploy_control.get("units") or []),
        deploy_control_paths=list(deploy_control.get("paths") or []),
    )


# --------------------------------------------------------------------------
# Path matching
# --------------------------------------------------------------------------
def _glob_to_regex(pattern: str) -> re.Pattern[str]:
    out: list[str] = []
    i = 0
    while i < len(pattern):
        char = pattern[i]
        if pattern.startswith("**/", i):
            out.append("(?:.*/)?")
            i += 3
            continue
        if pattern.startswith("**", i):
            out.append(".*")
            i += 2
            continue
        if char == "*":
            out.append("[^/]*")
        elif char == "?":
            out.append("[^/]")
        else:
            out.append(re.escape(char))
        i += 1
    return re.compile("^" + "".join(out) + "$")


_GLOB_CACHE: dict[str, re.Pattern[str]] = {}


def path_matches(path: str, patterns: list[str]) -> bool:
    for pattern in patterns:
        regex = _GLOB_CACHE.get(pattern)
        if regex is None:
            regex = _glob_to_regex(pattern)
            _GLOB_CACHE[pattern] = regex
        if regex.match(path):
            return True
    return False


# --------------------------------------------------------------------------
# Git
# --------------------------------------------------------------------------
def _git(root: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(root), *args],
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError(f"git {' '.join(args)} failed: {result.stderr.strip()}")
    return result.stdout


def changed_files(root: Path, base: str, head: str) -> list[str]:
    """Files touched between base and head, renames expanded to delete+add."""
    out = _git(root, "diff", "--name-only", "--no-renames", f"{base}...{head}")
    return sorted({line.strip() for line in out.splitlines() if line.strip()})


def resolve_base(root: Path, requested: str | None) -> str | None:
    """Pick a base revision, or None when the whole tree must run."""
    if requested:
        if set(requested) <= {"0"}:
            return None
        try:
            return _git(root, "rev-parse", "--verify", f"{requested}^{{commit}}").strip()
        except RuntimeError:
            pass  # fall through to the develop merge-base
    for candidate in ("origin/develop", "develop"):
        try:
            merge_base = _git(root, "merge-base", "HEAD", candidate).strip()
            if merge_base:
                return merge_base
        except RuntimeError:
            continue
    return None


# --------------------------------------------------------------------------
# Go dependency closure
# --------------------------------------------------------------------------
def go_package_dirs(root: Path, unit: Unit) -> set[str]:
    """Absolute directories of every package the unit compiles or imports."""
    module_dir = (root / unit.go_module).resolve()
    result = subprocess.run(
        ["go", "list", "-deps", "-f", "{{.Dir}}", *unit.go_packages],
        cwd=module_dir,
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"go list failed for unit {unit.id}: {result.stderr.strip().splitlines()[-1:] or ''}"
        )
    return {line.strip() for line in result.stdout.splitlines() if line.strip()}


# --------------------------------------------------------------------------
# Bazel loading graph
# --------------------------------------------------------------------------
BAZEL_GRAPH_QUERY = 'kind(".*", //...)'


@dataclass
class BazelGraph:
    """Rule dependency graph from `bazel query --output=xml`.

    Only loading-phase information is used, so the query works without a JDK
    or the Android SDK.
    """

    rules: dict[str, str]  # rule label -> rule class
    inputs: dict[str, set[str]]  # rule label -> rule-input labels (files, deps)
    package_rules: dict[str, set[str]]  # "//pkg" -> rule labels in that package
    path_index: dict[str, set[str]] = field(default_factory=dict)  # file path -> labels

    def __post_init__(self) -> None:
        if not self.path_index:
            index: dict[str, set[str]] = {}
            for label, deps in self.inputs.items():
                index.setdefault(self.label_path(label), set()).add(label)
                for dep in deps:
                    index.setdefault(self.label_path(dep), set()).add(dep)
            self.path_index = index

    @classmethod
    def from_xml(cls, text: str) -> "BazelGraph":
        root = ET.fromstring(text)
        if root.tag != "query":  # pragma: no cover - defensive
            raise ValueError("unexpected bazel query XML root")
        rules: dict[str, str] = {}
        inputs: dict[str, set[str]] = {}
        package_rules: dict[str, set[str]] = {}
        for rule in root.findall("rule"):
            label = rule.get("name")
            if not label:
                continue
            rules[label] = rule.get("class", "")
            inputs[label] = {
                inp.get("name", "") for inp in rule.findall("rule-input") if inp.get("name")
            }
            package_rules.setdefault(label.rsplit(":", 1)[0], set()).add(label)
        return cls(rules=rules, inputs=inputs, package_rules=package_rules)

    def dependents(self) -> dict[str, set[str]]:
        reverse: dict[str, set[str]] = {}
        for label, deps in self.inputs.items():
            for dep in deps:
                reverse.setdefault(dep, set()).add(label)
        return reverse

    @staticmethod
    def label_path(label: str) -> str:
        """`//pkg:sub/file.kt` -> `pkg/sub/file.kt` (repo-relative)."""
        if not label.startswith("//"):
            return label
        return label[2:].replace(":", "/", 1)

    def labels_for_path(self, path: str) -> set[str]:
        return self.path_index.get(path, set())

    def package_for_path(self, path: str) -> str:
        """Nearest Bazel package (ancestor directory with a BUILD file)."""
        package = path.rpartition("/")[0]
        while True:
            key = f"//{package}" if package else "//"
            if key in self.package_rules:
                return key
            if not package:
                return "//"
            package = package.rpartition("/")[0]

    def affected_rules(self, changed: list[str]) -> set[str]:
        reverse = self.dependents()
        seeds: set[str] = set()
        for path in changed:
            labels = self.labels_for_path(path)
            if labels:
                seeds.update(labels)
            else:
                # Deleted file, new file or BUILD/.bzl change: fall back to the
                # whole package, which over-approximates safely.
                seeds.update(self.package_rules.get(self.package_for_path(path), set()))
        affected: set[str] = set()
        stack = list(seeds)
        while stack:
            node = stack.pop()
            if node in affected:
                continue
            affected.add(node)
            stack.extend(reverse.get(node, ()))
        return {label for label in affected if label in self.rules}


def load_bazel_graph(root: Path, bazel: str, timeout: int = 300) -> BazelGraph:
    if not (shutil.which(bazel) or Path(bazel).exists()):
        raise RuntimeError(f"bazel binary not found: {bazel}")
    result = subprocess.run(
        [bazel, "query", "--output=xml", "--noshow_progress", BAZEL_GRAPH_QUERY],
        cwd=root,
        text=True,
        capture_output=True,
        timeout=timeout,
        check=False,
    )
    if result.returncode != 0:
        tail = " ".join(result.stderr.strip().splitlines()[-3:])
        raise RuntimeError(f"bazel query failed: {tail}")
    return BazelGraph.from_xml(result.stdout)


# --------------------------------------------------------------------------
# Routing
# --------------------------------------------------------------------------
@dataclass
class PlatformRoute:
    platform: str
    mode: str
    units: list[Unit] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)


@dataclass
class Route:
    base: str | None
    head: str
    changed: list[str]
    routes: dict[str, PlatformRoute]
    docs_only: bool
    deploy_control: bool
    warnings: list[str] = field(default_factory=list)


GLOBAL = "*"


def _invalidated_platforms(config: Config, changed: list[str]) -> set[str]:
    hit: set[str] = set()
    for rule in config.invalidations:
        if any(path_matches(path, rule.paths) for path in changed):
            for platform in rule.platforms:
                if platform == GLOBAL:
                    hit.update(PLATFORMS)
                else:
                    hit.update(p.strip() for p in platform.split(",") if p.strip())
    return hit


def _expand_depends_on(config: Config, selected: list[Unit]) -> list[Unit]:
    by_id = {unit.id: unit for unit in config.units}
    order: list[Unit] = []
    seen: set[str] = set()
    stack = list(selected)
    while stack:
        unit = stack.pop()
        if unit.id in seen:
            continue
        seen.add(unit.id)
        order.append(unit)
        stack.extend(by_id[dep] for dep in unit.depends_on if dep in by_id)
    return [u for u in config.units if u.id in seen]


def route(
    config: Config,
    changed: list[str],
    root: Path,
    base: str | None,
    head: str = "HEAD",
    bazel: str = "bazel",
    graph: BazelGraph | None = None,
    graph_loader=None,
) -> Route:
    """Route changed paths onto units. Exposed for tests."""
    invalidated = _invalidated_platforms(config, changed)
    changed_dirs = {
        str((root / Path(path).parent).resolve()) for path in changed if path
    }
    routes: dict[str, PlatformRoute] = {}
    warnings: list[str] = []

    # --- go: dependency closure from the Go toolchain ---------------------
    go_route = PlatformRoute("go", "go-list")
    go_units = config.by_platform("go")
    go_dirty = (
        GLOBAL in invalidated
        or "go" in invalidated
        or any(
            path.endswith((".go", "go.mod", "go.sum", "go.work"))
            or path_matches(path, [pattern for unit in go_units for pattern in unit.paths])
            for path in changed
        )
    )
    if not go_dirty:
        go_route.mode = "paths"
    elif GLOBAL in invalidated or "go" in invalidated:
        go_route.mode = "all"
        go_route.units = list(go_units)
    else:
        for unit in go_units:
            matched_paths = any(path_matches(path, unit.paths) for path in changed)
            try:
                deps = go_package_dirs(root, unit)
            except Exception as exc:  # noqa: BLE001 - degrade to "all"
                go_route.mode = "all"
                go_route.units = list(config.by_platform("go"))
                go_route.warnings.append(f"go list failed ({exc}); running every go unit")
                break
            if matched_paths or changed_dirs & deps:
                go_route.units.append(unit)
    go_route.units = _expand_depends_on(config, go_route.units)
    routes["go"] = go_route

    # --- android: Bazel reverse dependencies ------------------------------
    android_route = PlatformRoute("android", "bazel-graph")
    android_units = config.by_platform("android")
    if GLOBAL in invalidated or "android" in invalidated:
        android_route.mode = "all"
        android_route.units = list(android_units)
    else:
        touched = [
            path
            for path in changed
            if any(path_matches(path, unit.paths) for unit in android_units)
        ]
        if not touched:
            android_route.mode = "paths"
        else:
            try:
                if graph is None:
                    if graph_loader is None:
                        raise RuntimeError("no graph loader configured")
                    graph = graph_loader()
                affected = graph.affected_rules(changed)
                for unit in android_units:
                    if any(path_matches(path, unit.paths) for path in changed) or any(
                        label.startswith(root_label + ":") or label.startswith(root_label + "/")
                        for label in affected
                        for root_label in unit.bazel_roots
                    ):
                        android_route.units.append(unit)
            except Exception as exc:  # noqa: BLE001 - degrade to "all"
                android_route.mode = "all"
                android_route.units = list(android_units)
                android_route.warnings.append(
                    f"bazel graph failed ({exc}); running every android unit"
                )
    android_route.units = _expand_depends_on(config, android_route.units)
    routes["android"] = android_route

    # --- python: declarative path rules ----------------------------------
    python_route = PlatformRoute("python", "paths")
    for unit in config.by_platform("python"):
        if GLOBAL in invalidated or "python" in invalidated:
            python_route.units.append(unit)
            python_route.mode = "all"
        elif any(path_matches(path, unit.paths) for path in changed):
            python_route.units.append(unit)
    python_route.units = _expand_depends_on(config, python_route.units)
    routes["python"] = python_route

    # --- quality: always --------------------------------------------------
    quality_route = PlatformRoute("quality", "always", list(config.by_platform("quality")))
    routes["quality"] = quality_route

    docs_only = bool(changed) and all(
        path.endswith((".md", ".mdx")) or path.startswith("docs/") for path in changed
    )
    affected_go = {unit.id for unit in routes["go"].units}
    deploy_control = bool(affected_go & set(config.deploy_control_units)) or any(
        path_matches(path, config.deploy_control_paths) for path in changed
    )

    for route_ in routes.values():
        warnings.extend(route_.warnings)
    return Route(
        base=base,
        head=head,
        changed=changed,
        routes=routes,
        docs_only=docs_only,
        deploy_control=deploy_control,
        warnings=warnings,
    )


def _space(items: list[str]) -> str:
    return " ".join(items)


def matrix_entries(route_: Route, config: Config) -> dict[str, list[dict[str, object]]]:
    """Matrix payloads for the reusable platform workflows."""
    go = [
        {
            "id": unit.id,
            "workdir": unit.go_module,
            "packages": _space(unit.go_packages),
            "race": unit.go_race,
            "build_packages": _space(unit.go_build),
        }
        for unit in route_.routes["go"].units
    ]
    # Every android/kotlin target so the precise target step can see shared
    # libraries that belong to another unit than the application it builds.
    android_roots = sorted(
        {root for unit in config.by_platform("android") for root in unit.bazel_roots}
    )
    universe = " + ".join(root + "/..." for root in android_roots)
    android = [
        {
            "id": unit.id,
            "test_targets": _space(unit.test_targets),
            "build_targets": _space(unit.build_targets),
            "artifact": unit.artifact,
            "artifact_name": unit.artifact_name,
            "universe": universe,
        }
        for unit in route_.routes["android"].units
    ]
    python = [
        {
            "id": unit.id,
            "suites": _space(unit.suites),
            "compile": _space(unit.compile_dirs),
        }
        for unit in route_.routes["python"].units
    ]
    return {"go": go, "android": android, "python": python}


def deploy_entries(route_: Route, config: Config) -> list[dict[str, object]]:
    """Deployable units selected by the same routing as CI.

    A shared Go package change reaches every importer, so it also deploys every
    importer that declares a `deploy` block. Units without one (documentation,
    infrastructure, observability) never appear here, so `deploy-home` cannot
    roll them by accident.
    """
    selected = {unit.id for platform_route in route_.routes.values() for unit in platform_route.units}
    entries: list[dict[str, object]] = []
    for unit in config.units:
        if not unit.deploy_images or unit.id not in selected:
            continue
        entries.append(
            {
                "id": unit.id,
                "images": _space(unit.deploy_images),
                "workloads": _space(unit.deploy_workloads),
                "builder": unit.deploy_builder,
                "target_root": unit.deploy_target_root,
                "namespace": unit.deploy_namespace,
                "script": unit.deploy_script,
                "env_key": unit.deploy_env_key,
                "env_file": unit.deploy_env_file,
                "externalsecrets": _space(unit.deploy_externalsecrets),
            }
        )
    return entries


def summary_markdown(route_: Route) -> str:
    lines = [
        f"Base: `{route_.base or '(none - full run)'}` -> head `{route_.head}`",
        f"Changed files: {len(route_.changed)}",
        "",
        "| Platform | Mode | Units |",
        "|---|---|---|",
    ]
    for platform in PLATFORMS:
        platform_route = route_.routes[platform]
        ids = ", ".join(f"`{u.id}`" for u in platform_route.units) or "-"
        lines.append(f"| {platform} | {platform_route.mode} | {ids} |")
    lines.append("")
    lines.append(f"Docs only: `{str(route_.docs_only).lower()}`")
    lines.append(f"Deploy control regression: `{str(route_.deploy_control).lower()}`")
    deployed = sorted(
        {unit.id for pr in route_.routes.values() for unit in pr.units if unit.deploy_images}
    )
    lines.append(f"Deploy units: {', '.join(f'`{u}`' for u in deployed) or '-'}")
    if route_.warnings:
        lines.append("")
        lines.append("Warnings:")
        lines.extend(f"- {warning}" for warning in route_.warnings)
    return "\n".join(lines)


def _write_github_output(path: Path, values: dict[str, str]) -> None:
    with path.open("a", encoding="utf-8") as handle:
        for key, value in values.items():
            if "\n" in value:
                handle.write(f"{key}<<EOF\n{value}\nEOF\n")
            else:
                handle.write(f"{key}={value}\n")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--root", default=".", help="repository root (default: .)")
    parser.add_argument("--units", default=str(DEFAULT_UNITS), help="unit map (default: platform/ci/units.yml)")
    parser.add_argument("--base", default=None, help="base revision (PR base, push before, merge-base)")
    parser.add_argument("--head", default="HEAD", help="head revision (default: HEAD)")
    parser.add_argument("--bazel", default=os.environ.get("BAZEL", "bazel"), help="bazel binary")
    parser.add_argument("--graph-file", default=None, help="use a saved `bazel query --output=xml` dump")
    parser.add_argument("--changed-file", action="append", default=[], help="simulate a changed path (repeatable)")
    parser.add_argument("--format", choices=("text", "json", "github"), default="text")
    parser.add_argument("--out", default=None, help="output file for --format github (default: $GITHUB_OUTPUT)")
    parser.add_argument("--all", action="store_true", help="force every unit (manual dispatch)")
    args = parser.parse_args(argv)

    root = Path(args.root).resolve()
    config = load_config(Path(args.units))
    base = None if args.all else resolve_base(root, args.base)

    if args.changed_file:
        changed = sorted(args.changed_file)
    elif base:
        changed = changed_files(root, base, args.head)
    else:
        changed = []

    if args.graph_file:
        graph_text = Path(args.graph_file).read_text(encoding="utf-8")
        graph = BazelGraph.from_xml(graph_text)
        loader = None
    else:
        graph = None

        def loader() -> BazelGraph:
            return load_bazel_graph(root, args.bazel)

    result = route(
        config,
        changed,
        root=root,
        base=base,
        head=args.head,
        bazel=args.bazel,
        graph=graph,
        graph_loader=loader,
    )

    # No base revision (workflow_dispatch, unknown before-SHA): run everything.
    if (base is None and not args.changed_file) or args.all:
        for platform in PLATFORMS:
            platform_route = result.routes[platform]
            if platform == "quality":
                continue
            platform_route.mode = "all"
            platform_route.units = list(config.by_platform(platform))
        result.deploy_control = True
        result.changed = []
        result.warnings.append("no base revision resolved; running every unit")

    entries = matrix_entries(result, config)

    if args.format == "json":
        print(
            json.dumps(
                {
                    "base": result.base,
                    "head": result.head,
                    "changed_files": result.changed,
                    "docs_only": result.docs_only,
                    "deploy_control": result.deploy_control,
                    "deploy_units": deploy_entries(result, config),
                    "warnings": result.warnings,
                    "modes": {name: route_.mode for name, route_ in result.routes.items()},
                    "units": entries,
                },
                indent=2,
                sort_keys=True,
            )
        )
        return 0

    if args.format == "github":
        out = Path(args.out or os.environ.get("GITHUB_OUTPUT", os.devnull))
        _write_github_output(
            out,
            {
                "base_sha": result.base or "",
                "go_units": json.dumps(entries["go"]),
                "android_units": json.dumps(entries["android"]),
                "python_units": json.dumps(entries["python"]),
                "quality": "true",
                "docs_only": str(result.docs_only).lower(),
                "deploy_control": str(result.deploy_control).lower(),
                "deploy_units": json.dumps(deploy_entries(result, config)),
                "summary": summary_markdown(result),
            },
        )
        print(summary_markdown(result))
        return 0

    print(summary_markdown(result))
    return 0


if __name__ == "__main__":
    sys.exit(main())
