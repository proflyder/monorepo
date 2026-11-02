"""Hermetic checks for the bb Proxmox scripts (no Proxmox, no network)."""
import getpass
import io
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).with_name("bb")
CREATE = ROOT / "create-lxc.sh"
BACKUP = ROOT / "bb-db-backup.sh"


def make_export_cli(directory: Path, body: str) -> Path:
    cli = directory / "bb"
    cli.write_text("#!/bin/sh\n" + body, encoding="utf-8")
    cli.chmod(0o755)
    return cli


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
            export_dir = base / "export"
            (export_dir / "files").mkdir(parents=True)
            (export_dir / "files" / "bb.db").write_bytes(b"sqlite-export")
            cli = make_export_cli(
                base,
                'out=\n'
                'while [ $# -gt 0 ]; do\n'
                '  case $1 in --out) out=$2; shift 2 ;; *) shift ;; esac\n'
                'done\n'
                'tar -czf "$out" -C "$BB_FAKE_EXPORT_DIR" files\n',
            )
            result = self.run_backup(base, cli, export_dir)
            self.assertEqual(result.returncode, 0, result.stderr)
            copy = base / "backups" / "bb.db"
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

    def run_backup(self, base: Path, cli: Path, export_dir: Path | None = None,
                   extra_env: dict[str, str] | None = None):
        data = base / "data"
        data.mkdir(exist_ok=True)
        db = data / "bb.db"
        if not db.exists():
            with sqlite3.connect(db) as connection:
                connection.execute("create table threads (id text primary key)")
                connection.execute("insert into threads values ('thr_test')")
        env = base / "env"
        env.write_text(f"BB_DATA_DIR={data}\n")
        environment = {
            **os.environ,
            "BB_BACKUP_ENV_FILE": str(env),
            "BB_BACKUP_DIR": str(base / "backups"),
            "BB_CLI": str(cli),
            "BB_USER": getpass.getuser(),
            "BB_HOME": str(base / "home"),
        }
        if export_dir is not None:
            environment["BB_FAKE_EXPORT_DIR"] = str(export_dir)
        if extra_env is not None:
            environment.update(extra_env)
        return subprocess.run(
            ["bash", str(BACKUP)],
            check=False,
            capture_output=True,
            text=True,
            env=environment,
        )

    def test_server_export_is_written_next_to_the_sqlite_copy(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            export_dir = base / "export"
            (export_dir / "files").mkdir(parents=True)
            (export_dir / "files" / "bb.db").write_bytes(b"sqlite-export")
            cli = make_export_cli(
                base,
                'out=\n'
                'while [ $# -gt 0 ]; do\n'
                '  case $1 in --out) out=$2; shift 2 ;; *) shift ;; esac\n'
                'done\n'
                '[ -n "$out" ] || exit 2\n'
                'tar -czf "$out" -C "$BB_FAKE_EXPORT_DIR" files\n',
            )
            result = self.run_backup(base, cli, export_dir)
            self.assertEqual(result.returncode, 0, result.stderr)
            export = base / "backups" / "bb-server-export.tgz"
            self.assertTrue(export.is_file())
            self.assertEqual(export.stat().st_mode & 0o777, 0o600)
            self.assertTrue((base / "backups" / "bb.db").is_file())
            listing = subprocess.run(
                ["tar", "-tzf", str(export)],
                check=True,
                capture_output=True,
                text=True,
            ).stdout
            self.assertIn("files/bb.db", listing)

    def test_failed_export_still_keeps_the_sqlite_copy_and_fails_the_run(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            cli = make_export_cli(base, "exit 1\n")
            result = self.run_backup(base, cli)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("bb server export failed", result.stderr)
            self.assertTrue((base / "backups" / "bb.db").is_file())
            self.assertFalse((base / "backups" / "bb-server-export.tgz").exists())

    def test_export_without_bb_db_is_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            export_dir = base / "export"
            (export_dir / "files").mkdir(parents=True)
            (export_dir / "files" / "attachments").write_bytes(b"data")
            cli = make_export_cli(
                base,
                'out=\n'
                'while [ $# -gt 0 ]; do\n'
                '  case $1 in --out) out=$2; shift 2 ;; *) shift ;; esac\n'
                'done\n'
                'tar -czf "$out" -C "$BB_FAKE_EXPORT_DIR" files\n',
            )
            result = self.run_backup(base, cli, export_dir)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("does not contain files/bb.db", result.stderr)
            self.assertFalse((base / "backups" / "bb-server-export.tgz").exists())

    def test_export_with_broken_gzip_is_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            cli = make_export_cli(
                base,
                'out=\n'
                'while [ $# -gt 0 ]; do\n'
                '  case $1 in --out) out=$2; shift 2 ;; *) shift ;; esac\n'
                'done\n'
                'printf "not gzip" > "$out"\n',
            )
            result = self.run_backup(base, cli)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("not a valid gzip archive", result.stderr)
            self.assertFalse((base / "backups" / "bb-server-export.tgz").exists())

    def test_export_listing_is_not_truncated_by_early_grep_exit(self):
        """Regression: `grep -q` + pipefail made tar die of SIGPIPE.

        bsdtar on macOS ignores the closed pipe, so the listing is produced by
        a small GNU-tar-like streamer: it writes every member and exits
        non-zero when grep stops reading after the first match.
        """
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            source = base / "export.tgz"
            with tarfile.open(source, mode="w:gz") as archive:
                first = tarfile.TarInfo("files/bb.db")
                first.size = len(b"sqlite-export")
                archive.addfile(first, io.BytesIO(b"sqlite-export"))
                # The first match is followed by far more listing output than a
                # pipe buffer, so tar must keep writing after grep saw it.
                for index in range(3000):
                    info = tarfile.TarInfo(f"files/{index:05d}-" + "x" * 120)
                    info.size = 0
                    archive.addfile(info)
            fake_bin = base / "fakebin"
            fake_bin.mkdir()
            fake_tar = fake_bin / "tar"
            fake_tar.write_text(
                "#!/usr/bin/env python3\n"
                "import sys, tarfile\n"
                'archive = sys.argv[sys.argv.index("-tzf") + 1]\n'
                "try:\n"
                "    with tarfile.open(archive) as handle:\n"
                "        for member in handle:\n"
                "            print(member.name, flush=True)\n"
                "except BrokenPipeError:\n"
                "    sys.exit(141)\n",
                encoding="utf-8",
            )
            fake_tar.chmod(0o755)
            cli = make_export_cli(
                base,
                'out=\n'
                'while [ $# -gt 0 ]; do\n'
                '  case $1 in --out) out=$2; shift 2 ;; *) shift ;; esac\n'
                'done\n'
                'cat "$BB_FAKE_EXPORT_DIR" > "$out"\n',
            )
            path = f"{fake_bin}:{os.environ.get('PATH', '')}"
            result = self.run_backup(base, cli, source, {"PATH": path})
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertTrue((base / "backups" / "bb-server-export.tgz").is_file())


if __name__ == "__main__":
    unittest.main()
