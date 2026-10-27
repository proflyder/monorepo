#!/usr/bin/env python3
"""Hermetic tests for projects/home-assistant/stack/ctl.sh using fake docker/curl/ss/systemd."""
from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CTL = ROOT / "projects" / "home-assistant" / "stack" / "ctl.sh"

DOCKER_STUB = r"""#!/bin/sh
printf '%s\n' "docker $*" >> "$DOCKER_LOG"
case "$*" in
  *"compose ps"*) echo "NAME  STATUS";;
  *"compose config"*) exit 0;;
  *"info --format"*) echo "29.0.0";;
  *) exit 0;;
esac
"""
CURL_STUB = r"""#!/bin/sh
printf '200'
"""
SS_STUB = r"""#!/bin/sh
# One line avoids SIGPIPE when the caller uses grep -q; port_open runs under pipefail.
printf 'LISTEN 0 128 127.0.0.1:10200 127.0.0.1:10300 127.0.0.1:8123 127.0.0.1:8095 127.0.0.1:5580 0.0.0.0:*\n'
"""
SYSTEMCTL_STUB = r"""#!/bin/sh
case "$*" in
  *is-system-running*) echo running;;
  *is-active*) echo active;;
  *--failed*) exit 0;;
  *) echo active;;
esac
"""
IP_STUB = r"""#!/bin/sh
echo "eth0 UP 192.0.2.25/24"
echo "default via 192.0.2.1"
"""
SUDO_STUB = r"""#!/bin/sh
exit 1
"""
SLEEP_STUB = r"""#!/bin/sh
exit 0
"""


class CtlTests(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="ctl-test-"))
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)
        self.stack = self.tmp / "stack"
        self.stack.mkdir()
        shutil.copy2(CTL, self.stack / "ctl.sh")
        (self.stack / ".env.example").write_text("HA_URL=http://example\n")
        (self.stack / "docker-compose.yml").write_text("services: {}\n")
        self.bin = self.tmp / "bin"
        self.bin.mkdir()
        for name, body in {
            "docker": DOCKER_STUB,
            "curl": CURL_STUB,
            "ss": SS_STUB,
            "systemctl": SYSTEMCTL_STUB,
            "ip": IP_STUB,
            "sudo": SUDO_STUB,
            "sleep": SLEEP_STUB,
        }.items():
            path = self.bin / name
            path.write_text(body)
            path.chmod(0o755)
        self.docker_log = self.tmp / "docker.log"
        self.env = os.environ.copy()
        self.env["PATH"] = f"{self.bin}:/usr/bin:/bin:/usr/sbin:/sbin"
        self.env["DOCKER_LOG"] = str(self.docker_log)

    def run_ctl(self, *args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [str(self.stack / "ctl.sh"), *args],
            cwd=self.stack,
            env=self.env,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=20,
        )

    def calls(self) -> str:
        return self.docker_log.read_text() if self.docker_log.exists() else ""

    def test_help_and_invalid_command(self):
        help_res = self.run_ctl("help")
        self.assertEqual(help_res.returncode, 0)
        self.assertIn("Usage:", help_res.stdout)
        bad = self.run_ctl("nonsense")
        self.assertEqual(bad.returncode, 2)
        self.assertIn("Usage:", bad.stdout)

    def test_config_and_status(self):
        cfg = self.run_ctl("config")
        self.assertEqual(cfg.returncode, 0)
        self.assertIn("compose config", self.calls())
        status = self.run_ctl("status")
        self.assertEqual(status.returncode, 0)
        self.assertIn("HA      http://127.0.0.1:8123 -> 200", status.stdout)
        self.assertIn("MA      http://127.0.0.1:8095 -> 200", status.stdout)
        self.assertIn("Matter  127.0.0.1:5580 -> open", status.stdout)
        self.assertIn("Whisper 127.0.0.1:10300 -> open", status.stdout)

    def test_service_aliases(self):
        self.assertEqual(self.run_ctl("stop", "ma").returncode, 0)
        self.assertEqual(self.run_ctl("stop", "ha").returncode, 0)
        self.assertEqual(self.run_ctl("stop").returncode, 0)
        self.assertEqual(self.run_ctl("restart", "music-assistant").returncode, 0)
        self.assertEqual(self.run_ctl("restart", "matter").returncode, 0)
        calls = self.calls()
        self.assertIn("compose stop music-assistant", calls)
        self.assertIn("compose stop homeassistant", calls)
        self.assertIn("compose stop\n", calls)
        self.assertIn("compose restart music-assistant", calls)
        self.assertIn("compose restart matter-server", calls)

    def test_up_creates_dirs_and_env(self):
        res = self.run_ctl("up")
        self.assertEqual(res.returncode, 0)
        self.assertTrue((self.stack / ".env").exists())
        for name in ("homeassistant/config", "data/music-assistant", "data/matter-server", "data/whisper", "data/piper", "backups"):
            self.assertTrue((self.stack / name).is_dir(), name)
        self.assertIn("compose up -d --remove-orphans", self.calls())

    def test_doctor(self):
        res = self.run_ctl("doctor")
        self.assertEqual(res.returncode, 0)
        self.assertIn("docker ok", res.stdout)
        self.assertIn("compose config ok", res.stdout)
        self.assertIn("compose config -q", self.calls())

    def test_backup_stops_archives_and_starts(self):
        (self.stack / "homeassistant" / "config").mkdir(parents=True)
        (self.stack / "homeassistant" / "config" / "configuration.yaml").write_text("homeassistant: {}\n")
        for d in ("data/music-assistant", "data/matter-server", "data/whisper", "data/piper"):
            (self.stack / d).mkdir(parents=True)
            (self.stack / d / "keep").write_text("x")
        res = self.run_ctl("backup")
        self.assertEqual(res.returncode, 0, res.stdout)
        archives = list((self.stack / "backups").glob("nettop-*.tar.gz"))
        self.assertEqual(len(archives), 1)
        calls = self.calls()
        self.assertIn("compose stop", calls)
        self.assertIn("compose start", calls)
        self.assertIn("Backup written:", res.stdout)

    def test_logs_and_shell(self):
        logs = self.run_ctl("logs", "ha")
        self.assertEqual(logs.returncode, 0)
        self.assertIn("compose logs -f --tail=200 homeassistant", self.calls())
        serv = self.run_ctl("shell", "ma")
        self.assertEqual(serv.returncode, 0)
        self.assertIn("compose exec music-assistant sh", self.calls())
        matter = self.run_ctl("shell", "matter")
        self.assertEqual(matter.returncode, 0)
        self.assertIn("compose exec matter-server sh", self.calls())
        bad = self.run_ctl("shell")
        self.assertEqual(bad.returncode, 1)
        self.assertIn("usage: ./ctl.sh shell <ha|ma|matter>", bad.stdout)

    def test_down(self):
        self.assertEqual(self.run_ctl("down").returncode, 0)
        self.assertIn("compose down", self.calls())

    def test_pull_and_update(self):
        self.assertEqual(self.run_ctl("pull").returncode, 0)
        upd = self.run_ctl("update")
        self.assertEqual(upd.returncode, 0)
        calls = self.calls()
        self.assertIn("compose pull", calls)
        self.assertIn("compose up -d --remove-orphans", calls)


if __name__ == "__main__":
    unittest.main(verbosity=2)
