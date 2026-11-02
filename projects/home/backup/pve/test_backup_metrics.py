"""Hermetic tests for the PVE vzdump metrics exporter."""
from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("backup_metrics", HERE / "backup_metrics.py")
metrics = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = metrics
SPEC.loader.exec_module(metrics)


def completed(command: list[str], returncode: int = 0, stdout: str = "") -> subprocess.CompletedProcess:
    return subprocess.CompletedProcess(command, returncode, stdout, "")


SAMPLE = json.dumps(
    [
        {"vmid": 100, "ctime": 1700000000, "size": 1000, "volid": "backup:backup/vzdump-qemu-100-old.vma.zst"},
        {"vmid": 100, "ctime": 1700100000, "size": 2000, "volid": "backup:backup/vzdump-qemu-100-new.vma.zst"},
        {"vmid": 101, "ctime": 1700050000, "size": 3000, "volid": "backup:backup/vzdump-qemu-101.vma.zst"},
        {"vmid": 102, "ctime": 1700060000, "size": 4000, "volid": "backup:backup/vzdump-lxc-102.tar.zst"},
    ]
)


class VzdumpMetricsTests(unittest.TestCase):
    def test_parse_array_object_and_json_lines(self) -> None:
        self.assertEqual(len(metrics.parse_pvesm_output(SAMPLE)), 4)
        wrapped = json.dumps({"data": json.loads(SAMPLE)})
        self.assertEqual(len(metrics.parse_pvesm_output(wrapped)), 4)
        lines = "\n".join(json.dumps(item) for item in json.loads(SAMPLE))
        self.assertEqual(len(metrics.parse_pvesm_output(lines)), 4)
        self.assertEqual(metrics.parse_pvesm_output("not json"), [])

    def test_newest_archive_per_guest_wins(self) -> None:
        guests = metrics.newest_per_guest(metrics.parse_pvesm_output(SAMPLE))
        self.assertEqual(sorted(guests), [100, 101, 102])
        self.assertEqual(guests[100].ctime, 1700100000)
        self.assertEqual(guests[100].size, 2000)
        self.assertIn("100-new", guests[100].volid)

    def test_guest_name_from_qm_and_pct_config(self) -> None:
        self.assertEqual(metrics.parse_guest_name("cores: 2\nname: haos\n"), "haos")
        self.assertEqual(metrics.parse_guest_name("hostname: vpn\n"), "vpn")
        self.assertEqual(metrics.parse_guest_name("cores: 2\n"), "")

    def test_guest_name_falls_back_to_notes_file(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            archive = Path(tmp) / "vzdump-qemu-100.vma.zst"
            archive.write_bytes(b"x")
            archive.with_name(archive.name + ".notes").write_text("haos guest\n", encoding="utf-8")

            def runner(command: list[str], timeout: int = 60) -> subprocess.CompletedProcess:
                if command[:2] == ["qm", "config"]:
                    return completed(command, 1)
                if command[:2] == ["pct", "config"]:
                    return completed(command, 1)
                if command[:2] == ["pvesm", "path"]:
                    return completed(command, 0, str(archive) + "\n")
                raise AssertionError(command)

            with patch.object(metrics, "run_command", side_effect=runner):
                self.assertEqual(metrics.guest_name(100, "backup:backup/x"), "haos guest")

    def test_render_escapes_label_values(self) -> None:
        guest = metrics.GuestBackup(100, 1700000000, 10, "v", 'we"ird\\name')
        text = metrics.render_metrics([guest])
        self.assertIn('guest="we\\"ird\\\\name"', text)
        self.assertIn('home_vzdump_last_backup_timestamp_seconds{vmid="100"', text)

    def test_main_writes_metrics_atomically(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / "node-exporter" / "vzdump.prom"

            def runner(command: list[str], timeout: int = 60) -> subprocess.CompletedProcess:
                if command[:2] == ["pvesh", "get"] and command[2].endswith("/storage/backup/content"):
                    return completed(command, 0, SAMPLE)
                if command[:2] == ["qm", "config"]:
                    names = {"100": "haos", "101": "lab-k3s"}
                    return completed(command, 0, f"name: {names.get(command[2], '')}\n")
                if command[:2] == ["pct", "config"]:
                    return completed(command, 0, "hostname: vpn\n")
                raise AssertionError(command)

            with patch.object(metrics, "run_command", side_effect=runner):
                code = metrics.main(["--storage", "backup", "--output", str(output)])
            self.assertEqual(code, 0)
            text = output.read_text(encoding="utf-8")
            self.assertIn('vmid="100",guest="haos"', text)
            self.assertIn('vmid="101",guest="lab-k3s"', text)
            self.assertIn('vmid="102",guest="vpn"', text)
            self.assertIn("home_vzdump_last_backup_bytes", text)
            self.assertEqual([item.name for item in output.parent.iterdir()], ["vzdump.prom"])

    def test_main_fails_when_pvesm_fails(self) -> None:
        with patch.object(metrics, "run_command", return_value=completed([], 1)):
            self.assertEqual(metrics.main(["--output", "/nonexistent/ignored.prom"]), 1)


if __name__ == "__main__":
    unittest.main()
