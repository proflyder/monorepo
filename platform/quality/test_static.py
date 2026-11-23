#!/usr/bin/env python3
from __future__ import annotations

import re
import shutil
import subprocess
import unittest
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[2]
KNOWN_BROKEN_LEGACY = {ROOT / "resources" / "spotify-autostart" / "spotify.sh"}


def all_repo_shell() -> list[Path]:
    files = []
    for path in ROOT.rglob("*.sh"):
        rel = path.relative_to(ROOT)
        if any(part in {"backups", "artifacts", "secrets", ".git"} for part in rel.parts):
            continue
        files.append(path)
    return sorted(files)


def first_party_shell() -> list[Path]:
    candidates = []
    for pattern in (
        "projects/home-assistant/**/*.sh",
        "projects/station/ble-gateway/station/*.sh",
        "projects/station/scripts/*.sh",
        "projects/station/configs/*.sh",
        "projects/station/configs/station-applied/*.sh",
    ):
        candidates.extend(ROOT.glob(pattern))
    return sorted(set(candidates))


def first_party_python() -> list[Path]:
    files = []
    for pattern in (
        "projects/station/scripts/*.py",
        "projects/station/tools/**/*.py",
        "projects/station/resources/quasar-connect-bot/*.py",
    ):
        files.extend(ROOT.glob(pattern))
    return sorted(set(files))


class ShellSyntaxTests(unittest.TestCase):
    def test_all_repo_shell_syntax_except_documented_broken_legacy(self):
        files = [p for p in all_repo_shell() if p not in KNOWN_BROKEN_LEGACY]
        self.assertGreaterEqual(len(files), 40)
        for path in files:
            with self.subTest(path=str(path)):
                res = subprocess.run(["bash", "-n", str(path)], text=True, capture_output=True)
                self.assertEqual(res.returncode, 0, f"{path}:\n{res.stderr}")
        self.assertTrue(set(first_party_shell()).issubset(files))
        for path in sorted(KNOWN_BROKEN_LEGACY):
            if not path.exists():
                continue
            self.assertIn("битый", (ROOT / "resources" / "README.md").read_text(), "legacy exception must stay documented")

    def test_active_shell_syntax(self):
        files = first_party_shell()
        self.assertGreaterEqual(len(files), 10)
        for path in files:
            with self.subTest(path=str(path)):
                first = path.read_text(errors="replace").splitlines()[0] if path.read_bytes() else ""
                interpreter = "sh" if "sh" in first and "bash" not in first else "bash"
                res = subprocess.run([interpreter, "-n", str(path)], text=True, capture_output=True)
                self.assertEqual(res.returncode, 0, f"{path}:\n{res.stderr}")

    @unittest.skipUnless(shutil.which("shellcheck"), "shellcheck not installed")
    def test_shellcheck_errors(self):
        files = [str(p) for p in first_party_shell()]
        res = subprocess.run(
            ["shellcheck", "-S", "error", *files], text=True, capture_output=True, timeout=60
        )
        self.assertEqual(res.returncode, 0, res.stdout + res.stderr)


class PythonSyntaxTests(unittest.TestCase):
    def test_python_syntax(self):
        files = first_party_python()
        self.assertGreaterEqual(len(files), 4)
        for path in files:
            with self.subTest(path=str(path)):
                compile(path.read_text(encoding="utf-8"), str(path), "exec")


class SecretHygieneTests(unittest.TestCase):

    def test_no_known_secret_patterns_in_tracked_text(self):
        import re

        pattern = re.compile(
            r"(?:BQ[A-Za-z0-9_-]{30,}|spotify_refresh_token:\s*[A-Za-z0-9_-]{20,}|"
            r"station_control_bearer:\s*[0-9a-fA-F]{32,})"
        )
        roots = [ROOT / "docs", ROOT / "projects", ROOT / "packages", ROOT / "solutions", ROOT / "platform"]
        hits = []
        for base in roots:
            for path in base.rglob("*"):
                if not path.is_file() or path.suffix not in {".md", ".yaml", ".yml", ".sh", ".py", ".rs", ".toml"}:
                    continue
                if "secrets" in path.parts:
                    continue
                for i, line in enumerate(path.read_text(errors="replace").splitlines(), 1):
                    if pattern.search(line):
                        hits.append(f"{path}:{i}")
        self.assertEqual(hits, [])


class KubernetesCatalogCoverageTests(unittest.TestCase):
    KUBERNETES = ROOT / "projects/home/kubernetes"
    CATALOG = KUBERNETES / "mcp-gateway/manifests/catalog.yaml"
    WORKLOAD_KINDS = {"Deployment", "StatefulSet", "DaemonSet"}

    def manifest_workloads(self) -> set[tuple[str, str, str]]:
        workloads: set[tuple[str, str, str]] = set()
        for path in sorted(self.KUBERNETES.rglob("*.yaml")):
            for document in yaml.safe_load_all(path.read_text(encoding="utf-8")):
                if not isinstance(document, dict) or document.get("kind") not in self.WORKLOAD_KINDS:
                    continue
                metadata = document.get("metadata") or {}
                namespace = str(metadata.get("namespace") or "default")
                name = str(metadata.get("name") or "")
                if name:
                    workloads.add((namespace, document["kind"], name))
        return workloads

    def catalog_workloads(self) -> tuple[set[tuple[str, str, str]], set[tuple[str, str, str]]]:
        catalog = yaml.safe_load(self.CATALOG.read_text(encoding="utf-8"))
        services = {
            (service["namespace"], service["workload"]["kind"], service["workload"]["name"])
            for service in catalog.get("services", [])
            if service.get("kind") == "k8s"
        }
        ignored = {
            tuple(part for part in str(entry).split("/") if part)
            for entry in catalog.get("ignore_workloads", [])
        }
        return services, ignored

    def test_every_manifest_workload_is_in_the_report_catalog(self):
        services, ignored = self.catalog_workloads()
        missing = [
            f"{namespace}/{kind}/{name}"
            for namespace, kind, name in sorted(self.manifest_workloads() - services - ignored)
        ]
        self.assertEqual(
            missing,
            [],
            "workloads missing from the report catalog and ignore_workloads "
            "(projects/home/kubernetes/mcp-gateway/manifests/catalog.yaml):\n"
            + "\n".join(missing),
        )


class AndroidManifestPermissionTests(unittest.TestCase):

    PATTERN = __import__("re").compile(r'<uses-permission\s+android:name="([^"]+)"')

    def test_library_permissions_are_declared_by_the_app(self) -> None:
        android = ROOT / "projects/mobile/android"
        app = android / "app/src/main/AndroidManifest.xml"
        declared = set(self.PATTERN.findall(app.read_text(encoding="utf-8")))
        for manifest in sorted(android.glob("**/src/main/AndroidManifest.xml")):
            if manifest == app:
                continue
            for permission in self.PATTERN.findall(manifest.read_text(encoding="utf-8")):
                with self.subTest(manifest=str(manifest.relative_to(ROOT)), permission=permission):
                    self.assertIn(permission, declared)


class AndroidThemeColorTests(unittest.TestCase):
    ANDROID = ROOT / "projects/mobile/android"
    COLOR = __import__("re").compile(r'"(#[0-9A-Fa-f]{3,8}|@color/[a-z0-9_]+|@android:color/(?!transparent)[a-z_]+)"')
    ALLOWED_REFS = {"@color/ds_pill_text", "@color/alarms_day_toggle_text", "@color/alarms_day_toggle_background",
                    "@color/alarms_day_toggle_stroke", "@color/alarms_delete_ripple",
                    "@color/control_scrim", "@color/control_speed_dial_label"}

    def _problems(self, path: Path) -> list[str]:
        text = path.read_text(encoding="utf-8")
        is_vector = "<vector" in text
        problems = []
        for number, line in enumerate(text.splitlines(), 1):
            for value in self.COLOR.findall(line):
                if value in self.ALLOWED_REFS:
                    continue
                if is_vector and ("fillColor" in line or "strokeColor" in line):
                    continue
                if "@android:id/mask" in text and value == "@android:color/white":
                    continue
                if path.name == "control_bg_swatch.xml":
                    continue
                if path.name.startswith("app_ic_launcher_"):
                    continue
                problems.append(f"{path.relative_to(ROOT)}:{number}: {value}")
        return problems

    def test_layouts_drawables_and_styles_use_theme_attributes(self) -> None:
        files = []
        for pattern in ("**/src/main/res/layout*/*.xml", "**/src/main/res/drawable*/*.xml",
                        "**/src/main/res/color*/*.xml", "**/src/main/res/values*/styles.xml"):
            files.extend(self.ANDROID.glob(pattern))
        self.assertGreater(len(files), 50)
        problems = [p for f in sorted(set(files)) for p in self._problems(f)]
        self.assertEqual(problems, [], "hard-coded colours outside the theme palette:\n" + "\n".join(problems))

    def test_no_segmented_button_groups(self) -> None:
        banned = ("MaterialButtonToggleGroup", "SegmentedButton")
        problems = []
        for path in sorted(self.ANDROID.glob("**/src/main/**/*")):
            if path.suffix not in {".xml", ".kt"}:
                continue
            text = path.read_text(encoding="utf-8")
            problems.extend(f"{path.relative_to(ROOT)}: {name}" for name in banned if name in text)
        self.assertEqual(problems, [], "use core/designsystem PillSegments instead:\n" + "\n".join(problems))

    def test_kotlin_does_not_parse_or_build_colours(self) -> None:
        pattern = __import__("re").compile(r"Color\.(parseColor|rgb|argb)\(|0xFF[0-9A-Fa-f]{6}")
        problems = []
        for path in sorted(self.ANDROID.glob("**/src/main/kotlin/**/*.kt")):
            for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
                if pattern.search(line):
                    problems.append(f"{path.relative_to(ROOT)}:{number}")
        self.assertEqual(problems, [], "\n".join(problems))


class AndroidDesignSystemTests(unittest.TestCase):
    ANDROID = ROOT / "projects/mobile/android"
    DESIGN_SYSTEM = ANDROID / "core/designsystem"
    RE = __import__("re")
    TAG = RE.compile(r"<([A-Za-z0-9_.]+)\b([^>]*)>", RE.S)
    STYLE = RE.compile(r'\bstyle="@style/([^"]+)"')
    COMPONENTS = {
        "Button": ("Widget.Home.Button.", "Widget.Home.IconButton."),
        "com.google.android.material.button.MaterialButton": ("Widget.Home.Button.", "Widget.Home.IconButton."),
        "com.google.android.material.textfield.TextInputLayout": (None, "Widget.Home.TextInput"),
        "com.google.android.material.floatingactionbutton.FloatingActionButton": (None, "Widget.Home.FloatingActionButton"),
        "com.google.android.material.floatingactionbutton.ExtendedFloatingActionButton": (None, "Widget.Home.ExtendedFloatingActionButton"),
        "dev.proflyder.home.core.designsystem.OutlinedFloatingActionButton": (None, "Widget.Home.FloatingActionButton"),
        "com.google.android.material.card.MaterialCardView": ("Widget.Home.Card",),
        "com.google.android.material.chip.Chip": ("Widget.Home.Chip.",),
        "com.google.android.material.slider.Slider": (None, "Widget.Home.Slider"),
    }
    KOTLIN = RE.compile(r"\b(MaterialButton|TextInputLayout|FloatingActionButton|ExtendedFloatingActionButton|MaterialCardView|Chip)\(")

    def _outside(self, pattern: str) -> list[Path]:
        return [p for p in sorted(self.ANDROID.glob(pattern)) if self.DESIGN_SYSTEM not in p.parents]

    def _tag_problem(self, tag: str, attrs: str) -> str | None:
        allowed = self.COMPONENTS.get(tag)
        if allowed is None:
            return None
        style = self.STYLE.search(attrs)
        if style is None:
            return None if None in allowed else "no design-system style"
        prefixes = tuple(a for a in allowed if a)
        return None if style.group(1).startswith(prefixes) else "style " + style.group(1)

    def test_widget_styles_are_declared_only_in_the_design_system(self) -> None:
        problems = []
        theme = self.ANDROID / "core/theme"
        for path in self._outside("**/src/main/res/values*/*.xml"):
            if theme in path.parents:
                continue
            for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
                if '<style name="Widget.' in line:
                    problems.append(f"{path.relative_to(ROOT)}:{number}")
        self.assertEqual(problems, [], "declare Widget.* styles in core/designsystem:\n" + "\n".join(problems))

    def test_layouts_use_design_system_components(self) -> None:
        problems = []
        for path in self._outside("**/src/main/res/layout*/*.xml"):
            for match in self.TAG.finditer(path.read_text(encoding="utf-8")):
                problem = self._tag_problem(match.group(1), match.group(2))
                if problem:
                    problems.append(f"{path.relative_to(ROOT)}: <{match.group(1).rsplit('.', 1)[-1]}> {problem}")
        self.assertEqual(problems, [], "use core/designsystem styles:\n" + "\n".join(problems))

    def test_one_filled_button_per_button_group(self) -> None:
        group = self.RE.compile(r"<([\w.]*\.designsystem\.ButtonRow)\b.*?</\1>", self.RE.S)
        problems = []
        for path in sorted(self.ANDROID.glob("**/src/main/res/layout*/*.xml")):
            for match in group.finditer(path.read_text(encoding="utf-8")):
                if match.group(0).count("Widget.Home.Button.Filled.") > 1:
                    problems.append(str(path.relative_to(ROOT)))
        self.assertEqual(problems, [], "one filled (main) action per ButtonRow/EmptyState:\n" + "\n".join(problems))

    def test_kotlin_does_not_build_components_by_hand(self) -> None:
        problems = []
        for path in self._outside("**/src/main/kotlin/**/*.kt"):
            for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
                if self.KOTLIN.search(line):
                    problems.append(f"{path.relative_to(ROOT)}:{number}")
        self.assertEqual(problems, [], "inflate design-system components from layouts:\n" + "\n".join(problems))




class KubernetesConfigMapSecretTests(unittest.TestCase):
    KUBERNETES = ROOT / "projects/home/kubernetes"

    def test_config_maps_do_not_carry_token_fields_or_values(self):
        import re

        key_pattern = re.compile(r"(token|secret|password|credential|private_key)", re.I)
        problems = []
        for path in self.KUBERNETES.rglob("*.yaml"):
            try:
                docs = [d for d in yaml.safe_load_all(path.read_text()) if d]
            except yaml.YAMLError as error:
                problems.append(f"{path}: YAML: {error}")
                continue
            for doc in docs:
                if doc.get("kind") != "ConfigMap":
                    continue
                for key, value in (doc.get("data") or {}).items():
                    if key_pattern.search(str(key)):
                        problems.append(f"{path}: data.{key}")
                    if isinstance(value, str) and key_pattern.search(value):
                        problems.append(f"{path}: data.{key} value")
        self.assertEqual(problems, [])



ENV_CONTRACT = {
    "projects/home/kubernetes/scheduler/manifests/scheduler.yaml": (
        "solutions/scheduler/crates/scheduler/src",
        {
            "SCHEDULER_TOKEN_FILE",
            "SCHEDULER_SERVICE_TOKEN_FILE",
            "SCHEDULER_SERVICE_TOKEN_PREVIOUS_FILE",
            "SCHEDULER_APP_TOKENS_FILE",
        },
    ),
    "projects/home/kubernetes/scheduler/manifests/bridge.yaml": (
        "solutions/ha-bridge/crates/ha-bridge/src",
        {"SCHEDULER_SERVICE_TOKEN_FILE"},
    ),
    "projects/home/kubernetes/scheduler/manifests/control.yaml": (
        "solutions/ha-bridge/crates/ha-bridge/src",
        {"SCHEDULER_SERVICE_TOKEN_FILE"},
    ),
    "projects/home/kubernetes/scheduler/manifests/scenario.yaml": (
        "solutions/ha-bridge/crates/ha-bridge/src",
        {"SCHEDULER_SERVICE_TOKEN_FILE"},
    ),
    "projects/home/kubernetes/intercom/manifests/intercom.yaml": (
        "solutions/intercom/crates/intercom/src",
        {"INTERCOM_SERVICE_TOKEN_FILE", "INTERCOM_ACCESS_URL"},
    ),
    "projects/home/kubernetes/mcp-gateway/manifests/deployment.yaml": (
        "solutions/mcp-gateway/crates/mcp-gateway/src",
        {"MCP_SCHEDULER_SERVICE_TOKEN_FILE", "MCP_APP_API"},
    ),
    "projects/home/kubernetes/phone-push/manifests/phone-push.yaml": (
        "solutions/phone-push/crates/phone-push/src",
        {"PUSH_SCHEDULER_SERVICE_TOKEN_FILE"},
    ),
}
SHARED_ENV_SOURCES = ("packages/rust",)
LEGACY_TOKEN_ENV = {
    "SCHEDULER_TOKEN_FILE": {"scheduler.yaml"},
    "MCP_APP_TOKEN_FILE": set(),
    "MCP_SCHEDULER_TOKEN_FILE": set(),
    "INTERCOM_TOKEN_FILE": set(),
    "PUSH_SCHEDULER_TOKEN_FILE": set(),
}


def env_names_read_by(directory: str) -> set[str]:
    names = set()
    for base in (directory, *SHARED_ENV_SOURCES):
        for path in (ROOT / base).rglob("*.rs"):
            text = path.read_text(encoding="utf-8")
            text = text.split("#[cfg(test)]", 1)[0]
            names.update(re.findall(r'"([A-Z][A-Z0-9_]{2,})"', text))
    return names | {f"{name}_FILE" for name in names}


def manifest_env(path: str) -> set[str]:
    names = set()
    for document in yaml.safe_load_all((ROOT / path).read_text(encoding="utf-8")):
        if not isinstance(document, dict) or document.get("kind") != "Deployment":
            continue
        for container in document["spec"]["template"]["spec"].get("containers", []):
            names.update(item["name"] for item in container.get("env") or [])
    return names


class ServiceEnvContractTests(unittest.TestCase):
    def test_every_manifest_variable_is_read_by_the_service(self):
        for manifest, (source, _) in ENV_CONTRACT.items():
            unknown = manifest_env(manifest) - env_names_read_by(source)
            self.assertEqual(unknown, set(), f"{manifest} sets variables nobody reads")

    def test_token_variables_the_service_requires_are_set(self):
        for manifest, (source, required) in ENV_CONTRACT.items():
            self.assertLessEqual(required, env_names_read_by(source), source)
            missing = required - manifest_env(manifest)
            self.assertEqual(missing, set(), f"{manifest} misses {sorted(missing)}")

    def test_no_manifest_passes_a_removed_legacy_token_variable(self):
        for manifest in ENV_CONTRACT:
            env = manifest_env(manifest)
            for name, allowed in LEGACY_TOKEN_ENV.items():
                if Path(manifest).name in allowed:
                    continue
                self.assertNotIn(name, env, manifest)


if __name__ == "__main__":
    unittest.main(verbosity=2)
