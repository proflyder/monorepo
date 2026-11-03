#include "credential.h"

#include <new>

#include "config.h"
#include "helpers.h"
#include "log.h"
#include "provider.h"
#include "provider_guid.h"
#include "protocol/protocol.h"

namespace ricardo {
namespace komp {
namespace {

enum FieldId {
    kFieldLargeText = 0,
    kFieldSmallText = 1,
    kFieldCount = 2,
};

constexpr wchar_t kTileTitle[] = L"Ricardo";
// "Автовход" written with universal character names so the source encoding can
// never change the UI text.
constexpr wchar_t kTileSubtitle[] = L"\u0410\u0432\u0442\u043e\u0432\u0445\u043e\u0434";

const CREDENTIAL_PROVIDER_FIELD_DESCRIPTOR kDescriptors[kFieldCount] = {
    {kFieldLargeText, CPFT_LARGE_TEXT, const_cast<LPWSTR>(kTileTitle), GUID_NULL},
    {kFieldSmallText, CPFT_SMALL_TEXT, const_cast<LPWSTR>(kTileSubtitle), GUID_NULL},
};

struct FieldState {
    CREDENTIAL_PROVIDER_FIELD_STATE cpfs;
    CREDENTIAL_PROVIDER_FIELD_INTERACTIVE_STATE cpfis;
};

const FieldState kFieldStates[kFieldCount] = {
    {CPFS_DISPLAY_IN_BOTH, CPFIS_NONE},
    {CPFS_DISPLAY_IN_SELECTED_TILE, CPFIS_NONE},
};

std::wstring Widen(const std::string& text) {
    if (text.empty()) return std::wstring();
    const int size = MultiByteToWideChar(CP_UTF8, 0, text.data(), static_cast<int>(text.size()),
                                         nullptr, 0);
    if (size <= 0) return std::wstring();
    std::wstring out(static_cast<size_t>(size), L'\0');
    MultiByteToWideChar(CP_UTF8, 0, text.data(), static_cast<int>(text.size()), out.data(), size);
    return out;
}

std::string HexStatus(NTSTATUS status) {
    static const char kDigits[] = "0123456789abcdef";
    unsigned long value = static_cast<unsigned long>(status);
    std::string out = "0x";
    bool started = false;
    for (int shift = 28; shift >= 0; shift -= 4) {
        const unsigned digit = (value >> shift) & 0xF;
        if (digit != 0 || started || shift == 0) {
            started = true;
            out.push_back(kDigits[digit]);
        }
    }
    return out;
}

void WipeWideString(std::wstring* text) {
    if (text->empty()) return;
    SecureZeroMemory(&(*text)[0], text->size() * sizeof(wchar_t));
    text->clear();
}

}  // namespace

DWORD CredentialFieldCount() { return kFieldCount; }

HRESULT CredentialFieldDescriptorAt(DWORD index, CREDENTIAL_PROVIDER_FIELD_DESCRIPTOR** ppcpfd) {
    if (ppcpfd == nullptr) return E_INVALIDARG;
    *ppcpfd = nullptr;
    if (index >= kFieldCount) return E_INVALIDARG;
    return FieldDescriptorCoAllocCopy(kDescriptors[index], ppcpfd);
}

Credential::Credential(const std::shared_ptr<Session>& session) : session_(session) {}

Credential::~Credential() {
    if (events_ != nullptr) {
        events_->Release();
        events_ = nullptr;
    }
}

IFACEMETHODIMP Credential::QueryInterface(REFIID riid, void** ppv) {
    if (ppv == nullptr) return E_INVALIDARG;
    *ppv = nullptr;
    if (IsEqualIID(riid, IID_IUnknown) || IsEqualIID(riid, IID_ICredentialProviderCredential) ||
        IsEqualIID(riid, IID_ICredentialProviderCredential2)) {
        *ppv = static_cast<ICredentialProviderCredential2*>(this);
    } else {
        return E_NOINTERFACE;
    }
    AddRef();
    return S_OK;
}

IFACEMETHODIMP_(ULONG) Credential::AddRef() {
    return static_cast<ULONG>(c_ref_.fetch_add(1) + 1);
}

IFACEMETHODIMP_(ULONG) Credential::Release() {
    const long remaining = c_ref_.fetch_sub(1) - 1;
    if (remaining == 0) delete this;
    return static_cast<ULONG>(remaining);
}

IFACEMETHODIMP Credential::Advise(ICredentialProviderCredentialEvents* pcpce) {
    if (pcpce == nullptr) return E_INVALIDARG;
    if (events_ != nullptr) events_->Release();
    events_ = pcpce;
    events_->AddRef();
    return S_OK;
}

IFACEMETHODIMP Credential::UnAdvise() {
    if (events_ != nullptr) {
        events_->Release();
        events_ = nullptr;
    }
    return S_OK;
}

IFACEMETHODIMP Credential::SetSelected(BOOL* pbAutoLogon) {
    if (pbAutoLogon != nullptr) *pbAutoLogon = FALSE;
    selected_ = true;
    return S_OK;
}

IFACEMETHODIMP Credential::SetDeselected() {
    if (!selected_) {
        // LogonUI deselects tiles during re-enumeration; that must not cancel a
        // fresh arming before the automatic submission.
        return S_OK;
    }
    selected_ = false;
    LogEvent("cancel", "reason=deselected");
    CancelSession(session_);
    return S_OK;
}

IFACEMETHODIMP Credential::GetFieldState(
    DWORD dwFieldID,
    CREDENTIAL_PROVIDER_FIELD_STATE* pcpfs,
    CREDENTIAL_PROVIDER_FIELD_INTERACTIVE_STATE* pcpfis) {
    if (pcpfs == nullptr || pcpfis == nullptr) return E_INVALIDARG;
    if (dwFieldID >= kFieldCount) return E_INVALIDARG;
    *pcpfs = kFieldStates[dwFieldID].cpfs;
    *pcpfis = kFieldStates[dwFieldID].cpfis;
    return S_OK;
}

IFACEMETHODIMP Credential::GetStringValue(DWORD dwFieldID, PWSTR* ppwsz) {
    if (ppwsz == nullptr) return E_INVALIDARG;
    *ppwsz = nullptr;
    switch (dwFieldID) {
        case kFieldLargeText:
            return SHStrDupW(kTileTitle, ppwsz);
        case kFieldSmallText:
            return SHStrDupW(kTileSubtitle, ppwsz);
        default:
            return E_INVALIDARG;
    }
}

IFACEMETHODIMP Credential::GetBitmapValue(DWORD, HBITMAP*) { return E_NOTIMPL; }

IFACEMETHODIMP Credential::SetStringValue(DWORD, PCWSTR) { return E_NOTIMPL; }

IFACEMETHODIMP Credential::GetCheckboxValue(DWORD, BOOL*, PWSTR* ppwszLabel) {
    if (ppwszLabel != nullptr) *ppwszLabel = nullptr;
    return E_NOTIMPL;
}

IFACEMETHODIMP Credential::GetSubmitButtonValue(DWORD, DWORD*) { return E_NOTIMPL; }

IFACEMETHODIMP Credential::GetComboBoxValueCount(DWORD, DWORD*, DWORD*) { return E_NOTIMPL; }

IFACEMETHODIMP Credential::GetComboBoxValueAt(DWORD, DWORD, PWSTR* ppwszItem) {
    if (ppwszItem != nullptr) *ppwszItem = nullptr;
    return E_NOTIMPL;
}

IFACEMETHODIMP Credential::SetCheckboxValue(DWORD, BOOL) { return E_NOTIMPL; }

IFACEMETHODIMP Credential::SetComboBoxSelectedValue(DWORD, DWORD) { return E_NOTIMPL; }

IFACEMETHODIMP Credential::CommandLinkClicked(DWORD) { return E_NOTIMPL; }

IFACEMETHODIMP Credential::GetSerialization(
    CREDENTIAL_PROVIDER_GET_SERIALIZATION_RESPONSE* pcpgsr,
    CREDENTIAL_PROVIDER_CREDENTIAL_SERIALIZATION* pcpcs,
    PWSTR* ppwszOptionalStatusText,
    CREDENTIAL_PROVIDER_STATUS_ICON* pcpsiOptionalStatusIcon) {
    if (pcpgsr == nullptr || pcpcs == nullptr || ppwszOptionalStatusText == nullptr ||
        pcpsiOptionalStatusIcon == nullptr) {
        return E_INVALIDARG;
    }
    *pcpgsr = CPGSR_NO_CREDENTIAL_NOT_FINISHED;
    *ppwszOptionalStatusText = nullptr;
    *pcpsiOptionalStatusIcon = CPSI_NONE;
    ZeroMemory(pcpcs, sizeof(*pcpcs));

    if (!ConsumeArming(session_)) {
        LogEvent("serialization", "ok=0 reason=not_armed");
        return S_OK;
    }

    const ULONGLONG started = GetTickCount64();
    const HRESULT hr = Serialize(SessionScenario(session_), pcpcs);
    const unsigned elapsed = static_cast<unsigned>(GetTickCount64() - started);
    LogEvent("serialization", std::string("elapsed_ms=") + std::to_string(elapsed));

    if (FAILED(hr)) {
        // Fail closed: no credential, no status text, no dialog. Re-enumerate
        // so LogonUI drops the tile instead of showing a dead one.
        CoTaskMemFree(pcpcs->rgbSerialization);
        ZeroMemory(pcpcs, sizeof(*pcpcs));
        NotifySessionChanged(session_);
        return S_OK;
    }
    *pcpgsr = CPGSR_RETURN_CREDENTIAL_FINISHED;
    return S_OK;
}

HRESULT Credential::Serialize(CREDENTIAL_PROVIDER_USAGE_SCENARIO cpus,
                              CREDENTIAL_PROVIDER_CREDENTIAL_SERIALIZATION* pcpcs) {
    Config config;
    std::string error;
    if (LoadConfig(&config, &error) != ConfigState::Ok) {
        LogEvent("serialization_config", "error=" + error);
        return E_FAIL;
    }

    wakegrant::MachineSecret secret;
    if (!LoadSecret(&secret, &error)) {
        LogEvent("serialization_secret", "error=" + error);
        return E_FAIL;
    }

    std::wstring domain = Widen(config.domain);
    std::wstring user = Widen(config.user);
    std::wstring password = Widen(secret.password);
    ClearSecret(&secret);

    HRESULT hr = E_FAIL;
    PWSTR protected_password = nullptr;
    if (!domain.empty() && !user.empty() && !password.empty()) {
        hr = ProtectIfNecessaryAndCopyPassword(password.c_str(), cpus, &protected_password);
    }
    WipeWideString(&password);
    if (FAILED(hr)) {
        LogEvent("serialization_secret", "error=protect_failed");
        return hr;
    }

    KERB_INTERACTIVE_UNLOCK_LOGON logon;
    ZeroMemory(&logon, sizeof(logon));
    hr = KerbInteractiveUnlockLogonInit(domain.data(), user.data(), protected_password, cpus,
                                        &logon);
    if (SUCCEEDED(hr)) {
        hr = KerbInteractiveUnlockLogonPack(logon, &pcpcs->rgbSerialization,
                                            &pcpcs->cbSerialization);
    }
    if (SUCCEEDED(hr)) {
        ULONG auth_package = 0;
        hr = RetrieveNegotiateAuthPackage(&auth_package);
        if (SUCCEEDED(hr)) {
            pcpcs->ulAuthenticationPackage = auth_package;
            pcpcs->clsidCredentialProvider = kProviderClsid;
        }
    }

    if (protected_password != nullptr) {
        SecureZeroMemory(protected_password, (wcslen(protected_password) + 1) * sizeof(wchar_t));
        CoTaskMemFree(protected_password);
    }
    SecureZeroMemory(&logon, sizeof(logon));
    WipeWideString(&user);
    WipeWideString(&domain);
    return hr;
}

IFACEMETHODIMP Credential::ReportResult(
    NTSTATUS ntsStatus,
    NTSTATUS ntsSubstatus,
    PWSTR* ppwszOptionalStatusText,
    CREDENTIAL_PROVIDER_STATUS_ICON* pcpsiOptionalStatusIcon) {
    (void)ntsSubstatus;
    if (ppwszOptionalStatusText != nullptr) *ppwszOptionalStatusText = nullptr;
    if (pcpsiOptionalStatusIcon != nullptr) *pcpsiOptionalStatusIcon = CPSI_NONE;

    LogEvent("report_result", "status=" + HexStatus(ntsStatus));
    if (FAILED(HRESULT_FROM_NT(ntsStatus))) {
        SessionNoteFailedLogon(session_);
    } else {
        session_->serialized.store(false);
    }
    return S_OK;
}

IFACEMETHODIMP Credential::GetUserSid(PWSTR* ppszSid) {
    if (ppszSid == nullptr) return E_INVALIDARG;
    *ppszSid = nullptr;

    const std::string sid = ConfiguredUserSid();
    if (sid.empty()) return S_FALSE;  // credential stays valid, just unbound

    const std::wstring wide = Widen(sid);
    if (wide.empty()) return S_FALSE;
    return SHStrDupW(wide.c_str(), ppszSid);
}

}  // namespace komp
}  // namespace ricardo
