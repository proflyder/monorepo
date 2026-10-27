#!/usr/bin/env python3
"""Cheap syntax/lint gates for first-party shell and Python."""
from __future__ import annotations

import shutil
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
# Documented in docs/RESOURCES.md: incomplete legacy copy, not in any active path.
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
        self.assertGreaterEqual(len(files), 60)
        for path in files:
            with self.subTest(path=str(path)):
                res = subprocess.run(["bash", "-n", str(path)], text=True, capture_output=True)
                self.assertEqual(res.returncode, 0, f"{path}:\n{res.stderr}")
        broken = sorted(KNOWN_BROKEN_LEGACY)
        for path in broken:
            self.assertTrue(path.exists())
            self.assertIn("битый", (ROOT / "docs" / "RESOURCES.md").read_text(), "legacy exception must stay documented")

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
    """Docs and code must not contain obvious live credentials."""

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
                if not path.is_file() or path.suffix not in {".md", ".yaml", ".yml", ".sh", ".py", ".go"}:
                    continue
                if "secrets" in path.parts:
                    continue
                for i, line in enumerate(path.read_text(errors="replace").splitlines(), 1):
                    if pattern.search(line):
                        hits.append(f"{path}:{i}")
        self.assertEqual(hits, [])


if __name__ == "__main__":
    unittest.main(verbosity=2)
