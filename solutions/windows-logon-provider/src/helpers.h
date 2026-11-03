// Credential-serialization helpers for the Ricardo komp logon provider.
//
// Adapted from Microsoft Windows-classic-samples,
// Samples/CredentialProvider/cpp/helpers.cpp (MIT). See
// THIRD-PARTY-NOTICES.md in this directory for the full notice. Only the
// functions this provider needs are kept, and they are otherwise unchanged.

#pragma once

#define SECURITY_WIN32
#include <windows.h>

#pragma warning(push)
#pragma warning(disable : 28251)
#include <credentialprovider.h>
#include <ntsecapi.h>
#pragma warning(pop)

#include <intsafe.h>
#include <security.h>
#include <shlwapi.h>
#include <strsafe.h>
#include <wincred.h>

// Copy a field descriptor into CoTaskMemAlloc memory (LogonUI owns it).
HRESULT FieldDescriptorCoAllocCopy(const CREDENTIAL_PROVIDER_FIELD_DESCRIPTOR& rcpfd,
                                   CREDENTIAL_PROVIDER_FIELD_DESCRIPTOR** ppcpfd);

// Copy a field descriptor into ordinary heap memory.
HRESULT FieldDescriptorCopy(const CREDENTIAL_PROVIDER_FIELD_DESCRIPTOR& rcpfd,
                            CREDENTIAL_PROVIDER_FIELD_DESCRIPTOR* pcpfd);

// UNICODE_STRING with weak references to a NUL-terminated wide string.
HRESULT UnicodeStringInitWithString(PWSTR pwz, UNICODE_STRING* pus);

// KERB_INTERACTIVE_UNLOCK_LOGON with weak references to the given strings.
HRESULT KerbInteractiveUnlockLogonInit(PWSTR pwzDomain,
                                       PWSTR pwzUsername,
                                       PWSTR pwzPassword,
                                       CREDENTIAL_PROVIDER_USAGE_SCENARIO cpus,
                                       KERB_INTERACTIVE_UNLOCK_LOGON* pkiul);

// Pack the logon structure into the buffer that LSA consumes.
HRESULT KerbInteractiveUnlockLogonPack(const KERB_INTERACTIVE_UNLOCK_LOGON& rkiulIn,
                                       BYTE** prgb,
                                       DWORD* pcb);

// Look up the "negotiate" authentication package id.
HRESULT RetrieveNegotiateAuthPackage(ULONG* pulAuthPackage);

// CredProtect the password for logon/unlock scenarios, otherwise copy it.
HRESULT ProtectIfNecessaryAndCopyPassword(PCWSTR pwzPassword,
                                          CREDENTIAL_PROVIDER_USAGE_SCENARIO cpus,
                                          PWSTR* ppwzProtectedPassword);
