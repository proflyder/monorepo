#!/usr/bin/env python3
"""Static contract tests for the Windows logon provider scripts.

The scripts run elevated on the owner's PC, so what matters here is the shape:
one fixed CLSID everywhere, registration of exactly one credential provider and
of no filters, exact-key removal on uninstall, machine-scope DPAPI with the same
entropy as the Rust code, and no secret ever printed.

Run: python3 -m unittest discover -s solutions/windows-logon-provider
"""
from __future__ import annotations

import re
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
PROVIDER = HERE / "crates" / "provider" / "src"

CLSID_FIXED = "{9459CCEA-4CCC-4435-89BF-E0E132E0C27F}"
DLL_NAME = "RicardoKompLogon.dll"
CONFIG_KEY = r"HKLM:\SOFTWARE\Ricardo\KompLogon"
PROVIDERS_KEY = (
    r"HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Authentication\Credential Providers"
)
CLASSES_KEY = r"HKLM:\SOFTWARE\Classes\CLSID"
SECRET_PATH = r"C:\ProgramData\Ricardo\KompLogon\secret.bin"
DATA_DIR = r"C:\ProgramData\Ricardo\KompLogon"


def read_ps1(name: str) -> tuple[bytes, str]:
    raw = (HERE / name).read_bytes()
    return raw, raw.decode("utf-8-sig")


def clsid_from_source() -> str:
    text = (PROVIDER / "provider.rs").read_text(encoding="utf-8")
    block = text.split("pub const PROVIDER_CLSID: GUID = GUID {", 1)[1].split("};", 1)[0]
    data1 = int(re.search(r"data1:\s*(0x[0-9a-fA-F_]+)", block).group(1).replace("_", ""), 0)
    data2 = int(re.search(r"data2:\s*(0x[0-9a-fA-F_]+)", block).group(1).replace("_", ""), 0)
    data3 = int(re.search(r"data3:\s*(0x[0-9a-fA-F_]+)", block).group(1).replace("_", ""), 0)
    data4_block = re.search(r"data4:\s*\[([^\]]+)\]", block).group(1)
    data4 = [int(value, 0) for value in re.findall(r"0x[0-9a-fA-F]+", data4_block)]
    if len(data4) != 8:
        raise AssertionError(f"expected 8 data4 bytes, got {data4}")
    head = f"{{{data1:08X}-{data2:04X}-{data3:04X}-{data4[0]:02X}{data4[1]:02X}"
    tail = "".join(f"{value:02X}" for value in data4[2:])
    return f"{head}-{tail}}}"


class ScriptBasicsTests(unittest.TestCase):
    def test_scripts_are_utf8_with_bom(self):
        # Windows PowerShell 5.1 reads .ps1 without a BOM as ANSI and would
        # mangle the Russian prompts and messages.
        for name in ("install.ps1", "set-secret.ps1", "uninstall.ps1", "status.ps1"):
            with self.subTest(script=name):
                raw, _ = read_ps1(name)
                self.assertTrue(raw.startswith(b"\xef\xbb\xbf"), f"{name} must be UTF-8 with BOM")

    def test_every_script_requires_elevation(self):
        for name in ("install.ps1", "set-secret.ps1", "uninstall.ps1", "status.ps1"):
            with self.subTest(script=name):
                _, text = read_ps1(name)
                self.assertIn("#Requires -RunAsAdministrator", text)
                self.assertIn("$ErrorActionPreference", text)


class ClsidTests(unittest.TestCase):
    def test_provider_clsid_matches_the_documented_value(self):
        self.assertEqual(clsid_from_source(), CLSID_FIXED)

    def test_all_scripts_use_the_same_clsid(self):
        for name in ("install.ps1", "uninstall.ps1", "status.ps1"):
            with self.subTest(script=name):
                _, text = read_ps1(name)
                self.assertIn(f"$ProviderClsid = '{CLSID_FIXED}'", text)

    def test_secret_script_does_not_touch_registration(self):
        _, text = read_ps1("set-secret.ps1")
        self.assertNotIn("CLSID", text)
        self.assertNotIn("Registry", text.replace("HKLM", ""))

    def test_no_other_guid_is_registered(self):
        _, text = read_ps1("install.ps1")
        guids = set(re.findall(r"\{[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}\}", text))
        self.assertEqual(guids, {CLSID_FIXED})


class InstallScriptTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.raw, cls.text = read_ps1("install.ps1")

    def test_registers_inproc_server_with_apartment_threading(self):
        self.assertIn("$ClassesKey = \"HKLM:\\SOFTWARE\\Classes\\CLSID\\$ProviderClsid\"", self.text)
        self.assertIn("$InprocKey = \"$ClassesKey\\InprocServer32\"", self.text)
        self.assertIn("Set-ItemProperty -Path $InprocKey -Name 'ThreadingModel' -Value 'Apartment'", self.text)
        self.assertIn("Set-ItemProperty -Path $InprocKey -Name '(default)' -Value $installedDll", self.text)

    def test_registers_exactly_one_credential_provider(self):
        self.assertIn(f"$CredentialProvidersKey = '{PROVIDERS_KEY}'", self.text)
        self.assertIn('$ProviderKey = "$CredentialProvidersKey\\$ProviderClsid"', self.text)
        self.assertIn("New-Item -Path $ProviderKey -Force", self.text)

    def test_never_registers_a_filter(self):
        for name in ("install.ps1", "uninstall.ps1", "status.ps1"):
            with self.subTest(script=name):
                _, text = read_ps1(name)
                self.assertNotIn("Credential Provider Filters", text)
                self.assertNotIn("CredentialProviderFilter", text)
                self.assertNotIn("FilterProviders", text)

    def test_configuration_values_written_match_the_rust_reader(self):
        config_rs = (PROVIDER / "config.rs").read_text(encoding="utf-8")
        for value in ("ServiceUrl", "Machine", "UserName", "Domain", "Disabled", "FailCount"):
            with self.subTest(value=value):
                self.assertIn(f'w!("{value}")', config_rs)
                self.assertIn(f"-Name '{value}'", self.text)
        self.assertIn(f"$ConfigKey = '{CONFIG_KEY}'", self.text)
        # Reinstall resets the disable latch.
        self.assertIn("-Name 'FailCount' -Value 0 -Type DWord", self.text)
        self.assertIn("-Name 'Disabled' -Value 0 -Type DWord", self.text)

    def test_dll_is_copied_and_declared_consistently(self):
        self.assertIn(f"$DllName = '{DLL_NAME}'", self.text)
        self.assertIn("Copy-Item -LiteralPath $sourceDll -Destination $installedDll -Force", self.text)
        build = (HERE / "build.sh").read_text(encoding="utf-8")
        self.assertIn(DLL_NAME, build)
        # The exported names are the COM contract; the PE verifier enforces
        # exactly these two exports.
        self.assertIn('"DllGetClassObject", "DllCanUnloadNow"', (HERE / "pe_artifacts.py").read_text(encoding="utf-8"))

    def test_directory_acls_are_system_and_administrators_only(self):
        # Program Files: SYSTEM + Administrators write, Users read/execute.
        self.assertIn("*S-1-5-18:(OI)(CI)F", self.text)
        self.assertIn("*S-1-5-32-544:(OI)(CI)F", self.text)
        self.assertIn("*S-1-5-32-545:(OI)(CI)RX", self.text)
        # Data dir (secret + log): only SYSTEM and Administrators, no Users.
        data_block = self.text.split("# --- data directory", 1)[1].split("# --- configuration", 1)[0]
        self.assertNotIn("S-1-5-32-545", data_block)
        for broad in ("S-1-1-0", "S-1-5-11", "Everyone", "Authenticated Users", "BUILTIN\\Users:F"):
            self.assertNotIn(broad, self.text, f"broad ACL grant: {broad}")

    def test_validates_inputs_before_writing_anything(self):
        self.assertIn("$Machine -notmatch '^[a-z0-9-]{1,32}$'", self.text)
        self.assertIn("$ServiceUrl -notmatch", self.text)
        self.assertLess(self.text.index("$Machine -notmatch"), self.text.index("Copy-Item"))


class UninstallScriptTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.raw, cls.text = read_ps1("uninstall.ps1")

    def test_removes_both_registrations(self):
        self.assertIn('$ProviderKey = "$CredentialProvidersKey\\$ProviderClsid"', self.text)
        self.assertIn('$ClassesKey = "HKLM:\\SOFTWARE\\Classes\\CLSID\\$ProviderClsid"', self.text)
        for variable in ("$ProviderKey", "$ClassesKey"):
            with self.subTest(key=variable):
                self.assertRegex(self.text, rf"Remove-Item -LiteralPath {re.escape(variable)} -Recurse -Force")

    def test_deregisters_before_deleting_files(self):
        self.assertLess(self.text.index("Remove-Item -LiteralPath $ProviderKey"), self.text.index("$installedDll"))
        self.assertLess(self.text.index("Remove-Item -LiteralPath $ClassesKey"), self.text.index("$installedDll"))

    def test_leaves_logs_and_config(self):
        self.assertNotIn("logon.log", self.text)
        self.assertNotIn("Remove-Item -LiteralPath $ConfigKey", self.text)

    def test_secret_is_removed_unless_kept(self):
        self.assertIn("$KeepSecret", self.text)
        self.assertIn("secret.bin", self.text)
        self.assertIn("Remove-Item -LiteralPath $secretPath -Force", self.text)

    def test_no_broad_registry_or_file_deletion(self):
        # Only two exact CLSID keys and the installed files may be touched.
        removals = re.findall(r"Remove-Item\s+-LiteralPath\s+(\$\w+|\"[^\"]*\")", self.text)
        self.assertGreaterEqual(len(removals), 3)
        for target in removals:
            self.assertNotIn("*", target)
        self.assertNotIn("HKLM:\\SOFTWARE\\Classes\\CLSID -Recurse", self.text)


class SecretScriptTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.raw, cls.text = read_ps1("set-secret.ps1")

    def test_uses_machine_scope_dpapi_with_the_rust_entropy(self):
        config_rs = (PROVIDER / "config.rs").read_text(encoding="utf-8")
        self.assertIn('SECRET_ENTROPY: &[u8] = b"ricardo-komp-logon-v1"', config_rs)
        self.assertIn("$Entropy = 'ricardo-komp-logon-v1'", self.text)
        self.assertIn("DataProtectionScope]::LocalMachine", self.text)
        self.assertIn("ProtectedData]::Protect", self.text)

    def test_writes_the_documented_secret_path(self):
        self.assertIn("$SecretPath = Join-Path $DataDir 'secret.bin'", self.text)
        self.assertIn(r"$DataDir = 'C:\ProgramData\Ricardo\KompLogon'", self.text)
        config_rs = (PROVIDER / "config.rs").read_text(encoding="utf-8")
        self.assertIn(r'SECRET_PATH: &str = r"C:\ProgramData\Ricardo\KompLogon\secret.bin"', config_rs)

    def test_secret_never_printed(self):
        self.assertNotIn("Write-Host", self.text)
        for line in self.text.splitlines():
            stripped = line.strip()
            if not stripped.startswith(("Write-Output", "Write-Warning", "Write-Error")):
                continue
            for secret in ("$plainPassword", "$payload", "$Key", "$keyBytes", "$bstr", "$blob"):
                with self.subTest(line=stripped, secret=secret):
                    self.assertNotIn(secret, stripped)
        self.assertIn("Password and key were not printed", self.text)

    def test_password_arrives_as_secure_string_or_prompt(self):
        self.assertIn("[System.Security.SecureString]$Password", self.text)
        self.assertIn("Read-Host -AsSecureString", self.text)
        self.assertIn("[Runtime.InteropServices.Marshal]::SecureStringToBSTR", self.text)
        self.assertIn("ZeroFreeBSTR", self.text)

    def test_key_is_32_bytes_and_may_come_from_stdin(self):
        self.assertIn("$keyBytes.Length -ne 32", self.text)
        self.assertIn("FromBase64String", self.text)
        self.assertIn("[Console]::In.ReadLine()", self.text)
        self.assertIn("IsInputRedirected", self.text)

    def test_secret_file_acl_is_system_and_administrators_only(self):
        secret_block = self.text.rsplit("Set-AclStrict", 1)[1]
        self.assertIn("*S-1-5-18:F", secret_block)
        self.assertIn("*S-1-5-32-544:F", secret_block)
        self.assertNotIn("S-1-5-32-545", secret_block)


class ProviderSourceTests(unittest.TestCase):
    def test_dll_exports_only_the_two_com_entry_points(self):
        exports = re.findall(
            r'#\[unsafe\(no_mangle\)\]\s*\npub (?:unsafe )?extern "system" fn (Dll\w+)',
            (PROVIDER / "dll.rs").read_text(encoding="utf-8"),
        )
        self.assertEqual(sorted(exports), ["DllCanUnloadNow", "DllGetClassObject"])

    def test_build_script_uses_bazel_and_verifies_the_pe(self):
        build = (HERE / "build.sh").read_text(encoding="utf-8")
        self.assertIn("--platforms=\"$WINDOWS_PLATFORM\"", build)
        self.assertIn("//solutions/windows-logon-provider/crates/provider:provider", build)
        self.assertIn("//solutions/windows-logon-provider/crates/cp-selftest:cp-selftest", build)
        self.assertIn("pe_artifacts.py normalize", build)
        self.assertIn("pe_artifacts.py verify", build)

    def test_only_logon_claims_grants(self):
        provider = (PROVIDER / "provider.rs").read_text(encoding="utf-8")
        scenario = provider.split("fn set_usage_scenario(", 1)[1].split("\n        fn advise", 1)[0]
        self.assertIn("cpus != CPUS_LOGON && cpus != CPUS_UNLOCK_WORKSTATION", scenario)
        self.assertIn("return Err(E_NOTIMPL.into());", scenario)
        # An unlock must be inert: the logon branch is the only one that starts
        # the challenge/claim worker.
        unlock_branch, logon_tail = scenario.split("if cpus != CPUS_LOGON {", 1)[1].split(
            "            self.start_worker_once();", 1
        )
        self.assertIn("claims=0", unlock_branch)
        self.assertIn("state.armed = false;", scenario)
        self.assertNotIn("start_worker_once", unlock_branch)
        self.assertIn("Ok(())", logon_tail)

    def test_session_and_uptime_gates_run_before_the_worker(self):
        provider = (PROVIDER / "provider.rs").read_text(encoding="utf-8")
        start = provider.split("fn start_worker_once(", 1)[1].split(
            "\n        fn set_usage_scenario", 1
        )[0]
        self.assertIn("crate::gate::evaluate()", start)
        self.assertIn("gate={}", start)
        # A denied gate must return before the network thread is created.
        gate_at = start.index("crate::gate::evaluate()")
        self.assertLess(gate_at, start.index("start_worker_thread(&self.session)"))
        self.assertIn("log_event(\"scenario\", \"cpus=logon gate=ok\")", start)

    def test_worker_and_credentials_are_logon_only(self):
        provider = (PROVIDER / "provider.rs").read_text(encoding="utf-8")
        start = provider.split("fn start_worker_once(", 1)[1].split("\n        fn set_usage_scenario", 1)[0]
        self.assertIn("session_scenario(&self.session) != CPUS_LOGON", start)
        count = provider.split("fn get_credential_count(", 1)[1].split("\n        fn get_credential_at", 1)[0]
        self.assertIn("session_scenario(&self.session) != CPUS_LOGON", count)
        self.assertIn("pdwcount.write(0)", count)
        for unsupported in ("CPUS_CHANGE_PASSWORD", "CPUS_CREDUI", "CPUS_PLAP"):
            self.assertNotIn(f"if cpus == {unsupported}", provider)

    def test_log_and_data_paths_match_the_documentation(self):
        log_rs = (PROVIDER / "log.rs").read_text(encoding="utf-8")
        self.assertIn(r'LOG_PATH: &str = r"C:\ProgramData\Ricardo\KompLogon\logon.log"', log_rs)
        self.assertIn("MAX_LOG_BYTES: u64 = 256 * 1024", log_rs)
        self.assertIn(DATA_DIR, read_ps1("status.ps1")[1])
        self.assertIn("logon.log", read_ps1("status.ps1")[1])
        self.assertIn("-Tail $Tail", read_ps1("status.ps1")[1])

    def test_provider_never_blocks_with_unbounded_waits(self):
        worker = (PROVIDER / "worker.rs").read_text(encoding="utf-8")
        self.assertIn("WORKER_BUDGET_MS", worker)
        self.assertNotIn("thread::sleep(Duration::from_millis(2000))", worker)
        self.assertIn("sleep_interruptible", worker)
        self.assertIn("CreateThread", worker)
        # The DLL must stay mapped while the worker runs.
        self.assertIn("ACTIVE_WORKERS", (PROVIDER / "dll.rs").read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()
