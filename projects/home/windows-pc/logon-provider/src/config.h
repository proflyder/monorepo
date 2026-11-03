// Provider configuration: HKLM registry + DPAPI-protected secret file.
//
// All reads are cheap (registry + one small file) and are done from the worker
// thread or from GetSerialization, never from a speculatively repeated UI path.

#pragma once

#include <windows.h>

#include <string>

#include "protocol/protocol.h"

namespace ricardo {
namespace komp {

constexpr wchar_t kRegistryKey[] = L"SOFTWARE\\Ricardo\\KompLogon";
// Fixed optional entropy for the DPAPI blob; set-secret.ps1 uses the same bytes.
constexpr char kSecretEntropy[] = "ricardo-komp-logon-v1";
constexpr wchar_t kSecretPath[] = L"C:\\ProgramData\\Ricardo\\KompLogon\\secret.bin";

struct Config {
    std::string service_url;  // normalized, no trailing slash
    std::string machine;
    std::string user;
    std::string domain;
    DWORD fail_count = 0;
    bool disabled = false;
};

enum class ConfigState {
    Ok,       // fully usable
    Disabled, // Disabled=1: do nothing at all
    Invalid,  // missing/invalid values: log and stay closed
};

// Reads and validates the registry configuration.
ConfigState LoadConfig(Config* config, std::string* error);

// Reads, DPAPI-decrypts and parses the secret file.
bool LoadSecret(wakegrant::MachineSecret* secret, std::string* error);

// Overwrites a secret before it goes out of scope.
void ClearSecret(wakegrant::MachineSecret* secret);

// Increments FailCount and sets Disabled=1 at 3. Returns the stored values.
void RecordFailedLogon(DWORD* fail_count, bool* disabled);

}  // namespace komp
}  // namespace ricardo
