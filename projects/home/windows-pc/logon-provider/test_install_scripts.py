#!/usr/bin/env python3
"""Static contract tests for the Windows logon provider scripts.

The scripts run elevated on the owner's PC, so what matters here is the shape:
one fixed CLSID everywhere, registration of exactly one credential provider and
of no filters, exact-key removal on uninstall, machine-scope DPAPI with the same
entropy as the C++ code, and no secret ever printed.

Run: python3 -m unittest discover -s projects/home/windows-pc/logon-provider
"""
from __future__ import annotations

import re
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
SRC = HERE / "src"

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


def assignment(text: str, name: str) -> str:
    match = re.search(rf"^\s*\${re.escape(name)}\s*=\s*(.+)$", text, re.MULTILINE)
    return match.group(1).strip() if match else ""


def clsid_from_header() -> str:
    text = (SRC / "provider_guid.h").read_text(encoding="utf-8")
    values = [int(value, 16) for value in re.findall(r"0x([0-9a-fA-F]+)", text.split("kProviderClsid")[1])]
    if len(values) != 11:
        raise AssertionError(f"expected 11 GUID components, got {values}")
    head = f"{{{values[0]:08X}-{values[1]:04X}-{values[2]:04X}-{values[3]:02X}{values[4]:02X}"
    tail = "".join(f"{value:02X}" for value in values[5:])
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
    def test_header_clsid_matches_the_documented_value(self):
        self.assertEqual(clsid_from_header(), CLSID_FIXED)

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

    def test_configuration_values_written_match_the_cpp_reader(self):
        config_cpp = (SRC / "config.cpp").read_text(encoding="utf-8")
        for value in ("ServiceUrl", "Machine", "UserName", "Domain", "Disabled", "FailCount"):
            with self.subTest(value=value):
                self.assertIn(f'L"{value}"', config_cpp)
                self.assertIn(f"-Name '{value}'", self.text)
        self.assertIn(f"$ConfigKey = '{CONFIG_KEY}'", self.text)
        # Reinstall resets the disable latch.
        self.assertIn("-Name 'FailCount' -Value 0 -Type DWord", self.text)
        self.assertIn("-Name 'Disabled' -Value 0 -Type DWord", self.text)

    def test_dll_is_copied_and_declared_consistently(self):
        self.assertIn(f"$DllName = '{DLL_NAME}'", self.text)
        self.assertIn("Copy-Item -LiteralPath $sourceDll -Destination $installedDll -Force", self.text)
        self.assertIn(f"LIBRARY {DLL_NAME}", (SRC / "ricardo_logon.def").read_text(encoding="utf-8"))
        self.assertIn(DLL_NAME, (HERE / "build.sh").read_text(encoding="utf-8"))

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

    def test_uses_machine_scope_dpapi_with_the_cpp_entropy(self):
        config_h = (SRC / "config.h").read_text(encoding="utf-8")
        self.assertIn('kSecretEntropy[] = "ricardo-komp-logon-v1"', config_h)
        self.assertIn("$Entropy = 'ricardo-komp-logon-v1'", self.text)
        self.assertIn("DataProtectionScope]::LocalMachine", self.text)
        self.assertIn("ProtectedData]::Protect", self.text)

    def test_writes_the_documented_secret_path(self):
        self.assertIn(f"$SecretPath = Join-Path $DataDir 'secret.bin'", self.text)
        self.assertIn(r"$DataDir = 'C:\ProgramData\Ricardo\KompLogon'", self.text)
        config_h = (SRC / "config.h").read_text(encoding="utf-8")
        self.assertIn(r'kSecretPath[] = L"C:\\ProgramData\\Ricardo\\KompLogon\\secret.bin"', config_h)

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
    def test_def_exports_only_the_two_com_entry_points(self):
        exports = re.findall(
            r"^\s+(Dll\w+)\s+PRIVATE", (SRC / "ricardo_logon.def").read_text(encoding="utf-8"), re.MULTILINE
        )
        self.assertEqual(sorted(exports), ["DllCanUnloadNow", "DllGetClassObject"])

    def test_build_script_compiles_every_source_once(self):
        build = (HERE / "build.sh").read_text(encoding="utf-8")
        sources = [
            path.relative_to(HERE).as_posix()
            for path in sorted(SRC.rglob("*.cpp"))
            if path.name not in {"cp_selftest.cpp", "hmac_selftest.cpp", "hmac_sha256_reference.cpp"}
        ]
        # The DLL links each source once; the CNG HMAC self-test reuses
        # hmac_bcrypt.cpp and protocol.cpp in its own executable.
        dll_block = build.split('echo "== DLL =="', 1)[1].split(
            'echo "== self-test executable =="', 1
        )[0]
        selftest_blocks = build.split('echo "== self-test executable =="', 1)[1]
        for source in sources:
            with self.subTest(source=source):
                self.assertEqual(dll_block.count(source), 1, f"{source} must be compiled exactly once into the DLL")
                self.assertLessEqual(
                    selftest_blocks.count(source), 1, f"{source} must be linked at most once into self-tests"
                )
        # The portable reference HMAC must never be linked into the DLL.
        self.assertNotIn("hmac_sha256_reference.cpp", build)

    def test_only_logon_claims_grants(self):
        provider = (SRC / "provider.cpp").read_text(encoding="utf-8")
        scenario = provider.split("HRESULT Provider::SetUsageScenario", 1)[1].split("\n}", 1)[0]
        self.assertIn("cpus != CPUS_LOGON && cpus != CPUS_UNLOCK_WORKSTATION", scenario)
        self.assertIn("return E_NOTIMPL;", scenario)
        # An unlock must be inert: the logon branch is the only one that starts
        # the challenge/claim worker.
        unlock_branch, logon_tail = scenario.split("if (cpus != CPUS_LOGON) {", 1)[1].split("\n    }", 1)
        self.assertIn("claims=0", unlock_branch)
        self.assertIn("session_->armed = false;", scenario)
        self.assertNotIn("StartWorkerOnce", unlock_branch)
        self.assertIn("StartWorkerOnce", logon_tail)
        self.assertIn('"cpus=logon"', logon_tail)

    def test_worker_and_credentials_are_logon_only(self):
        provider = (SRC / "provider.cpp").read_text(encoding="utf-8")
        start = provider.split("void Provider::StartWorkerOnce", 1)[1].split("\n}", 1)[0]
        self.assertIn("SessionScenario(session_) != CPUS_LOGON", start)
        count = provider.split("HRESULT Provider::GetCredentialCount", 1)[1].split("\n}", 1)[0]
        self.assertIn("SessionScenario(session_) != CPUS_LOGON", count)
        self.assertIn("*pdwCount = 0;", count)
        for unsupported in ("CPUS_CHANGE_PASSWORD", "CPUS_CREDUI", "CPUS_PLAP"):
            self.assertNotIn(f"case {unsupported}:", provider)

    def test_log_and_data_paths_match_the_documentation(self):
        log_cpp = (SRC / "log.cpp").read_text(encoding="utf-8")
        self.assertIn(r'"C:\\ProgramData\\Ricardo\\KompLogon\\logon.log"', log_cpp)
        self.assertIn("256ull * 1024ull", log_cpp)
        self.assertIn(DATA_DIR, read_ps1("status.ps1")[1])
        self.assertIn("logon.log", read_ps1("status.ps1")[1])
        self.assertIn("-Tail $Tail", read_ps1("status.ps1")[1])

    def test_provider_never_blocks_with_unbounded_waits(self):
        worker = (SRC / "worker.cpp").read_text(encoding="utf-8")
        self.assertIn("kWorkerBudgetMs", worker)
        self.assertNotIn("Sleep(2000)", worker)
        self.assertIn("SleepInterruptible", worker)
        self.assertIn("CreateThread", worker)
        # The DLL must stay mapped while the worker runs.
        self.assertIn("g_active_workers", (SRC / "dll.cpp").read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()
