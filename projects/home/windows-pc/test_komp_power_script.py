#!/usr/bin/env python3
"""Static contract test for the Windows forced-command wrapper.

The script runs inside the SSH session, so its allowlist is a security
boundary: only the two exact words may reach shutdown.exe and nothing else may
execute. This test does not run PowerShell; it checks the source shape.

Run: python3 -m unittest discover -s projects/home/windows-pc -p 'test_*.py'
"""
from __future__ import annotations

import re
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
SCRIPT = HERE / "komp-power.ps1"
INSTALL = HERE / "install-lock-task.ps1"


class KompPowerAllowlistTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.raw = SCRIPT.read_bytes()
        cls.text = cls.raw.decode("utf-8-sig")

    def test_script_exists_and_is_utf8_with_bom(self):
        # Windows PowerShell 5.1 reads .ps1 without a BOM in the legacy ANSI
        # code page and would mangle the Russian shutdown message.
        self.assertTrue(SCRIPT.is_file())
        self.assertTrue(self.raw.startswith(b"\xef\xbb\xbf"), "komp-power.ps1 must be UTF-8 with BOM")

    def test_reads_the_original_ssh_command(self):
        self.assertIn("$env:SSH_ORIGINAL_COMMAND", self.text)

    def test_only_shutdown_cancel_and_lock_are_compared(self):
        comparisons = re.findall(r"-ceq\s+'([^']*)'", self.text)
        self.assertEqual(comparisons, ["shutdown", "cancel", "lock"])
        # Case-insensitive `-eq`/`-ieq` would widen the allowlist.
        self.assertNotRegex(self.text, r"-i?eq\s")

    def test_shutdown_actions_are_exactly_the_two_documented_ones(self):
        self.assertIn("/s /t 60", self.text)
        self.assertIn("/c 'Рикардо: выключение через минуту'", self.text)
        self.assertIn("& shutdown.exe /a", self.text)
        # Everything else must be rejected with a non-zero exit code.
        self.assertIn("exit 1", self.text)
        self.assertRegex(self.text, r"\[Console\]::Error\.WriteLine\('komp-power: ")

    def test_lock_runs_only_the_fixed_scheduled_task(self):
        # LockWorkStation from the SSH service session would not lock the
        # user's desktop, so `lock` may only start the pre-created task.
        self.assertIn(r"& schtasks.exe /run /tn '\Home\KompLock'", self.text)
        code = re.sub(r"<#.*?#>", "", self.text, flags=re.S)
        code = "\n".join(line for line in code.splitlines() if not line.lstrip().startswith("#"))
        self.assertNotIn("LockWorkStation", code)
        self.assertNotIn("rundll32", code)
        self.assertEqual(self.text.count("schtasks.exe"), 1)

    def test_install_script_creates_the_interactive_task(self):
        install = INSTALL.read_text(encoding="utf-8")
        self.assertIn("KompLock", install)
        self.assertIn(r"$TaskPath = '\Home\'", install)
        self.assertIn("rundll32.exe", install)
        self.assertIn("user32.dll,LockWorkStation", install)
        self.assertIn("-LogonType Interactive", install)
        self.assertIn("Register-ScheduledTask", install)
        # No trigger: the task only runs on demand from komp-power.ps1.
        self.assertNotIn("New-ScheduledTaskTrigger", install)

    def test_no_general_purpose_execution_surfaces(self):
        # Comments document the authorized_keys line ("-Command", "cmd.exe"),
        # so scan only executable code.
        code = re.sub(r"<#.*?#>", "", self.text, flags=re.S)
        code = "\n".join(line for line in code.splitlines() if not line.lstrip().startswith("#"))
        code = code.lower()
        for forbidden in (
            "invoke-expression",
            "iex ",
            "start-process",
            "add-type",
            "new-object",
            "out-file",
            "set-content",
            "add-content",
            "-command",
            "cmd.exe",
            "read-host",
            "[scriptblock]",
            ".invoke(",
        ):
            self.assertNotIn(forbidden, code, f"unexpected execution surface: {forbidden}")


if __name__ == "__main__":
    unittest.main()
