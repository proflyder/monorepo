#include "config.h"

#include <wincrypt.h>

#include <cstring>
#include <vector>

namespace ricardo {
namespace komp {
namespace {

constexpr DWORD kMaxSecretFileBytes = 64 * 1024;
constexpr DWORD kFailureDisableThreshold = 3;

std::string Utf8FromWide(const std::wstring& text) {
    if (text.empty()) return std::string();
    const int size = WideCharToMultiByte(CP_UTF8, 0, text.data(), static_cast<int>(text.size()),
                                         nullptr, 0, nullptr, nullptr);
    if (size <= 0) return std::string();
    std::string out(static_cast<size_t>(size), '\0');
    WideCharToMultiByte(CP_UTF8, 0, text.data(), static_cast<int>(text.size()), out.data(), size,
                        nullptr, nullptr);
    return out;
}

bool ReadRegistryString(HKEY key, const wchar_t* name, std::string* out, bool* found) {
    DWORD type = 0;
    DWORD bytes = 0;
    constexpr DWORD kFlags = RRF_RT_REG_SZ | RRF_SUBKEY_WOW6464KEY;
    LSTATUS status =
        RegGetValueW(key, nullptr, name, kFlags, &type, nullptr, &bytes);
    *found = false;
    if (status == ERROR_FILE_NOT_FOUND) return true;
    if (status != ERROR_SUCCESS) return false;
    if (bytes == 0 || bytes > 8192) return false;

    std::vector<wchar_t> buffer(bytes / sizeof(wchar_t) + 1, L'\0');
    status = RegGetValueW(key, nullptr, name, kFlags, &type, buffer.data(), &bytes);
    if (status != ERROR_SUCCESS) return false;
    buffer[bytes / sizeof(wchar_t)] = L'\0';
    *out = Utf8FromWide(buffer.data());
    *found = true;
    return true;
}

bool ReadRegistryDword(HKEY key, const wchar_t* name, DWORD* out) {
    DWORD value = 0;
    DWORD bytes = sizeof(value);
    const LSTATUS status = RegGetValueW(key, nullptr, name, RRF_RT_REG_DWORD | RRF_SUBKEY_WOW6464KEY,
                                        nullptr, &value, &bytes);
    if (status == ERROR_FILE_NOT_FOUND) {
        *out = 0;
        return true;
    }
    if (status != ERROR_SUCCESS) return false;
    *out = value;
    return true;
}

}  // namespace

ConfigState LoadConfig(Config* config, std::string* error) {
    *config = Config();
    error->clear();

    HKEY key = nullptr;
    const LSTATUS status = RegOpenKeyExW(HKEY_LOCAL_MACHINE, kRegistryKey, 0,
                                         KEY_READ | KEY_WOW64_64KEY, &key);
    if (status != ERROR_SUCCESS) {
        *error = "registry_key_missing";
        return ConfigState::Invalid;
    }

    std::string service_url;
    std::string machine;
    std::string user;
    std::string domain;
    bool found = false;
    bool ok = ReadRegistryString(key, L"ServiceUrl", &service_url, &found) && found;
    if (!ok) {
        *error = "registry_value_invalid:ServiceUrl";
    }
    if (ok) {
        ok = ReadRegistryString(key, L"Machine", &machine, &found) && found;
        if (!ok) *error = "registry_value_invalid:Machine";
    }
    if (ok) {
        ok = ReadRegistryString(key, L"UserName", &user, &found) && found;
        if (!ok) *error = "registry_value_invalid:UserName";
    }
    if (ok) {
        ok = ReadRegistryString(key, L"Domain", &domain, &found) && found;
        if (!ok) *error = "registry_value_invalid:Domain";
    }

    DWORD disabled = 0;
    DWORD fail_count = 0;
    if (ok) {
        ok = ReadRegistryDword(key, L"Disabled", &disabled);
        if (!ok) *error = "registry_value_invalid:Disabled";
    }
    if (ok) {
        ok = ReadRegistryDword(key, L"FailCount", &fail_count);
        if (!ok) *error = "registry_value_invalid:FailCount";
    }
    RegCloseKey(key);
    if (!ok) return ConfigState::Invalid;

    std::string normalized;
    if (!wakegrant::validate_config(service_url, machine, user, domain, &normalized, error)) {
        return ConfigState::Invalid;
    }

    config->service_url = normalized;
    config->machine = machine;
    config->user = user;
    config->domain = domain;
    config->fail_count = fail_count;
    config->disabled = disabled != 0;
    return config->disabled ? ConfigState::Disabled : ConfigState::Ok;
}

bool LoadSecret(wakegrant::MachineSecret* secret, std::string* error) {
    *secret = wakegrant::MachineSecret();
    error->clear();

    HANDLE file = CreateFileW(kSecretPath, GENERIC_READ, FILE_SHARE_READ, nullptr, OPEN_EXISTING,
                              FILE_ATTRIBUTE_NORMAL, nullptr);
    if (file == INVALID_HANDLE_VALUE) {
        *error = "secret_file_missing";
        return false;
    }
    LARGE_INTEGER size{};
    if (!GetFileSizeEx(file, &size) || size.QuadPart <= 0 ||
        size.QuadPart > static_cast<LONGLONG>(kMaxSecretFileBytes)) {
        CloseHandle(file);
        *error = "secret_file_size_invalid";
        return false;
    }
    std::vector<BYTE> blob(static_cast<size_t>(size.QuadPart));
    DWORD read = 0;
    const BOOL read_ok = ReadFile(file, blob.data(), static_cast<DWORD>(blob.size()), &read, nullptr);
    CloseHandle(file);
    if (!read_ok || read != static_cast<DWORD>(blob.size())) {
        *error = "secret_file_unreadable";
        return false;
    }

    DATA_BLOB in{};
    in.pbData = blob.data();
    in.cbData = static_cast<DWORD>(blob.size());
    DATA_BLOB entropy{};
    entropy.pbData = reinterpret_cast<BYTE*>(const_cast<char*>(kSecretEntropy));
    entropy.cbData = static_cast<DWORD>(sizeof(kSecretEntropy) - 1);

    DATA_BLOB out{};
    if (!CryptUnprotectData(&in, nullptr, &entropy, nullptr, nullptr, 0, &out) || out.pbData == nullptr ||
        out.cbData == 0) {
        SecureZeroMemory(blob.data(), blob.size());
        *error = "secret_dpapi_failed";
        return false;
    }

    const bool parsed = wakegrant::parse_secret_json(
        std::string_view(reinterpret_cast<const char*>(out.pbData), out.cbData), secret, error);
    SecureZeroMemory(out.pbData, out.cbData);
    LocalFree(out.pbData);
    SecureZeroMemory(blob.data(), blob.size());
    return parsed;
}

void ClearSecret(wakegrant::MachineSecret* secret) {
    if (secret == nullptr) return;
    if (!secret->password.empty()) {
        SecureZeroMemory(secret->password.data(), secret->password.size());
        secret->password.clear();
    }
    if (!secret->key.empty()) {
        SecureZeroMemory(secret->key.data(), secret->key.size());
        secret->key.clear();
    }
}

void RecordFailedLogon(DWORD* fail_count, bool* disabled) {
    *fail_count = 0;
    *disabled = false;

    HKEY key = nullptr;
    if (RegOpenKeyExW(HKEY_LOCAL_MACHINE, kRegistryKey, 0,
                      KEY_READ | KEY_SET_VALUE | KEY_WOW64_64KEY, &key) != ERROR_SUCCESS) {
        return;
    }

    DWORD count = 0;
    DWORD disabled_value = 0;
    if (!ReadRegistryDword(key, L"FailCount", &count) ||
        !ReadRegistryDword(key, L"Disabled", &disabled_value)) {
        RegCloseKey(key);
        return;
    }
    if (count < 1000) ++count;
    if (count >= kFailureDisableThreshold) disabled_value = 1;

    RegSetValueExW(key, L"FailCount", 0, REG_DWORD, reinterpret_cast<const BYTE*>(&count),
                   sizeof(count));
    RegSetValueExW(key, L"Disabled", 0, REG_DWORD, reinterpret_cast<const BYTE*>(&disabled_value),
                   sizeof(disabled_value));
    RegCloseKey(key);

    *fail_count = count;
    *disabled = disabled_value != 0;
}

}  // namespace komp
}  // namespace ricardo
