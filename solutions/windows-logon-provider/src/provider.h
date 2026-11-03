// ICredentialProvider implementation for the Ricardo komp auto-logon.
//
// The provider shows no tile until the background worker has claimed a valid
// one-time wakegrant. Every failure path removes the tile again: a normal
// power-button boot behaves exactly like a machine without this provider.

#pragma once

#include <windows.h>

#include <credentialprovider.h>

#include <atomic>
#include <memory>
#include <string>

namespace ricardo {
namespace komp {

class Credential;

// State shared by the provider, its credential and the single background
// worker. The credential and the worker only ever touch this structure, so the
// provider object may be destroyed at any point without leaving a dangling
// reference behind.
struct Session {
    SRWLOCK lock = SRWLOCK_INIT;
    ICredentialProviderEvents* events = nullptr;  // guarded by lock
    UINT_PTR context = 0;                         // guarded by lock
    bool owner_alive = true;                      // guarded by lock
    bool armed = false;                           // guarded by lock
    CREDENTIAL_PROVIDER_USAGE_SCENARIO cpus = CPUS_LOGON;  // guarded by lock
    // Incremented by every cancel; a worker stops as soon as it changes.
    std::atomic<unsigned> cancel_generation{0};
    std::atomic<bool> running{false};
    std::atomic<bool> serialized{false};
    ULONGLONG load_tick = 0;
};

enum class ArmResult {
    Armed,       // tile shown, LogonUI notified
    Cancelled,   // generation cancelled or the provider is gone
    Unavailable, // LogonUI has not advised us yet: retry shortly, never re-claim
};

// Arms the session and notifies LogonUI once.
ArmResult ArmSession(const std::shared_ptr<Session>& session, unsigned generation);

// Drops the tile (arming) and stops the worker.
void CancelSession(const std::shared_ptr<Session>& session);

// True while the tile should be shown.
bool SessionIsArmed(const std::shared_ptr<Session>& session);

// Asks LogonUI to re-enumerate without arming (used after a failed
// serialization so the tile disappears instead of lingering).
void NotifySessionChanged(const std::shared_ptr<Session>& session);

// Consumes the arming for exactly one serialization attempt.
bool ConsumeArming(const std::shared_ptr<Session>& session);

CREDENTIAL_PROVIDER_USAGE_SCENARIO SessionScenario(const std::shared_ptr<Session>& session);

// Counts a failed logon only when our serialization was submitted, and
// disables the provider after the third failure.
void SessionNoteFailedLogon(const std::shared_ptr<Session>& session);

// UTF-8 SID of the configured account, empty when it cannot be resolved.
std::string ConfiguredUserSid();

// Number of live worker threads; DllCanUnloadNow keeps the DLL loaded while
// any of them is still running.
extern std::atomic<long> g_active_workers;

class Provider final : public ICredentialProvider, public ICredentialProviderSetUserArray {
  public:
    static HRESULT CreateInstance(REFIID riid, void** ppv);

    // IUnknown
    IFACEMETHODIMP QueryInterface(REFIID riid, void** ppv) override;
    IFACEMETHODIMP_(ULONG) AddRef() override;
    IFACEMETHODIMP_(ULONG) Release() override;

    // ICredentialProvider
    IFACEMETHODIMP SetUsageScenario(CREDENTIAL_PROVIDER_USAGE_SCENARIO cpus, DWORD flags) override;
    IFACEMETHODIMP SetSerialization(const CREDENTIAL_PROVIDER_CREDENTIAL_SERIALIZATION* pcpcs) override;
    IFACEMETHODIMP Advise(ICredentialProviderEvents* pcpe, UINT_PTR upAdviseContext) override;
    IFACEMETHODIMP UnAdvise() override;
    IFACEMETHODIMP GetFieldDescriptorCount(DWORD* pdwCount) override;
    IFACEMETHODIMP GetFieldDescriptorAt(DWORD dwIndex,
                                        CREDENTIAL_PROVIDER_FIELD_DESCRIPTOR** ppcpfd) override;
    IFACEMETHODIMP GetCredentialCount(DWORD* pdwCount,
                                      DWORD* pdwDefault,
                                      BOOL* pbAutoLogonWithDefault) override;
    IFACEMETHODIMP GetCredentialAt(DWORD dwIndex, ICredentialProviderCredential** ppcpc) override;

    // ICredentialProviderSetUserArray
    IFACEMETHODIMP SetUserArray(ICredentialProviderUserArray* users) override;

  private:
    Provider();
    ~Provider();

    void StartWorkerOnce();

    std::atomic<long> c_ref_{1};
    std::shared_ptr<Session> session_;
    SRWLOCK credential_lock_ = SRWLOCK_INIT;
    Credential* credential_ = nullptr;
    ICredentialProviderUserArray* user_array_ = nullptr;
};

}  // namespace komp
}  // namespace ricardo
