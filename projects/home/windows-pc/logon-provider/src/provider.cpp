#include "provider.h"

#include <sddl.h>

#include <new>

#include "config.h"
#include "credential.h"
#include "dll.h"
#include "log.h"
#include "provider_guid.h"
#include "worker.h"

namespace ricardo {
namespace komp {
namespace {

std::wstring Widen(const std::string& text) {
    if (text.empty()) return std::wstring();
    const int size = MultiByteToWideChar(CP_UTF8, 0, text.data(), static_cast<int>(text.size()),
                                         nullptr, 0);
    if (size <= 0) return std::wstring();
    std::wstring out(static_cast<size_t>(size), L'\0');
    MultiByteToWideChar(CP_UTF8, 0, text.data(), static_cast<int>(text.size()), out.data(), size);
    return out;
}

std::string Narrow(const wchar_t* text) {
    if (text == nullptr) return std::string();
    const int length = static_cast<int>(wcslen(text));
    if (length == 0) return std::string();
    const int size = WideCharToMultiByte(CP_UTF8, 0, text, length, nullptr, 0, nullptr, nullptr);
    if (size <= 0) return std::string();
    std::string out(static_cast<size_t>(size), '\0');
    WideCharToMultiByte(CP_UTF8, 0, text, length, out.data(), size, nullptr, nullptr);
    return out;
}

}  // namespace

std::atomic<long> g_active_workers{0};

ArmResult ArmSession(const std::shared_ptr<Session>& session, unsigned generation) {
    if (!session) return ArmResult::Cancelled;
    AcquireSRWLockExclusive(&session->lock);
    if (!session->owner_alive || session->cancel_generation.load() != generation) {
        ReleaseSRWLockExclusive(&session->lock);
        return ArmResult::Cancelled;
    }
    if (session->events == nullptr) {
        ReleaseSRWLockExclusive(&session->lock);
        return ArmResult::Unavailable;
    }
    session->armed = true;
    ICredentialProviderEvents* events = session->events;
    events->AddRef();
    const UINT_PTR context = session->context;
    ReleaseSRWLockExclusive(&session->lock);

    events->CredentialsChanged(context);
    events->Release();
    return ArmResult::Armed;
}

void CancelSession(const std::shared_ptr<Session>& session) {
    if (!session) return;
    session->cancel_generation.fetch_add(1);
    AcquireSRWLockExclusive(&session->lock);
    session->armed = false;
    ReleaseSRWLockExclusive(&session->lock);
}

bool SessionIsArmed(const std::shared_ptr<Session>& session) {
    if (!session) return false;
    AcquireSRWLockShared(&session->lock);
    const bool armed = session->owner_alive && session->armed;
    ReleaseSRWLockShared(&session->lock);
    return armed;
}

void NotifySessionChanged(const std::shared_ptr<Session>& session) {
    if (!session) return;
    ICredentialProviderEvents* events = nullptr;
    UINT_PTR context = 0;
    AcquireSRWLockExclusive(&session->lock);
    if (session->owner_alive && session->events != nullptr) {
        events = session->events;
        events->AddRef();
        context = session->context;
    }
    ReleaseSRWLockExclusive(&session->lock);
    if (events == nullptr) return;
    events->CredentialsChanged(context);
    events->Release();
}

bool ConsumeArming(const std::shared_ptr<Session>& session) {
    if (!session) return false;
    bool armed = false;
    AcquireSRWLockExclusive(&session->lock);
    if (session->owner_alive && session->armed) {
        session->armed = false;
        armed = true;
    }
    ReleaseSRWLockExclusive(&session->lock);
    if (!armed) return false;
    // One grant, one attempt: the worker must not arm again.
    session->cancel_generation.fetch_add(1);
    session->serialized.store(true);
    return true;
}

CREDENTIAL_PROVIDER_USAGE_SCENARIO SessionScenario(const std::shared_ptr<Session>& session) {
    if (!session) return CPUS_LOGON;
    AcquireSRWLockShared(&session->lock);
    const CREDENTIAL_PROVIDER_USAGE_SCENARIO cpus = session->cpus;
    ReleaseSRWLockShared(&session->lock);
    return cpus;
}

void SessionNoteFailedLogon(const std::shared_ptr<Session>& session) {
    if (!session) return;
    if (!session->serialized.exchange(false)) {
        LogEvent("report_result", "counted=0 reason=not_our_serialization");
        return;
    }
    DWORD fail_count = 0;
    bool disabled = false;
    RecordFailedLogon(&fail_count, &disabled);
    std::string fields = "counted=1 fail_count=" + std::to_string(fail_count);
    fields += disabled ? " disabled=1" : " disabled=0";
    LogEvent("report_result", fields);
    if (disabled) CancelSession(session);
}

std::string ConfiguredUserSid() {
    Config config;
    std::string error;
    if (LoadConfig(&config, &error) != ConfigState::Ok) return std::string();

    const std::wstring account = Widen(config.user);
    const std::wstring domain = Widen(config.domain);
    if (account.empty() || domain.empty()) return std::string();

    // "." means the local SAM: qualify with the computer name so a domain
    // account with the same name can never be picked instead.
    std::wstring qualified = account;
    if (domain == L".") {
        wchar_t computer_name[MAX_COMPUTERNAME_LENGTH + 1] = {};
        DWORD size = MAX_COMPUTERNAME_LENGTH + 1;
        if (GetComputerNameW(computer_name, &size)) {
            qualified = std::wstring(computer_name, size) + L"\\" + account;
        }
    } else {
        qualified = domain + L"\\" + account;
    }

    BYTE sid_buffer[SECURITY_MAX_SID_SIZE] = {};
    DWORD sid_size = sizeof(sid_buffer);
    wchar_t domain_name[256] = {};
    DWORD domain_size = 256;
    SID_NAME_USE use = SidTypeUnknown;
    if (!LookupAccountNameW(nullptr, qualified.c_str(), sid_buffer, &sid_size, domain_name,
                            &domain_size, &use)) {
        return std::string();
    }
    LPWSTR sid_text = nullptr;
    if (!ConvertSidToStringSidW(reinterpret_cast<PSID>(sid_buffer), &sid_text)) {
        return std::string();
    }
    const std::string result = Narrow(sid_text);
    LocalFree(sid_text);
    return result;
}

Provider::Provider() : session_(std::make_shared<Session>()) {
    session_->load_tick = GetTickCount64();
    DllAddRef();
    LogEvent("provider_load");
}

Provider::~Provider() {
    {
        AcquireSRWLockExclusive(&session_->lock);
        session_->owner_alive = false;
        session_->armed = false;
        if (session_->events != nullptr) {
            session_->events->Release();
            session_->events = nullptr;
        }
        ReleaseSRWLockExclusive(&session_->lock);
    }
    session_->cancel_generation.fetch_add(1);

    Credential* credential = nullptr;
    AcquireSRWLockExclusive(&credential_lock_);
    credential = credential_;
    credential_ = nullptr;
    ReleaseSRWLockExclusive(&credential_lock_);
    // The credential keeps its own Session reference and never calls back into
    // the provider, so releasing it here is safe at any moment.
    if (credential != nullptr) credential->Release();

    if (user_array_ != nullptr) {
        user_array_->Release();
        user_array_ = nullptr;
    }
    LogEvent("provider_unload");
    DllRelease();
}

HRESULT Provider::CreateInstance(REFIID riid, void** ppv) {
    *ppv = nullptr;
    try {
        Provider* provider = new (std::nothrow) Provider();
        if (provider == nullptr) return E_OUTOFMEMORY;
        const HRESULT hr = provider->QueryInterface(riid, ppv);
        provider->Release();
        return hr;
    } catch (...) {
        return E_OUTOFMEMORY;
    }
}

IFACEMETHODIMP Provider::QueryInterface(REFIID riid, void** ppv) {
    if (ppv == nullptr) return E_INVALIDARG;
    *ppv = nullptr;
    if (IsEqualIID(riid, IID_IUnknown) || IsEqualIID(riid, IID_ICredentialProvider)) {
        *ppv = static_cast<ICredentialProvider*>(this);
    } else if (IsEqualIID(riid, IID_ICredentialProviderSetUserArray)) {
        *ppv = static_cast<ICredentialProviderSetUserArray*>(this);
    } else {
        return E_NOINTERFACE;
    }
    AddRef();
    return S_OK;
}

IFACEMETHODIMP_(ULONG) Provider::AddRef() {
    return static_cast<ULONG>(c_ref_.fetch_add(1) + 1);
}

IFACEMETHODIMP_(ULONG) Provider::Release() {
    const long remaining = c_ref_.fetch_sub(1) - 1;
    if (remaining == 0) delete this;
    return static_cast<ULONG>(remaining);
}

HRESULT Provider::SetUsageScenario(CREDENTIAL_PROVIDER_USAGE_SCENARIO cpus, DWORD flags) {
    (void)flags;
    switch (cpus) {
        case CPUS_LOGON:
        case CPUS_UNLOCK_WORKSTATION:
            break;
        default:
            // Every other scenario is deliberately unsupported.
            return E_NOTIMPL;
    }
    // A scenario change invalidates any previous arming: LogonUI is asking for
    // a fresh tile set.
    AcquireSRWLockExclusive(&session_->lock);
    const bool changed = session_->cpus != cpus;
    session_->cpus = cpus;
    ReleaseSRWLockExclusive(&session_->lock);
    if (changed) CancelSession(session_);
    LogEvent("scenario", cpus == CPUS_LOGON ? "cpus=logon" : "cpus=unlock");
    StartWorkerOnce();
    return S_OK;
}

HRESULT Provider::SetSerialization(const CREDENTIAL_PROVIDER_CREDENTIAL_SERIALIZATION*) {
    return E_NOTIMPL;
}

HRESULT Provider::Advise(ICredentialProviderEvents* pcpe, UINT_PTR upAdviseContext) {
    if (pcpe == nullptr) return E_INVALIDARG;
    AcquireSRWLockExclusive(&session_->lock);
    if (session_->events != nullptr) session_->events->Release();
    session_->events = pcpe;
    session_->events->AddRef();
    session_->context = upAdviseContext;
    ReleaseSRWLockExclusive(&session_->lock);
    StartWorkerOnce();
    return S_OK;
}

HRESULT Provider::UnAdvise() {
    LogEvent("cancel", "reason=unadvise");
    {
        AcquireSRWLockExclusive(&session_->lock);
        if (session_->events != nullptr) {
            session_->events->Release();
            session_->events = nullptr;
        }
        session_->context = 0;
        session_->armed = false;
        ReleaseSRWLockExclusive(&session_->lock);
    }
    session_->cancel_generation.fetch_add(1);
    return S_OK;
}

HRESULT Provider::GetFieldDescriptorCount(DWORD* pdwCount) {
    if (pdwCount == nullptr) return E_INVALIDARG;
    *pdwCount = CredentialFieldCount();
    return S_OK;
}

HRESULT Provider::GetFieldDescriptorAt(DWORD dwIndex,
                                      CREDENTIAL_PROVIDER_FIELD_DESCRIPTOR** ppcpfd) {
    return CredentialFieldDescriptorAt(dwIndex, ppcpfd);
}

HRESULT Provider::GetCredentialCount(DWORD* pdwCount, DWORD* pdwDefault,
                                     BOOL* pbAutoLogonWithDefault) {
    if (pdwCount == nullptr || pdwDefault == nullptr || pbAutoLogonWithDefault == nullptr) {
        return E_INVALIDARG;
    }
    *pdwDefault = CREDENTIAL_PROVIDER_NO_DEFAULT;
    *pbAutoLogonWithDefault = FALSE;
    *pdwCount = 0;
    if (!SessionIsArmed(session_)) return S_OK;

    *pdwCount = 1;
    *pdwDefault = 0;
    *pbAutoLogonWithDefault = TRUE;
    return S_OK;
}

HRESULT Provider::GetCredentialAt(DWORD dwIndex, ICredentialProviderCredential** ppcpc) {
    if (ppcpc == nullptr) return E_INVALIDARG;
    *ppcpc = nullptr;
    if (dwIndex != 0 || !SessionIsArmed(session_)) return E_INVALIDARG;

    Credential* credential = nullptr;
    AcquireSRWLockExclusive(&credential_lock_);
    if (credential_ == nullptr) {
        try {
            credential_ = new (std::nothrow) Credential(session_);
        } catch (...) {
            credential_ = nullptr;
        }
    }
    credential = credential_;
    if (credential != nullptr) credential->AddRef();
    ReleaseSRWLockExclusive(&credential_lock_);
    if (credential == nullptr) return E_OUTOFMEMORY;

    const HRESULT hr = credential->QueryInterface(IID_ICredentialProviderCredential,
                                                  reinterpret_cast<void**>(ppcpc));
    credential->Release();
    return hr;
}

HRESULT Provider::SetUserArray(ICredentialProviderUserArray* users) {
    if (users == nullptr) return E_INVALIDARG;
    users->AddRef();
    if (user_array_ != nullptr) user_array_->Release();
    user_array_ = users;
    return S_OK;
}

void Provider::StartWorkerOnce() {
    if (session_->running.exchange(true)) return;
    if (GetTickCount64() - session_->load_tick >= kWorkerBudgetMs) {
        session_->running.store(false);
        LogEvent("worker", "skipped=budget_exhausted");
        return;
    }
    StartWorkerThread(session_);
}

}  // namespace komp
}  // namespace ricardo
