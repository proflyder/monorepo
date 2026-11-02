"""Hermetic checks for the bb Proxmox scripts (no Proxmox, no network)."""
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).with_name("bb")
CREATE = ROOT / "create-lxc.sh"
BACKUP = ROOT / "bb-db-backup.sh"


class CreateLxcTest(unittest.TestCase):
    def run_create(self, *args):
        return subprocess.run(
            ["bash", str(CREATE), *args],
            check=False,
            capture_output=True,
            text=True,
        )

    def test_dry_run_uses_bb_defaults_without_tun(self):
        result = self.run_create(
            "--dry-run",
            "--ip", "192.0.2.30",
            "--gateway", "192.0.2.1",
            "--dns", "192.0.2.1",
            "--mac", "02:00:00:00:00:30",
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        plan = result.stdout.replace("\\,", ",")
        self.assertIn("pct create 103", plan)
        self.assertIn("--hostname bb", plan)
        self.assertIn("--cores 2", plan)
        self.assertIn("--memory 2048", plan)
        self.assertIn("--swap 1024", plan)
        self.assertIn("local-lvm:16", plan)
        self.assertIn("hwaddr=02:00:00:00:00:30", plan)
        self.assertIn("--timezone Asia/Almaty", plan)
        self.assertIn("--startup order=3,up=30,down=60", plan)
        self.assertIn("--features nesting=1", plan)
        self.assertNotIn("/dev/net/tun", plan)

    def test_requires_static_network_and_mac(self):
        result = self.run_create("--dry-run")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(
            "--ip, --gateway, --dns and --mac are required", result.stderr
        )

    def test_rejects_invalid_addresses(self):
        result = self.run_create(
            "--dry-run",
            "--ip", "not-an-ip",
            "--gateway", "192.0.2.1",
            "--dns", "192.0.2.1",
            "--mac", "02:00:00:00:00:30",
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("invalid IPv4", result.stderr)

    def test_rejects_invalid_mac(self):
        result = self.run_create(
            "--dry-run",
            "--ip", "192.0.2.30",
            "--gateway", "192.0.2.1",
            "--dns", "192.0.2.1",
            "--mac", "not-a-mac",
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("invalid MAC", result.stderr)


@unittest.skipUnless(shutil.which("sqlite3"), "sqlite3 CLI is not installed")
class BackupTest(unittest.TestCase):
    def test_online_backup_is_consistent_and_private(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            data = base / "data"
            dest = base / "backups"
            data.mkdir()
            db = data / "bb.db"
            with sqlite3.connect(db) as connection:
                connection.execute("create table threads (id text primary key)")
                connection.execute("insert into threads values ('thr_test')")
            env = base / "env"
            env.write_text(f"BB_DATA_DIR={data}\n")
            result = subprocess.run(
                ["bash", str(BACKUP)],
                check=False,
                capture_output=True,
                text=True,
                env={
                    **os.environ,
                    "BB_BACKUP_ENV_FILE": str(env),
                    "BB_BACKUP_DIR": str(dest),
                },
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            copy = dest / "bb.db"
            self.assertTrue(copy.is_file())
            self.assertEqual(copy.stat().st_mode & 0o777, 0o600)
            with sqlite3.connect(copy) as connection:
                self.assertEqual(
                    connection.execute("select id from threads").fetchone()[0],
                    "thr_test",
                )
                self.assertEqual(
                    connection.execute("pragma integrity_check").fetchone()[0],
                    "ok",
                )

    def test_missing_database_is_a_warning_not_a_failure(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            env = base / "env"
            env.write_text(f"BB_DATA_DIR={base / 'missing'}\n")
            result = subprocess.run(
                ["bash", str(BACKUP)],
                check=False,
                capture_output=True,
                text=True,
                env={
                    **os.environ,
                    "BB_BACKUP_ENV_FILE": str(env),
                    "BB_BACKUP_DIR": str(base / "backups"),
                },
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("skipping bb database backup", result.stderr)
            self.assertFalse((base / "backups" / "bb.db").exists())


if __name__ == "__main__":
    unittest.main()
