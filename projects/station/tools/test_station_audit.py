#!/usr/bin/env python3
"""Hermetic tests for projects/station/tools/station-audit.

No SSH and no station: the remote probe is replaced by a parsed `Remote`
object, and repository files live in a temporary directory.
"""

from __future__ import annotations

import hashlib
import importlib.machinery
import importlib.util
import json
import pathlib
import sys
import tempfile
import unittest

TOOLS = pathlib.Path(__file__).resolve().parent


def load_audit():
    loader = importlib.machinery.SourceFileLoader("station_audit", str(TOOLS / "station-audit"))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    module = importlib.util.module_from_spec(spec)
    sys.modules[loader.name] = module
    loader.exec_module(module)
    return module


audit = load_audit()


def sha(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


class ManifestTests(unittest.TestCase):
    def test_repository_manifest_is_valid(self):
        manifest = audit.load_manifest(audit.DEFAULT_MANIFEST)
        self.assertEqual(manifest["version"], 1)
        ids = [component["id"] for component in manifest["components"]]
        self.assertEqual(len(ids), len(set(ids)))
        self.assertIn("pulsar", ids)
        self.assertIn("ble-gateway", ids)
        self.assertIn("dropbear", ids)

    def test_recovery_paths_are_marked(self):
        manifest = audit.load_manifest(audit.DEFAULT_MANIFEST)
        recovery = {
            component["id"]
            for component in manifest["components"]
            if component["recovery"]
        }
        for required in ("ble-gateway", "dropbear", "adbd", "rootkeeper"):
            self.assertIn(required, recovery)

    def test_ota_covers_only_pulsar(self):
        manifest = audit.load_manifest(audit.DEFAULT_MANIFEST)
        ota = [
            component["id"]
            for component in manifest["components"]
            if component["update"] == "ota"
        ]
        self.assertEqual(ota, ["pulsar"])

    def test_update_modes_are_known(self):
        manifest = audit.load_manifest(audit.DEFAULT_MANIFEST)
        for component in manifest["components"]:
            self.assertIn(component["update"], audit.UPDATE_MODES)

    def test_validate_rejects_unknown_compare(self):
        with self.assertRaises(audit.ManifestError):
            audit.validate_manifest(
                {
                    "version": 1,
                    "audit": {},
                    "components": [
                        {
                            "id": "x",
                            "title": "x",
                            "purpose": "x",
                            "startup": "x",
                            "owner": "x",
                            "update": "ota",
                            "recovery": False,
                            "source": {"path": "."},
                            "files": [{"device": "/x", "compare": "magic"}],
                        }
                    ],
                }
            )

    def test_validate_rejects_duplicate_ids(self):
        component = {
            "id": "x",
            "title": "x",
            "purpose": "x",
            "startup": "x",
            "owner": "x",
            "update": "ota",
            "recovery": False,
            "source": {"path": "."},
            "files": [{"device": "/x", "compare": "presence"}],
        }
        with self.assertRaises(audit.ManifestError):
            audit.validate_manifest(
                {"version": 1, "audit": {}, "components": [component, component]}
            )


class CompareTests(unittest.TestCase):
    def test_normalize_ipv4_masks_addresses(self):
        text = "forward-addr: \"192.0.2.11\"\nblock 192.168.3.0/24\n"
        self.assertEqual(
            audit.normalize_ipv4(text),
            "forward-addr: \"<ip>\"\nblock <ip>/24\n",
        )

    def test_normalized_hash_ignores_address_and_keeps_content(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            (root / "repo.conf").write_text("forward-addr: 192.0.2.11\n", encoding="utf-8")
            same = sha(audit.normalize_ipv4("forward-addr: 10.0.0.1\n"))
            different = sha(audit.normalize_ipv4("forward-addr: 10.0.0.1\nother: 1\n"))
            self.assertEqual(audit.repo_hash(root, "repo.conf", "ipv4"), same)
            self.assertNotEqual(audit.repo_hash(root, "repo.conf", "ipv4"), different)

    def test_expected_version_from_plain_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            (root / "VERSION").write_text("v0.5.1015\n", encoding="utf-8")
            spec = {"expected": {"repo": "VERSION", "prefix": "pulsar "}}
            self.assertEqual(audit.expected_version(spec, root), "pulsar v0.5.1015")

    def test_expected_version_from_regex(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            (root / "build.sh").write_text('DROPBEAR_VER=2025.88\n', encoding="utf-8")
            spec = {
                "expected": {
                    "repo": "build.sh",
                    "regex": r"^DROPBEAR_VER=([0-9.]+)$",
                    "prefix": "Dropbear v",
                }
            }
            self.assertEqual(audit.expected_version(spec, root), "Dropbear v2025.88")


class ProbeTests(unittest.TestCase):
    def manifest(self):
        return {
            "version": 1,
            "audit": {
                "ssh_user": "root",
                "ssh_key": "~/.ssh/home-lab/station_ed25519",
                "extra_dirs": ["/d"],
                "extra_patterns": ["*.bak"],
            },
            "components": [
                {
                    "id": "x",
                    "title": "x",
                    "purpose": "x",
                    "startup": "x",
                    "owner": "x",
                    "update": "manual-rootfs",
                    "recovery": True,
                    "device_root": "/d",
                    "source": {"path": "."},
                    "files": [
                        {"device": "/d/a", "repo": "a.txt", "compare": "sha256"},
                        {
                            "device": "/d/b",
                            "repo": "b.conf",
                            "compare": "sha256",
                            "normalize": "ipv4",
                        },
                        {
                            "device": "/d/v",
                            "compare": "version",
                            "version_command": "/d/v --version",
                            "expected": {"repo": "VERSION", "prefix": "v"},
                        },
                        {"device": "/d/p", "compare": "presence"},
                    ],
                }
            ],
        }

    def test_probe_is_read_only_and_quotes_paths(self):
        script = audit.build_remote_script(self.manifest())
        self.assertIn("sha256sum", script)
        self.assertIn("/d/v --version", script)
        self.assertIn("ls -1A /d", script)
        for forbidden in ("rm ", "mv ", "touch ", "mkdir ", "mount ", "chmod "):
            self.assertNotIn(forbidden, script)
        self.assertIn("'", script)

    def test_parse_remote(self):
        remote = audit.parse_remote(
            "F\t/d/a\tdeadbeef\nD\t/d/big\nM\t/d/missing\nV\t0\tv1\n"
            "L\t/d\nE\ta\nE\told.bak\n"
        )
        self.assertEqual(remote.files["/d/a"], ("file", "deadbeef"))
        self.assertEqual(remote.files["/d/big"], ("dir", None))
        self.assertEqual(remote.files["/d/missing"], ("missing", None))
        self.assertEqual(remote.versions[0], "v1")
        self.assertEqual(remote.listings["/d"], ["a", "old.bak"])

    def test_parse_remote_rejects_missing_sha256sum(self):
        with self.assertRaises(audit.ManifestError):
            audit.parse_remote("X\tno-sha256sum\n")

    def test_evaluate_all_statuses(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            (root / "a.txt").write_text("alpha", encoding="utf-8")
            (root / "b.conf").write_text("forward-addr: 192.0.2.11\n", encoding="utf-8")
            (root / "VERSION").write_text("1\n", encoding="utf-8")
            remote = audit.Remote(
                files={
                    "/d/a": ("file", sha("alpha")),
                    "/d/b": ("file", sha(audit.normalize_ipv4("forward-addr: 10.0.0.1\n"))),
                    "/d/v": ("file", "hash"),
                    "/d/p": ("file", "hash"),
                },
                versions={0: "v1"},
                listings={"/d": ["a", "old.bak", "notes.txt"]},
            )
            rows = audit.evaluate(self.manifest(), remote, repo_root=root)
            by_path = {row.path: row.status for row in rows}
            self.assertEqual(by_path["/d/a"], "OK")
            self.assertEqual(by_path["/d/b"], "OK")
            self.assertEqual(by_path["/d/v"], "OK")
            self.assertEqual(by_path["/d/p"], "OK")
            self.assertEqual(by_path["/d/old.bak"], "EXTRA")
            self.assertNotIn("/d/notes.txt", by_path)

    def test_evaluate_reports_drift_and_missing(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            (root / "a.txt").write_text("alpha", encoding="utf-8")
            (root / "b.conf").write_text("x\n", encoding="utf-8")
            (root / "VERSION").write_text("1\n", encoding="utf-8")
            remote = audit.Remote(
                files={
                    "/d/a": ("file", sha("changed")),
                    "/d/b": ("dir", None),
                    "/d/v": ("file", "hash"),
                    "/d/p": ("file", "hash"),
                },
                versions={0: "v2"},
                listings={},
            )
            rows = audit.evaluate(self.manifest(), remote, repo_root=root)
            by_path = {row.path: row.status for row in rows}
            self.assertEqual(by_path["/d/a"], "DRIFT")
            self.assertEqual(by_path["/d/b"], "DRIFT")
            self.assertEqual(by_path["/d/v"], "DRIFT")
            self.assertEqual(
                [row.status for row in rows if row.path == "/d/p"], ["OK"]
            )

    def test_missing_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            (root / "a.txt").write_text("alpha", encoding="utf-8")
            (root / "b.conf").write_text("x\n", encoding="utf-8")
            (root / "VERSION").write_text("1\n", encoding="utf-8")
            remote = audit.Remote(files={}, versions={}, listings={})
            rows = audit.evaluate(self.manifest(), remote, repo_root=root)
            self.assertTrue(all(row.status == "MISSING" for row in rows))


class ReportTests(unittest.TestCase):
    def test_format_report_has_header_and_summary(self):
        rows = [
            audit.Row("OK", "pulsar", "/data/pulsar/pulsar", "версия v1"),
            audit.Row("DRIFT", "ble-gateway", "/data/quasar/x", "sha256 отличается"),
            audit.Row("EXTRA", "rootkeeper", "/data/quasar/a.bak", "лишний файл"),
        ]
        text = audit.format_report(rows, audit.summarize(rows), components=2)
        self.assertIn("STATUS", text)
        self.assertIn("OK", text)
        self.assertIn("DRIFT", text)
        self.assertIn("EXTRA", text)
        self.assertIn("Итого: 1 OK, 1 DRIFT, 0 MISSING, 1 EXTRA", text)

    def test_json_report(self):
        rows = [audit.Row("OK", "pulsar", "/data/pulsar/pulsar", "версия v1")]
        payload = json.loads(audit.to_json(rows, audit.summarize(rows), components=1))
        self.assertEqual(payload["summary"]["OK"], 1)
        self.assertEqual(payload["rows"][0]["component"], "pulsar")

    def test_fail_on_thresholds(self):
        self.assertEqual(audit.failing_statuses("none"), set())
        self.assertEqual(audit.failing_statuses("missing"), {"MISSING"})
        self.assertEqual(audit.failing_statuses("drift"), {"DRIFT", "MISSING"})
        self.assertEqual(
            audit.failing_statuses("any"), {"DRIFT", "MISSING", "EXTRA"}
        )


class AddressTests(unittest.TestCase):
    def manifest(self):
        return {
            "audit": {
                "address": {
                    "env": "STATION_IP",
                    "file": "NETWORK.md",
                    "line_hint": "Станция",
                }
            }
        }

    def test_explicit_ip_wins(self):
        self.assertEqual(
            audit.resolve_address("192.0.2.50", self.manifest(), pathlib.Path("."), {}),
            "192.0.2.50",
        )

    def test_environment_wins_over_file(self):
        self.assertEqual(
            audit.resolve_address(
                None, self.manifest(), pathlib.Path("."), {"STATION_IP": "192.0.2.51"}
            ),
            "192.0.2.51",
        )

    def test_file_hint_line(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            (root / "NETWORK.md").write_text(
                "| Proxmox | 192.0.2.26 |\n| Станция Midi | 192.0.2.13 |\n",
                encoding="utf-8",
            )
            self.assertEqual(
                audit.resolve_address(None, self.manifest(), root, {}), "192.0.2.13"
            )

    def test_missing_address_raises(self):
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(audit.ManifestError):
                audit.resolve_address(None, self.manifest(), pathlib.Path(tmp), {})


if __name__ == "__main__":
    unittest.main(verbosity=2)
