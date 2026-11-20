#!/usr/bin/env python3
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
        self.assertTrue(SCRIPT.is_file())
        self.assertTrue(self.raw.startswith(b"\xef\xbb\xbf"), "komp-power.ps1 must be UTF-8 with BOM")

    def test_reads_the_original_ssh_command(self):
        self.assertIn("$env:SSH_ORIGINAL_COMMAND", self.text)

    def test_only_the_documented_words_are_compared(self):
        comparisons = re.findall(r"-ceq\s+'([^']*)'", self.text)
        self.assertEqual(comparisons, ["shutdown", "cancel", "lock", "status"])
        self.assertNotRegex(self.text, r"-i?eq\s")

    def test_shutdown_actions_are_exactly_the_two_documented_ones(self):
        self.assertIn("/s /t 60", self.text)
        self.assertIn("/c 'Рикардо: выключение через минуту'", self.text)
        self.assertIn("& shutdown.exe /a", self.text)
        self.assertIn("exit 1", self.text)
        self.assertRegex(self.text, r"\[Console\]::Error\.WriteLine\('komp-power: ")

    def test_lock_runs_only_the_fixed_scheduled_task(self):
        self.assertIn(r"& schtasks.exe /run /tn '\Home\KompLock'", self.text)
        code = re.sub(r"<#.*?#>", "", self.text, flags=re.S)
        code = "\n".join(line for line in code.splitlines() if not line.lstrip().startswith("#"))
        self.assertNotIn("LockWorkStation", code)
        self.assertNotIn("rundll32", code)
        self.assertEqual(self.text.count("schtasks.exe"), 1)

    def test_status_reports_the_lock_state_without_any_execution(self):
        self.assertIn("Get-Process -Name LogonUI", self.text)
        self.assertIn("Get-Process -Name explorer", self.text)
        self.assertIn("'locked'", self.text)
        self.assertIn("'unlocked'", self.text)
        status = self.text.split("if ($requested -ceq 'status')", 1)[1]
        status = status.split("[Console]::Error", 1)[0]
        self.assertIn("exit 0", status)
        self.assertNotIn("Write-Output", status)
        self.assertNotIn("schtasks", status)
        self.assertNotIn("shutdown", status)
        self.assertNotIn("ssh", status.lower())

    def test_install_script_creates_the_interactive_task(self):
        install = INSTALL.read_text(encoding="utf-8")
        self.assertIn("KompLock", install)
        self.assertIn(r"$TaskPath = '\Home\'", install)
        self.assertIn("rundll32.exe", install)
        self.assertIn("user32.dll,LockWorkStation", install)
        self.assertIn("-LogonType Interactive", install)
        self.assertIn("Register-ScheduledTask", install)
        self.assertNotIn("New-ScheduledTaskTrigger", install)

    def test_no_general_purpose_execution_surfaces(self):
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
