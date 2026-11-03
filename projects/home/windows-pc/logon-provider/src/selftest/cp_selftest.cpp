// cp-selftest.exe: loads the provider outside LogonUI.
//
// Tries the registered CLSID first; if the provider is not registered yet it
// loads RicardoKompLogon.dll from the directory of this executable and asks the
// DLL for its class factory, so the build can be smoke-tested before install.
//
// It changes nothing on the machine, but SetUsageScenario starts the single
// poller for at most a moment: run it when no wake grant is pending.

#include <windows.h>

#include <credentialprovider.h>

#include <cstdio>
#include <cstring>

#include "provider_guid.h"

namespace {

void Print(const char* name, HRESULT hr) {
    std::printf("  %-28s 0x%08lx (%s)\n", name, static_cast<unsigned long>(hr),
                SUCCEEDED(hr) ? "ok" : "FAIL");
}

HRESULT ProviderFromDllNextToExecutable(ICredentialProvider** provider) {
    wchar_t path[MAX_PATH] = {};
    if (GetModuleFileNameW(nullptr, path, MAX_PATH) == 0) return HRESULT_FROM_WIN32(GetLastError());
    wchar_t* slash = wcsrchr(path, L'\\');
    if (slash == nullptr) return E_FAIL;
    *(slash + 1) = L'\0';
    if (wcslen(path) + 20 >= MAX_PATH) return E_FAIL;
    wcsncat(path, L"RicardoKompLogon.dll", MAX_PATH - wcslen(path) - 1);

    HMODULE module = LoadLibraryW(path);
    if (module == nullptr) return HRESULT_FROM_WIN32(GetLastError());

    using GetClassObjectProc = HRESULT(WINAPI*)(REFCLSID, REFIID, void**);
    auto get_class_object = reinterpret_cast<GetClassObjectProc>(
        GetProcAddress(module, "DllGetClassObject"));
    if (get_class_object == nullptr) return E_FAIL;

    IClassFactory* factory = nullptr;
    HRESULT hr = get_class_object(ricardo::komp::kProviderClsid, IID_IClassFactory,
                                  reinterpret_cast<void**>(&factory));
    if (FAILED(hr)) {
        std::printf("  DllGetClassObject(CLSID) failed\n");
        return hr;
    }
    hr = factory->CreateInstance(nullptr, IID_ICredentialProvider,
                                 reinterpret_cast<void**>(provider));
    factory->Release();
    return hr;
}

}  // namespace

int main() {
    std::printf("Ricardo komp logon provider self-test\n");

    const HRESULT com = CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED);
    std::printf("  %-28s 0x%08lx\n", "CoInitializeEx", static_cast<unsigned long>(com));

    ICredentialProvider* provider = nullptr;
    HRESULT hr = CoCreateInstance(ricardo::komp::kProviderClsid, nullptr, CLSCTX_INPROC_SERVER,
                                  IID_ICredentialProvider,
                                  reinterpret_cast<void**>(&provider));
    std::printf("  %-28s 0x%08lx (%s)\n", "CoCreateInstance", static_cast<unsigned long>(hr),
                SUCCEEDED(hr) ? "registered" : "falling back to local DLL");

    if (FAILED(hr)) {
        hr = ProviderFromDllNextToExecutable(&provider);
        std::printf("  %-28s 0x%08lx\n", "LoadLibrary+DllGetClassObject",
                    static_cast<unsigned long>(hr));
    }
    if (FAILED(hr) || provider == nullptr) {
        std::printf("FAIL: provider could not be created\n");
        if (SUCCEEDED(com)) CoUninitialize();
        return 1;
    }

    hr = provider->SetUsageScenario(CPUS_LOGON, 0);
    Print("SetUsageScenario(CPUS_LOGON)", hr);

    hr = provider->SetUsageScenario(CPUS_CREDUI, 0);
    std::printf("  %-28s 0x%08lx (%s)\n", "SetUsageScenario(CPUS_CREDUI)",
                static_cast<unsigned long>(hr), hr == E_NOTIMPL ? "E_NOTIMPL as required" : "UNEXPECTED");

    DWORD field_count = 0;
    hr = provider->GetFieldDescriptorCount(&field_count);
    Print("GetFieldDescriptorCount", hr);
    std::printf("  %-28s %lu\n", "field count", static_cast<unsigned long>(field_count));

    DWORD count = 0;
    DWORD default_index = 0;
    BOOL auto_logon = FALSE;
    hr = provider->GetCredentialCount(&count, &default_index, &auto_logon);
    Print("GetCredentialCount", hr);
    std::printf("  %-28s count=%lu auto=%d (0 until a grant is armed)\n",
                "credential count", static_cast<unsigned long>(count),
                static_cast<int>(auto_logon));

    // Stop the poller immediately: the self-test must not consume a grant.
    hr = provider->UnAdvise();
    Print("UnAdvise", hr);

    provider->Release();
    if (SUCCEEDED(com)) CoUninitialize();

    const bool ok = SUCCEEDED(com) && field_count == 2 && count == 0;
    std::printf("%s\n", ok ? "PASS" : "FAIL");
    return ok ? 0 : 1;
}
