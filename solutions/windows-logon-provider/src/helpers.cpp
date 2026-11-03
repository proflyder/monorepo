// Credential-serialization helpers.
//
// Adapted from Microsoft Windows-classic-samples,
// Samples/CredentialProvider/cpp/helpers.cpp. Original notice:
//
//   THIS CODE AND INFORMATION IS PROVIDED "AS IS" WITHOUT WARRANTY OF
//   ANY KIND, EITHER EXPRESSED OR IMPLIED, INCLUDING BUT NOT LIMITED TO
//   THE IMPLIED WARRANTIES OF MERCHANTABILITY AND/OR FITNESS FOR A
//   PARTICULAR PURPOSE.
//
//   Copyright (c) Microsoft Corporation. All rights reserved.
//
//   Licensed under the MIT License. See THIRD-PARTY-NOTICES.md.

#include "helpers.h"

HRESULT FieldDescriptorCoAllocCopy(const CREDENTIAL_PROVIDER_FIELD_DESCRIPTOR& rcpfd,
                                   CREDENTIAL_PROVIDER_FIELD_DESCRIPTOR** ppcpfd) {
    *ppcpfd = nullptr;
    auto* copy = static_cast<CREDENTIAL_PROVIDER_FIELD_DESCRIPTOR*>(
        CoTaskMemAlloc(sizeof(CREDENTIAL_PROVIDER_FIELD_DESCRIPTOR)));
    if (copy == nullptr) return E_OUTOFMEMORY;

    copy->dwFieldID = rcpfd.dwFieldID;
    copy->cpft = rcpfd.cpft;
    copy->guidFieldType = rcpfd.guidFieldType;

    HRESULT hr = S_OK;
    if (rcpfd.pszLabel != nullptr) {
        hr = SHStrDupW(rcpfd.pszLabel, &copy->pszLabel);
    } else {
        copy->pszLabel = nullptr;
    }
    if (FAILED(hr)) {
        CoTaskMemFree(copy);
        return hr;
    }
    *ppcpfd = copy;
    return S_OK;
}

HRESULT FieldDescriptorCopy(const CREDENTIAL_PROVIDER_FIELD_DESCRIPTOR& rcpfd,
                            CREDENTIAL_PROVIDER_FIELD_DESCRIPTOR* pcpfd) {
    *pcpfd = rcpfd;
    if (rcpfd.pszLabel != nullptr) {
        return SHStrDupW(rcpfd.pszLabel, &pcpfd->pszLabel);
    }
    pcpfd->pszLabel = nullptr;
    return S_OK;
}

HRESULT UnicodeStringInitWithString(PWSTR pwz, UNICODE_STRING* pus) {
    if (pwz == nullptr) return E_INVALIDARG;
    const size_t length = wcslen(pwz);
    USHORT characters = 0;
    HRESULT hr = SizeTToUShort(length, &characters);
    if (FAILED(hr)) return hr;
    USHORT bytes_per_char = 0;
    hr = SizeTToUShort(sizeof(wchar_t), &bytes_per_char);
    if (FAILED(hr)) return hr;
    USHORT byte_length = 0;
    hr = UShortMult(characters, bytes_per_char, &byte_length);
    if (FAILED(hr)) return HRESULT_FROM_WIN32(ERROR_ARITHMETIC_OVERFLOW);
    pus->Length = byte_length;  // deliberately without the NUL terminator
    pus->MaximumLength = byte_length;
    pus->Buffer = pwz;
    return S_OK;
}

namespace {

// Callers of KerbInteractiveUnlockLogonPack guarantee the destination buffer is
// large enough, exactly like the Microsoft sample.
void PackUnicodeStringCopy(const UNICODE_STRING& source, PWSTR buffer, UNICODE_STRING* target) {
    target->Length = source.Length;
    target->MaximumLength = source.Length;
    target->Buffer = buffer;
    CopyMemory(target->Buffer, source.Buffer, target->Length);
}

}  // namespace

HRESULT KerbInteractiveUnlockLogonInit(PWSTR pwzDomain,
                                       PWSTR pwzUsername,
                                       PWSTR pwzPassword,
                                       CREDENTIAL_PROVIDER_USAGE_SCENARIO cpus,
                                       KERB_INTERACTIVE_UNLOCK_LOGON* pkiul) {
    KERB_INTERACTIVE_UNLOCK_LOGON kiul;
    ZeroMemory(&kiul, sizeof(kiul));
    KERB_INTERACTIVE_LOGON* pkil = &kiul.Logon;

    HRESULT hr = UnicodeStringInitWithString(pwzDomain, &pkil->LogonDomainName);
    if (SUCCEEDED(hr)) hr = UnicodeStringInitWithString(pwzUsername, &pkil->UserName);
    if (SUCCEEDED(hr)) hr = UnicodeStringInitWithString(pwzPassword, &pkil->Password);
    if (FAILED(hr)) return hr;

    switch (cpus) {
        case CPUS_UNLOCK_WORKSTATION:
            pkil->MessageType = KerbWorkstationUnlockLogon;
            break;
        case CPUS_LOGON:
            pkil->MessageType = KerbInteractiveLogon;
            break;
        default:
            return E_FAIL;
    }

    // KERB_INTERACTIVE_UNLOCK_LOGON is a flat series of structures.
    CopyMemory(pkiul, &kiul, sizeof(*pkiul));
    return S_OK;
}

HRESULT KerbInteractiveUnlockLogonPack(const KERB_INTERACTIVE_UNLOCK_LOGON& rkiulIn,
                                       BYTE** prgb,
                                       DWORD* pcb) {
    const KERB_INTERACTIVE_LOGON* pkilIn = &rkiulIn.Logon;

    const DWORD cb = static_cast<DWORD>(sizeof(rkiulIn)) + pkilIn->LogonDomainName.Length +
                     pkilIn->UserName.Length + pkilIn->Password.Length;

    auto* pkiulOut = static_cast<KERB_INTERACTIVE_UNLOCK_LOGON*>(CoTaskMemAlloc(cb));
    if (pkiulOut == nullptr) return E_OUTOFMEMORY;

    ZeroMemory(&pkiulOut->LogonId, sizeof(pkiulOut->LogonId));

    BYTE* buffer = reinterpret_cast<BYTE*>(pkiulOut) + sizeof(*pkiulOut);
    KERB_INTERACTIVE_LOGON* pkilOut = &pkiulOut->Logon;
    pkilOut->MessageType = pkilIn->MessageType;

    // The PWSTR members become byte offsets into the packed buffer.
    PackUnicodeStringCopy(pkilIn->LogonDomainName, reinterpret_cast<PWSTR>(buffer),
                          &pkilOut->LogonDomainName);
    pkilOut->LogonDomainName.Buffer =
        reinterpret_cast<PWSTR>(buffer - reinterpret_cast<BYTE*>(pkiulOut));
    buffer += pkilOut->LogonDomainName.Length;

    PackUnicodeStringCopy(pkilIn->UserName, reinterpret_cast<PWSTR>(buffer), &pkilOut->UserName);
    pkilOut->UserName.Buffer = reinterpret_cast<PWSTR>(buffer - reinterpret_cast<BYTE*>(pkiulOut));
    buffer += pkilOut->UserName.Length;

    PackUnicodeStringCopy(pkilIn->Password, reinterpret_cast<PWSTR>(buffer), &pkilOut->Password);
    pkilOut->Password.Buffer = reinterpret_cast<PWSTR>(buffer - reinterpret_cast<BYTE*>(pkiulOut));

    *prgb = reinterpret_cast<BYTE*>(pkiulOut);
    *pcb = cb;
    return S_OK;
}

namespace {

HRESULT LsaStringInit(STRING* destination, PCSTR source) {
    const size_t length = strlen(source);
    USHORT us_length = 0;
    HRESULT hr = SizeTToUShort(length, &us_length);
    if (FAILED(hr)) return hr;
    destination->Buffer = const_cast<PCHAR>(source);
    destination->Length = us_length;
    destination->MaximumLength = static_cast<USHORT>(us_length + 1);
    return S_OK;
}

HRESULT ProtectAndCopyString(PCWSTR pwzToProtect, PWSTR* ppwzProtected) {
    *ppwzProtected = nullptr;

    // CredProtect takes a mutable string.
    PWSTR copy = nullptr;
    HRESULT hr = SHStrDupW(pwzToProtect, &copy);
    if (FAILED(hr)) return hr;

    // First call determines the output length, so it is expected to fail.
    DWORD cch_protected = 0;
    if (!CredProtectW(FALSE, copy, static_cast<DWORD>(wcslen(copy)) + 1, nullptr, &cch_protected,
                      nullptr)) {
        const DWORD error = GetLastError();
        if (error == ERROR_INSUFFICIENT_BUFFER && cch_protected > 0) {
            auto* protected_text =
                static_cast<PWSTR>(CoTaskMemAlloc(cch_protected * sizeof(wchar_t)));
            if (protected_text == nullptr) {
                hr = E_OUTOFMEMORY;
            } else if (CredProtectW(FALSE, copy, static_cast<DWORD>(wcslen(copy)) + 1,
                                    protected_text, &cch_protected, nullptr)) {
                *ppwzProtected = protected_text;
                hr = S_OK;
            } else {
                CoTaskMemFree(protected_text);
                hr = HRESULT_FROM_WIN32(GetLastError());
            }
        } else {
            hr = HRESULT_FROM_WIN32(error);
        }
    } else {
        hr = E_UNEXPECTED;
    }

    SecureZeroMemory(copy, (wcslen(copy) + 1) * sizeof(wchar_t));
    CoTaskMemFree(copy);
    return hr;
}

}  // namespace

HRESULT RetrieveNegotiateAuthPackage(ULONG* pulAuthPackage) {
    HANDLE lsa = nullptr;
    NTSTATUS status = LsaConnectUntrusted(&lsa);
    if (FAILED(HRESULT_FROM_NT(status))) return HRESULT_FROM_NT(status);

    ULONG package = 0;
    LSA_STRING name;
    HRESULT hr = LsaStringInit(&name, NEGOSSP_NAME_A);
    if (SUCCEEDED(hr)) {
        status = LsaLookupAuthenticationPackage(lsa, &name, &package);
        if (FAILED(HRESULT_FROM_NT(status))) {
            hr = HRESULT_FROM_NT(status);
        } else {
            *pulAuthPackage = package;
        }
    }
    LsaDeregisterLogonProcess(lsa);
    return hr;
}

HRESULT ProtectIfNecessaryAndCopyPassword(PCWSTR pwzPassword,
                                          CREDENTIAL_PROVIDER_USAGE_SCENARIO cpus,
                                          PWSTR* ppwzProtectedPassword) {
    *ppwzProtectedPassword = nullptr;

    if (pwzPassword == nullptr || *pwzPassword == L'\0') {
        return SHStrDupW(L"", ppwzProtectedPassword);
    }

    PWSTR copy = nullptr;
    HRESULT hr = SHStrDupW(pwzPassword, &copy);
    if (FAILED(hr)) return hr;

    bool already_protected = false;
    CRED_PROTECTION_TYPE protection_type = CredUnprotected;
    if (CredIsProtectedW(copy, &protection_type) && protection_type != CredUnprotected) {
        already_protected = true;
    }

    if (cpus == CPUS_CREDUI || already_protected) {
        hr = SHStrDupW(copy, ppwzProtectedPassword);
    } else {
        hr = ProtectAndCopyString(copy, ppwzProtectedPassword);
    }

    SecureZeroMemory(copy, (wcslen(copy) + 1) * sizeof(wchar_t));
    CoTaskMemFree(copy);
    return hr;
}
