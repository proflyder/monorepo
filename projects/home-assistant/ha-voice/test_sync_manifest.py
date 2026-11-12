"""Contract for the ha-voice file set delivered by ha-config-sync.

The manifest is deployment data: every YAML file in this directory must be
covered by exactly one glob, and nothing else (configuration.yaml, secrets.yaml,
scenes.yaml, tests, README, the manifest itself) may ever be delivered.
"""
from __future__ import annotations

import fnmatch
import re
import unittest
from pathlib import Path

import yaml

HERE = Path(__file__).resolve().parent
MANIFEST = HERE / "sync.yaml"

# The exact deliverable set. A new file added to the directory must be listed
# here and in sync.yaml in the same change.
DELIVERED = {
    "automations.yaml",
    "command_line.yaml",
    "input_boolean.yaml",
    "input_datetime.yaml",
    "input_number.yaml",
    "input_select.yaml",
    "input_text.yaml",
    "intent_scripts.yaml",
    "rest_commands.yaml",
    "schedules_dashboard.yaml",
    "scripts.yaml",
    "shell_commands.yaml",
    "templates.yaml",
    "custom_sentences/ru/station_alarm.yaml",
    "custom_sentences/ru/station_control.yaml",
    "custom_sentences/ru/station_currency.yaml",
    "custom_sentences/ru/station_komp.yaml",
    "custom_sentences/ru/station_music.yaml",
    "custom_sentences/ru/station_schedules.yaml",
    "custom_sentences/ru/station_time.yaml",
}
DENIED = {"configuration.yaml", "secrets.yaml", "scenes.yaml"}
SERVICE = re.compile(r"^[a-z0-9_]+\.[a-z0-9_]+$")


def segment_matches(pattern: str, path: str) -> bool:
    """`*` and `?` never cross `/`, like the Rust and shell validators."""
    parts = pattern.split("/")
    values = path.split("/")
    return len(parts) == len(values) and all(
        fnmatch.fnmatchcase(value, part) for part, value in zip(parts, values)
    )


def manifest_entries() -> list[dict]:
    document = yaml.safe_load(MANIFEST.read_text(encoding="utf-8"))
    return document["files"]


class ManifestTests(unittest.TestCase):
    def setUp(self) -> None:
        self.entries = manifest_entries()

    def test_schema_is_exact(self):
        self.assertTrue(self.entries, "the manifest must not be empty")
        for entry in self.entries:
            self.assertLessEqual(set(entry), {"glob", "reload", "restart"}, entry)
            self.assertIsInstance(entry["glob"], str)
            self.assertTrue(entry["glob"].strip())
            for service in entry.get("reload", []):
                self.assertRegex(service, SERVICE)
                self.assertTrue(service.split(".")[0])
            self.assertIsInstance(entry.get("restart", False), bool)

    def test_every_delivered_file_is_covered_exactly_once(self):
        for path in DELIVERED:
            matches = [
                entry for entry in self.entries if segment_matches(entry["glob"], path)
            ]
            self.assertEqual(len(matches), 1, f"{path} must be covered by exactly one glob")

    def test_no_glob_matches_something_outside_the_delivered_set(self):
        actual = {
            path.relative_to(HERE).as_posix()
            for path in HERE.rglob("*.yaml")
            if path != MANIFEST
        }
        self.assertEqual(actual, DELIVERED)
        for entry in self.entries:
            matched = {path for path in actual if segment_matches(entry["glob"], path)}
            self.assertTrue(matched, f"glob {entry['glob']} matches nothing")
            self.assertTrue(matched <= DELIVERED)

    def test_denied_files_are_never_present_or_delivered(self):
        for name in DENIED:
            self.assertNotIn(name, DELIVERED)
            self.assertFalse((HERE / name).exists(), f"{name} must not exist here")
            for entry in self.entries:
                self.assertNotEqual(entry["glob"], name)

    def test_shell_commands_restart_and_dashboards_do_nothing(self):
        by_glob = {entry["glob"]: entry for entry in self.entries}
        self.assertTrue(by_glob["shell_commands.yaml"]["restart"])
        self.assertNotIn("reload", by_glob["shell_commands.yaml"])
        self.assertNotIn("reload", by_glob["schedules_dashboard.yaml"])
        self.assertFalse(by_glob["schedules_dashboard.yaml"].get("restart", False))
        self.assertEqual(by_glob["automations.yaml"]["reload"], ["automation.reload"])
        self.assertEqual(by_glob["custom_sentences/ru/*.yaml"]["reload"], ["conversation.reload"])


if __name__ == "__main__":
    unittest.main()
