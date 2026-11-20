#!/usr/bin/env python3
from __future__ import annotations

import shutil
import subprocess
import unittest
from pathlib import Path

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
                    "@color/control_scrim", "@color/control_speed_dial_label",
                    "@color/home_launcher_bg", "@color/home_launcher_fg"}

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


if __name__ == "__main__":
    unittest.main(verbosity=2)
