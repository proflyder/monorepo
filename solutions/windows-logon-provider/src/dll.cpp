// Standard COM in-proc server entry points for the credential provider.

#include "dll.h"

#include <unknwn.h>

#include <atomic>
#include <new>

#include "log.h"
#include "provider.h"
#include "provider_guid.h"

namespace ricardo {
namespace komp {

HINSTANCE g_hinst = nullptr;
namespace {
std::atomic<long> g_dll_refs{0};
}
}  // namespace komp
}  // namespace ricardo

using ricardo::komp::g_hinst;
using ricardo::komp::kProviderClsid;

namespace {

class ClassFactory final : public IClassFactory {
  public:
    ClassFactory() = default;

    IFACEMETHODIMP QueryInterface(REFIID riid, void** ppv) override {
        if (ppv == nullptr) return E_INVALIDARG;
        *ppv = nullptr;
        if (IsEqualIID(riid, IID_IUnknown) || IsEqualIID(riid, IID_IClassFactory)) {
            *ppv = static_cast<IClassFactory*>(this);
            AddRef();
            return S_OK;
        }
        return E_NOINTERFACE;
    }

    IFACEMETHODIMP_(ULONG) AddRef() override { return static_cast<ULONG>(c_ref_.fetch_add(1) + 1); }

    IFACEMETHODIMP_(ULONG) Release() override {
        const long remaining = c_ref_.fetch_sub(1) - 1;
        if (remaining == 0) delete this;
        return static_cast<ULONG>(remaining);
    }

    IFACEMETHODIMP CreateInstance(IUnknown* outer, REFIID riid, void** ppv) override {
        if (ppv == nullptr) return E_INVALIDARG;
        *ppv = nullptr;
        if (outer != nullptr) return CLASS_E_NOAGGREGATION;
        return ricardo::komp::Provider::CreateInstance(riid, ppv);
    }

    IFACEMETHODIMP LockServer(BOOL lock) override {
        if (lock) {
            ricardo::komp::DllAddRef();
        } else {
            ricardo::komp::DllRelease();
        }
        return S_OK;
    }

  private:
    ~ClassFactory() = default;
    std::atomic<long> c_ref_{1};
};

}  // namespace

namespace ricardo {
namespace komp {

void DllAddRef() { g_dll_refs.fetch_add(1); }
void DllRelease() { g_dll_refs.fetch_sub(1); }

}  // namespace komp
}  // namespace ricardo

STDAPI DllCanUnloadNow() {
    // A live worker thread means logon code could still run, so the module must
    // stay mapped until the thread exits (at most one poll budget).
    return (ricardo::komp::g_dll_refs.load() > 0 || ricardo::komp::g_active_workers.load() > 0)
               ? S_FALSE
               : S_OK;
}

STDAPI DllGetClassObject(REFCLSID rclsid, REFIID riid, void** ppv) {
    if (ppv == nullptr) return E_INVALIDARG;
    *ppv = nullptr;
    if (!IsEqualCLSID(rclsid, kProviderClsid)) return CLASS_E_CLASSNOTAVAILABLE;

    auto* factory = new (std::nothrow) ClassFactory();
    if (factory == nullptr) return E_OUTOFMEMORY;
    const HRESULT hr = factory->QueryInterface(riid, ppv);
    factory->Release();
    return hr;
}

STDAPI_(BOOL) DllMain(HINSTANCE instance, DWORD reason, void*) {
    if (reason == DLL_PROCESS_ATTACH) {
        g_hinst = instance;
        DisableThreadLibraryCalls(instance);
    }
    return TRUE;
}
