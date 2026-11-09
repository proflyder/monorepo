//! The single auto-logon tile.
//!
//! It never references the provider object, only the shared [`Session`].
//! `GetSerialization` is the only place that decrypts the DPAPI secret; it
//! packs `KERB_INTERACTIVE_UNLOCK_LOGON` for `.\<user>`, optionally runs
//! `CredProtectW` and wipes the password, protected password and key on every
//! path.

/// Field ids, in descriptor order.
pub const FIELD_LARGE_TEXT: u32 = 0;
pub const FIELD_SMALL_TEXT: u32 = 1;
/// Number of fields exposed to LogonUI.
pub const FIELD_COUNT: u32 = 2;

/// Tile title, as UTF-8 (tests assert the UTF-16 constants match).
pub const TILE_TITLE: &str = "Home";
pub const TILE_SUBTITLE: &str = "Автовход";

/// `L"Home"`, NUL terminated.
pub const TILE_TITLE_UTF16: &[u16] = &[0x0048, 0x006f, 0x006d, 0x0065, 0x0000];
/// `L"Автовход"`, NUL terminated. Universal-style code units so the source
/// encoding cannot change the UI text.
pub const TILE_SUBTITLE_UTF16: &[u16] = &[
    0x0410, 0x0432, 0x0442, 0x043e, 0x0432, 0x0445, 0x043e, 0x0434, 0x0000,
];

/// `0x<lowercase hex>` of the NTSTATUS, as logged by `ReportResult`.
pub fn hex_status(status: i32) -> String {
    format!("0x{:x}", status as u32)
}

#[cfg(windows)]
pub use windows_impl::{Credential, credential_field_descriptor_at};

#[cfg(windows)]
mod windows_impl {
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::Ordering;
    use std::sync::{Arc, Mutex};

    use crate::bindings::Windows::Win32::Foundation::{
        E_FAIL, E_INVALIDARG, E_NOTIMPL, E_OUTOFMEMORY, E_UNEXPECTED, ERROR_INSUFFICIENT_BUFFER,
        HANDLE, NTSTATUS, S_FALSE,
    };
    use crate::bindings::Windows::Win32::Security::Authentication::Identity::{
        LSA_STRING, LsaConnectUntrusted, LsaDeregisterLogonProcess, LsaLookupAuthenticationPackage,
    };
    use crate::bindings::Windows::Win32::Security::Credentials::{
        CRED_PROTECTION_TYPE, CredIsProtectedW, CredProtectW, CredUnprotected,
    };
    use crate::bindings::Windows::Win32::System::Com::{CoTaskMemAlloc, CoTaskMemFree};
    use crate::bindings::Windows::Win32::System::SystemInformation::GetTickCount64;
    use crate::bindings::Windows::Win32::UI::Shell::CREDENTIAL_PROVIDER_FIELD_TYPE;
    use crate::bindings::Windows::Win32::UI::Shell::{
        CPFIS_NONE, CPFS_DISPLAY_IN_BOTH, CPFS_DISPLAY_IN_SELECTED_TILE, CPFT_LARGE_TEXT,
        CPFT_SMALL_TEXT, CPGSR_NO_CREDENTIAL_NOT_FINISHED, CPGSR_RETURN_CREDENTIAL_FINISHED,
        CPSI_NONE, CPUS_CREDUI, CPUS_LOGON, CPUS_UNLOCK_WORKSTATION,
        CREDENTIAL_PROVIDER_CREDENTIAL_SERIALIZATION, CREDENTIAL_PROVIDER_FIELD_DESCRIPTOR,
        CREDENTIAL_PROVIDER_FIELD_INTERACTIVE_STATE, CREDENTIAL_PROVIDER_FIELD_STATE,
        CREDENTIAL_PROVIDER_GET_SERIALIZATION_RESPONSE, CREDENTIAL_PROVIDER_STATUS_ICON,
        CREDENTIAL_PROVIDER_USAGE_SCENARIO, ICredentialProviderCredential_Impl,
        ICredentialProviderCredential2, ICredentialProviderCredential2_Impl,
        ICredentialProviderCredentialEvents, SHStrDupW,
    };
    use windows_core::{BOOL, GUID, HRESULT, PCWSTR, PWSTR, Ref, Result as WinResult, implement};

    use zeroize::Zeroize;

    use crate::config::{load_config, load_secret};
    use crate::ffi_guard::guard;
    use crate::kerb::{self, KERB_INTERACTIVE_LOGON, KERB_WORKSTATION_UNLOCK_LOGON};
    use crate::log::log_event;
    use crate::provider::{
        PROVIDER_CLSID, Session, cancel_session, configured_user_sid, consume_arming,
        notify_session_changed, session_note_failed_logon, session_scenario,
    };
    use crate::sync::lock;
    use crate::wide::{WideNul, widen};

    const CPGSR: CREDENTIAL_PROVIDER_GET_SERIALIZATION_RESPONSE = CPGSR_NO_CREDENTIAL_NOT_FINISHED;

    /// Wipes a wide buffer on drop, including on early returns.
    struct WipedWide(Vec<u16>);

    impl Drop for WipedWide {
        fn drop(&mut self) {
            self.0.zeroize();
        }
    }

    impl WipedWide {
        /// `PCWSTR` over the buffer; callers must have made it NUL-terminated.
        fn as_pcwstr(&self) -> PCWSTR {
            PCWSTR(self.0.as_ptr())
        }
    }

    impl std::ops::Deref for WipedWide {
        type Target = [u16];
        fn deref(&self) -> &[u16] {
            &self.0
        }
    }

    fn message_type(cpus: CREDENTIAL_PROVIDER_USAGE_SCENARIO) -> Option<i32> {
        if cpus == CPUS_UNLOCK_WORKSTATION {
            Some(KERB_WORKSTATION_UNLOCK_LOGON)
        } else if cpus == CPUS_LOGON {
            Some(KERB_INTERACTIVE_LOGON)
        } else {
            None
        }
    }

    fn field_descriptor(index: u32) -> Option<(u32, CREDENTIAL_PROVIDER_FIELD_TYPE, PCWSTR)> {
        match index {
            0 => Some((0, CPFT_LARGE_TEXT, PCWSTR(super::TILE_TITLE_UTF16.as_ptr()))),
            1 => Some((
                1,
                CPFT_SMALL_TEXT,
                PCWSTR(super::TILE_SUBTITLE_UTF16.as_ptr()),
            )),
            _ => None,
        }
    }

    /// CoTaskMem copy of one field descriptor (LogonUI owns it).
    pub fn credential_field_descriptor_at(
        index: u32,
    ) -> WinResult<*mut CREDENTIAL_PROVIDER_FIELD_DESCRIPTOR> {
        let Some((field_id, field_type, label)) = field_descriptor(index) else {
            return Err(E_INVALIDARG.into());
        };
        // SAFETY: CoTaskMemAlloc returns a suitably aligned block for any type.
        let copy = unsafe { CoTaskMemAlloc(size_of::<CREDENTIAL_PROVIDER_FIELD_DESCRIPTOR>()) }
            as *mut CREDENTIAL_PROVIDER_FIELD_DESCRIPTOR;
        if copy.is_null() {
            return Err(E_OUTOFMEMORY.into());
        }
        // SAFETY: `copy` is a fresh allocation of the right size.
        unsafe {
            (*copy).dwFieldID = field_id;
            (*copy).cpft = field_type;
            (*copy).pszLabel = PWSTR::null();
            (*copy).guidFieldType = GUID::default(); // GUID_NULL
        }
        // SAFETY: `label` points at a static NUL-terminated wide string.
        match unsafe { SHStrDupW(label) } {
            Ok(text) => {
                // SAFETY: `copy` is still uniquely owned here.
                unsafe { (*copy).pszLabel = text };
                Ok(copy)
            }
            Err(error) => {
                // SAFETY: `copy` is uniquely owned and has no label yet.
                unsafe { CoTaskMemFree(Some(copy.cast())) };
                Err(error)
            }
        }
    }

    /// `CredProtect` for logon, plain copy already-protected or CREDUI passwords.
    fn protect_if_necessary_and_copy_password(
        password: &[u16],
        cpus: CREDENTIAL_PROVIDER_USAGE_SCENARIO,
    ) -> Result<WipedWide, HRESULT> {
        if password.is_empty() {
            return Ok(WipedWide(vec![0]));
        }

        // CredProtect takes the buffer together with its terminator, like the
        // C++ implementation (`wcslen(copy) + 1`).
        let copy = WipedWide(WideNul::from_wide(password).into_vec());

        let mut protection = CRED_PROTECTION_TYPE(0);
        // SAFETY: `copy` is NUL-terminated by construction.
        let already_protected = unsafe { CredIsProtectedW(copy.as_pcwstr(), &raw mut protection) }
            .is_ok()
            && protection != CredUnprotected;

        if cpus == CPUS_CREDUI || already_protected {
            return Ok(WipedWide(copy.0.clone()));
        }

        // First call determines the output length, so it is expected to fail.
        let mut capacity = 0u32;
        // SAFETY: a null output buffer with `capacity == 0` asks for the size.
        let first = unsafe { CredProtectW(false, &copy.0, PWSTR::null(), &raw mut capacity, None) };
        match first {
            Err(error)
                if error.code() == HRESULT::from_win32(ERROR_INSUFFICIENT_BUFFER.0)
                    && capacity > 0 =>
            {
                let mut protected = WipedWide(vec![0u16; capacity as usize]);
                // SAFETY: `protected` has exactly `capacity` units.
                match unsafe {
                    CredProtectW(
                        false,
                        &copy.0,
                        PWSTR(protected.0.as_mut_ptr()),
                        &raw mut capacity,
                        None,
                    )
                } {
                    Ok(()) => {
                        protected.0.truncate(capacity as usize);
                        Ok(protected)
                    }
                    Err(error) => Err(error.code()),
                }
            }
            Err(error) => Err(error.code()),
            Ok(()) => Err(E_UNEXPECTED),
        }
    }

    /// Looks up the "negotiate" authentication package id.
    fn retrieve_negotiate_auth_package() -> Result<u32, HRESULT> {
        // Same as NEGOSSP_NAME_A: "Negotiate" with the terminating NUL, and
        // MaximumLength one past the length.
        const NAME: &[u8] = b"Negotiate\0";
        let mut lsa = HANDLE::default();
        // SAFETY: `lsa` is a valid out-parameter.
        let status = unsafe { LsaConnectUntrusted(&raw mut lsa) };
        if HRESULT::from_nt(status.0).is_err() {
            return Err(HRESULT::from_nt(status.0));
        }

        let package_name = LSA_STRING {
            Length: (NAME.len() - 1) as u16,
            MaximumLength: NAME.len() as u16,
            Buffer: windows_core::PSTR(NAME.as_ptr().cast_mut()),
        };
        let mut package = 0u32;
        // SAFETY: the name struct and the out-parameter are valid.
        let status =
            unsafe { LsaLookupAuthenticationPackage(lsa, &package_name, &raw mut package) };
        // SAFETY: `lsa` was opened above and is closed exactly once.
        unsafe {
            let _ = LsaDeregisterLogonProcess(lsa);
        }
        if HRESULT::from_nt(status.0).is_err() {
            return Err(HRESULT::from_nt(status.0));
        }
        Ok(package)
    }

    /// Builds the packed credential. `Err` is a log-safe `HRESULT`.
    fn serialize(cpus: CREDENTIAL_PROVIDER_USAGE_SCENARIO) -> Result<(Vec<u8>, u32), HRESULT> {
        let config = match load_config() {
            Ok(config) => config,
            Err(error) => {
                log_event("serialization_config", &format!("error={}", error.detail()));
                return Err(E_FAIL);
            }
        };

        let secret = match load_secret() {
            Ok(secret) => secret,
            Err(detail) => {
                log_event("serialization_secret", &format!("error={detail}"));
                return Err(E_FAIL);
            }
        };

        let domain = WipedWide(widen(&config.domain));
        let user = WipedWide(widen(&config.user));
        let password = WipedWide(widen(secret.password()));
        drop(secret); // `MachineSecret` zeroizes the key and password on drop.

        if domain.is_empty() || user.is_empty() || password.is_empty() {
            // This branch leaves `hr` at E_FAIL and still logs it.
            log_event("serialization_secret", "error=protect_failed");
            return Err(E_FAIL);
        }

        let protected = match protect_if_necessary_and_copy_password(&password, cpus) {
            Ok(protected) => protected,
            Err(error) => {
                log_event("serialization_secret", "error=protect_failed");
                return Err(error);
            }
        };
        let protected_len = protected
            .iter()
            .position(|&unit| unit == 0)
            .unwrap_or(protected.len());

        let Some(kind) = message_type(cpus) else {
            return Err(E_FAIL);
        };
        let Some(mut packed) = kerb::pack(&domain, &user, &protected[..protected_len], kind) else {
            return Err(E_FAIL);
        };
        let package = match retrieve_negotiate_auth_package() {
            Ok(package) => package,
            Err(error) => {
                packed.zeroize();
                return Err(error);
            }
        };
        Ok((packed, package))
    }

    /// Optional COM methods the tile does not expose.
    fn not_implemented<T>() -> WinResult<T> {
        guard(|| Err(E_NOTIMPL.into()), || Err(E_FAIL.into()))
    }

    /// The single auto-logon tile.
    #[implement(ICredentialProviderCredential2)]
    pub struct Credential {
        session: Arc<Session>,
        events: Mutex<Option<ICredentialProviderCredentialEvents>>,
        selected: AtomicBool,
    }

    impl Credential {
        pub fn new(session: Arc<Session>) -> Self {
            Self {
                session,
                events: Mutex::new(None),
                selected: AtomicBool::new(false),
            }
        }

        fn lock_events(
            &self,
        ) -> std::sync::MutexGuard<'_, Option<ICredentialProviderCredentialEvents>> {
            lock(&self.events)
        }

        fn set_deselected(&self) -> WinResult<()> {
            if !self.selected.swap(false, Ordering::SeqCst) {
                // LogonUI deselects tiles during re-enumeration; that must not
                // cancel a fresh arming before the automatic submission.
                return Ok(());
            }
            log_event("cancel", "reason=deselected");
            cancel_session(&self.session);
            Ok(())
        }

        fn get_serialization(
            &self,
            pcpgsr: *mut CREDENTIAL_PROVIDER_GET_SERIALIZATION_RESPONSE,
            pcpcs: *mut CREDENTIAL_PROVIDER_CREDENTIAL_SERIALIZATION,
            ppszoptionalstatustext: *mut PWSTR,
            pcpsioptionalstatusicon: *mut CREDENTIAL_PROVIDER_STATUS_ICON,
        ) -> WinResult<()> {
            if pcpgsr.is_null()
                || pcpcs.is_null()
                || ppszoptionalstatustext.is_null()
                || pcpsioptionalstatusicon.is_null()
            {
                return Err(E_INVALIDARG.into());
            }
            // SAFETY: all four out-parameters were checked above.
            unsafe {
                pcpgsr.write(CPGSR);
                ppszoptionalstatustext.write(PWSTR::null());
                pcpsioptionalstatusicon.write(CPSI_NONE);
                pcpcs.write(CREDENTIAL_PROVIDER_CREDENTIAL_SERIALIZATION::default());
            }

            if !consume_arming(&self.session) {
                log_event("serialization", "ok=0 reason=not_armed");
                return Ok(());
            }

            // SAFETY: GetTickCount64 takes no arguments.
            let started = unsafe { GetTickCount64() };
            let result = serialize(session_scenario(&self.session));
            // SAFETY: same as above.
            let elapsed = unsafe { GetTickCount64() }.saturating_sub(started) as u32;
            log_event("serialization", &format!("elapsed_ms={elapsed}"));

            let Ok((mut packed, package)) = result else {
                // Fail closed: no credential, no status text, no dialog.
                // Re-enumerate so LogonUI drops the tile instead of
                // showing a dead one.
                notify_session_changed(&self.session);
                return Ok(());
            };

            // The buffer LogonUI frees must come from CoTaskMemAlloc.
            // SAFETY: CoTaskMemAlloc returns a suitably aligned block.
            let buffer = unsafe { CoTaskMemAlloc(packed.len()) } as *mut u8;
            if buffer.is_null() {
                packed.zeroize();
                notify_session_changed(&self.session);
                return Ok(());
            }
            // SAFETY: `buffer` is a fresh allocation of `packed.len()` bytes.
            unsafe {
                core::ptr::copy_nonoverlapping(packed.as_ptr(), buffer, packed.len());
                (*pcpcs).ulAuthenticationPackage = package;
                (*pcpcs).clsidCredentialProvider = PROVIDER_CLSID;
                (*pcpcs).cbSerialization = packed.len() as u32;
                (*pcpcs).rgbSerialization = buffer;
                pcpgsr.write(CPGSR_RETURN_CREDENTIAL_FINISHED);
            }
            // LogonUI owns the CoTaskMem copy; the local one must not linger in
            // the Rust heap with the protected password inside.
            packed.zeroize();
            Ok(())
        }
    }

    impl ICredentialProviderCredential_Impl for Credential_Impl {
        fn Advise(&self, pcpce: Ref<'_, ICredentialProviderCredentialEvents>) -> WinResult<()> {
            guard(
                || {
                    let Some(events) = pcpce.cloned() else {
                        return Err(E_INVALIDARG.into());
                    };
                    *self.lock_events() = Some(events);
                    Ok(())
                },
                || Err(E_FAIL.into()),
            )
        }

        fn UnAdvise(&self) -> WinResult<()> {
            guard(
                || {
                    *self.lock_events() = None;
                    Ok(())
                },
                || Err(E_FAIL.into()),
            )
        }

        fn SetSelected(&self) -> WinResult<BOOL> {
            guard(
                || {
                    self.selected.store(true, Ordering::SeqCst);
                    Ok(BOOL(0))
                },
                || Ok(BOOL(0)),
            )
        }

        fn SetDeselected(&self) -> WinResult<()> {
            guard(|| self.set_deselected(), || Err(E_FAIL.into()))
        }

        fn GetFieldState(
            &self,
            dwfieldid: u32,
            pcpfs: *mut CREDENTIAL_PROVIDER_FIELD_STATE,
            pcpfis: *mut CREDENTIAL_PROVIDER_FIELD_INTERACTIVE_STATE,
        ) -> WinResult<()> {
            guard(
                || {
                    if pcpfs.is_null() || pcpfis.is_null() || dwfieldid >= super::FIELD_COUNT {
                        return Err(E_INVALIDARG.into());
                    }
                    let (field_state, interactive) = if dwfieldid == 0 {
                        (CPFS_DISPLAY_IN_BOTH, CPFIS_NONE)
                    } else {
                        (CPFS_DISPLAY_IN_SELECTED_TILE, CPFIS_NONE)
                    };
                    // SAFETY: both pointers were checked above.
                    unsafe {
                        pcpfs.write(field_state);
                        pcpfis.write(interactive);
                    }
                    Ok(())
                },
                || Err(E_FAIL.into()),
            )
        }

        fn GetStringValue(&self, dwfieldid: u32) -> WinResult<PWSTR> {
            guard(
                || match dwfieldid {
                    0 => {
                        // SAFETY: static NUL-terminated label.
                        unsafe { SHStrDupW(PCWSTR(super::TILE_TITLE_UTF16.as_ptr())) }
                    }
                    1 => {
                        // SAFETY: static NUL-terminated label.
                        unsafe { SHStrDupW(PCWSTR(super::TILE_SUBTITLE_UTF16.as_ptr())) }
                    }
                    _ => Err(E_INVALIDARG.into()),
                },
                || Err(E_FAIL.into()),
            )
        }

        fn GetBitmapValue(
            &self,
            _dwfieldid: u32,
        ) -> WinResult<crate::bindings::Windows::Win32::Graphics::Gdi::HBITMAP> {
            not_implemented()
        }

        fn GetCheckboxValue(
            &self,
            _dwfieldid: u32,
            _pbchecked: *mut BOOL,
            ppszlabel: *mut PWSTR,
        ) -> WinResult<()> {
            guard(
                || {
                    if !ppszlabel.is_null() {
                        // SAFETY: checked non-null.
                        unsafe { ppszlabel.write(PWSTR::null()) };
                    }
                    Err(E_NOTIMPL.into())
                },
                || Err(E_FAIL.into()),
            )
        }

        fn GetSubmitButtonValue(&self, _dwfieldid: u32) -> WinResult<u32> {
            not_implemented()
        }

        fn GetComboBoxValueCount(
            &self,
            _dwfieldid: u32,
            _pcitems: *mut u32,
            _pdwselecteditem: *mut u32,
        ) -> WinResult<()> {
            not_implemented()
        }

        fn GetComboBoxValueAt(&self, _dwfieldid: u32, _dwitem: u32) -> WinResult<PWSTR> {
            not_implemented()
        }

        fn SetStringValue(&self, _dwfieldid: u32, _psz: &PCWSTR) -> WinResult<()> {
            not_implemented()
        }

        fn SetCheckboxValue(&self, _dwfieldid: u32, _bchecked: BOOL) -> WinResult<()> {
            not_implemented()
        }

        fn SetComboBoxSelectedValue(&self, _dwfieldid: u32, _dwselecteditem: u32) -> WinResult<()> {
            not_implemented()
        }

        fn CommandLinkClicked(&self, _dwfieldid: u32) -> WinResult<()> {
            not_implemented()
        }

        fn GetSerialization(
            &self,
            pcpgsr: *mut CREDENTIAL_PROVIDER_GET_SERIALIZATION_RESPONSE,
            pcpcs: *mut CREDENTIAL_PROVIDER_CREDENTIAL_SERIALIZATION,
            ppszoptionalstatustext: *mut PWSTR,
            pcpsioptionalstatusicon: *mut CREDENTIAL_PROVIDER_STATUS_ICON,
        ) -> WinResult<()> {
            guard(
                || {
                    self.get_serialization(
                        pcpgsr,
                        pcpcs,
                        ppszoptionalstatustext,
                        pcpsioptionalstatusicon,
                    )
                },
                || Err(E_FAIL.into()),
            )
        }

        fn ReportResult(
            &self,
            ntsstatus: NTSTATUS,
            _ntssubstatus: NTSTATUS,
            ppszoptionalstatustext: *mut PWSTR,
            pcpsioptionalstatusicon: *mut CREDENTIAL_PROVIDER_STATUS_ICON,
        ) -> WinResult<()> {
            guard(
                || {
                    if !ppszoptionalstatustext.is_null() {
                        // SAFETY: checked non-null.
                        unsafe { ppszoptionalstatustext.write(PWSTR::null()) };
                    }
                    if !pcpsioptionalstatusicon.is_null() {
                        // SAFETY: checked non-null.
                        unsafe { pcpsioptionalstatusicon.write(CPSI_NONE) };
                    }
                    log_event(
                        "report_result",
                        &format!("status={}", super::hex_status(ntsstatus.0)),
                    );
                    if HRESULT::from_nt(ntsstatus.0).is_err() {
                        session_note_failed_logon(&self.session);
                    } else {
                        self.session.serialized.store(false, Ordering::SeqCst);
                    }
                    Ok(())
                },
                || Err(E_FAIL.into()),
            )
        }
    }

    impl ICredentialProviderCredential2_Impl for Credential_Impl {
        fn GetUserSid(&self) -> WinResult<PWSTR> {
            guard(
                || {
                    let sid = configured_user_sid();
                    if sid.is_empty() {
                        // The credential stays valid, just unbound.
                        return Err(S_FALSE.into());
                    }
                    let wide = WideNul::new(&sid);
                    // SAFETY: `wide` is NUL-terminated by construction.
                    unsafe { SHStrDupW(wide.as_pcwstr()) }
                },
                || Err(E_FAIL.into()),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_texts_are_the_documented_strings() {
        assert_eq!(TILE_TITLE, "Home");
        assert_eq!(TILE_SUBTITLE, "Автовход");
        // The UTF-16 literals used by the COM methods are NUL terminated and
        // encode exactly those strings.
        assert_eq!(TILE_TITLE_UTF16.last(), Some(&0));
        assert_eq!(TILE_SUBTITLE_UTF16.last(), Some(&0));
        assert_eq!(
            String::from_utf16(&TILE_TITLE_UTF16[..TILE_TITLE_UTF16.len() - 1]).unwrap(),
            TILE_TITLE
        );
        assert_eq!(
            String::from_utf16(&TILE_SUBTITLE_UTF16[..TILE_SUBTITLE_UTF16.len() - 1]).unwrap(),
            TILE_SUBTITLE
        );
    }

    #[test]
    fn there_are_exactly_two_fields() {
        assert_eq!(FIELD_COUNT, 2);
        assert_eq!(FIELD_LARGE_TEXT, 0);
        assert_eq!(FIELD_SMALL_TEXT, 1);
    }

    #[test]
    fn status_is_printed_like_the_cxx_helper() {
        assert_eq!(hex_status(0), "0x0");
        assert_eq!(hex_status(0xC000_006D_u32 as i32), "0xc000006d");
        assert_eq!(hex_status(0x4000_0000_u32 as i32), "0x40000000");
        assert_eq!(hex_status(-1), "0xffffffff");
    }
}
