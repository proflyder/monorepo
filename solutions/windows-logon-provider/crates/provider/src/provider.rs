//! `ICredentialProvider` and the state shared with the credential and the
//! worker.
//!
//! No tile appears until the worker has claimed a valid one-time wakegrant;
//! every failure path removes it again. [`Session`] is reference-counted and is
//! the only state the credential and worker touch, so the provider object may
//! be destroyed at any time.

/// Budget from provider load to the last challenge attempt: 90 s.
pub const WORKER_BUDGET_MS: u64 = 90 * 1000;

/// Cold-boot window from `GetTickCount64()`: a grant is considered only within
/// the first 10 minutes after boot.
pub const MAX_UPTIME_MS: u64 = 10 * 60 * 1000;

pub fn within_budget(elapsed_ms: u64) -> bool {
    elapsed_ms < WORKER_BUDGET_MS
}

#[cfg(windows)]
pub use windows_impl::*;

#[cfg(windows)]
mod windows_impl {
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
    use std::sync::{Arc, Mutex, MutexGuard};

    use crate::bindings::Windows::Win32::Foundation::{
        E_FAIL, E_INVALIDARG, E_NOTIMPL, E_OUTOFMEMORY, HLOCAL, LocalFree,
    };
    use crate::bindings::Windows::Win32::Security::Authorization::ConvertSidToStringSidW;
    use crate::bindings::Windows::Win32::Security::{
        LookupAccountNameW, PSID, SECURITY_MAX_SID_SIZE, SID_NAME_USE,
    };
    use crate::bindings::Windows::Win32::System::SystemInformation::GetTickCount64;
    use crate::bindings::Windows::Win32::System::WindowsProgramming::{
        GetComputerNameW, MAX_COMPUTERNAME_LENGTH,
    };
    use crate::bindings::Windows::Win32::UI::Shell::{
        CPUS_LOGON, CPUS_UNLOCK_WORKSTATION, CREDENTIAL_PROVIDER_NO_DEFAULT,
        CREDENTIAL_PROVIDER_USAGE_SCENARIO, ICredentialProvider, ICredentialProvider_Impl,
        ICredentialProviderCredential, ICredentialProviderEvents, ICredentialProviderSetUserArray,
        ICredentialProviderSetUserArray_Impl, ICredentialProviderUserArray,
    };
    use windows_core::{
        BOOL, GUID, HRESULT, Interface, PCWSTR, PWSTR, Ref, Result as WinResult, implement,
    };

    use wakegrant_protocol::Purpose;

    use crate::config::{load_config, record_failed_logon};
    use crate::credential::{Credential, FIELD_COUNT, credential_field_descriptor_at};
    use crate::dll::DllRef;
    use crate::ffi_guard::guard;
    use crate::gate::Mode;
    use crate::log::log_event;
    use crate::sync::lock;
    use crate::wide::{WideNul, widen};
    use crate::worker::start_worker_thread;

    use super::within_budget;

    /// `{9459CCEA-4CCC-4435-89BF-E0E132E0C27F}`. Install/uninstall scripts and
    /// tests pin this value; it must never change.
    pub const PROVIDER_CLSID: GUID = GUID {
        data1: 0x9459_ccea,
        data2: 0x4ccc,
        data3: 0x4435,
        data4: [0x89, 0xbf, 0xe0, 0xe1, 0x32, 0xe0, 0xc2, 0x7f],
    };

    /// Number of live worker threads; `DllCanUnloadNow` keeps the DLL mapped
    /// while any of them is still running.
    pub static ACTIVE_WORKERS: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

    struct SessionState {
        events: Option<ICredentialProviderEvents>,
        context: usize,
        owner_alive: bool,
        armed: bool,
        cpus: CREDENTIAL_PROVIDER_USAGE_SCENARIO,
    }

    /// Sentinel for [`Session::worker_generation`]: no worker owns the session.
    const NO_WORKER: u32 = u32::MAX;

    /// State shared by the provider, its credential and the worker.
    pub struct Session {
        state: Mutex<SessionState>,
        /// Incremented by every cancel; a worker stops as soon as it changes.
        pub cancel_generation: AtomicU32,
        /// Generation of the worker that currently owns the claim loop, or
        /// [`NO_WORKER`]. A new generation may start while a cancelled worker
        /// is still finishing its last bounded request.
        worker_generation: AtomicU32,
        pub serialized: AtomicBool,
        pub load_tick: AtomicU64,
    }

    // SAFETY: the interface pointer in `SessionState` is only reached under
    // the mutex; the other fields are atomics.
    unsafe impl Send for Session {}
    unsafe impl Sync for Session {}

    impl Session {
        pub fn new() -> Arc<Self> {
            // SAFETY: GetTickCount64 takes no arguments.
            let load_tick = unsafe { GetTickCount64() };
            Arc::new(Self {
                state: Mutex::new(SessionState {
                    events: None,
                    context: 0,
                    owner_alive: true,
                    armed: false,
                    cpus: CPUS_LOGON,
                }),
                cancel_generation: AtomicU32::new(0),
                worker_generation: AtomicU32::new(NO_WORKER),
                serialized: AtomicBool::new(false),
                load_tick: AtomicU64::new(load_tick),
            })
        }

        fn lock(&self) -> MutexGuard<'_, SessionState> {
            lock(&self.state)
        }

        /// Milliseconds since the provider was loaded.
        pub fn elapsed_ms(&self) -> u64 {
            // SAFETY: GetTickCount64 takes no arguments.
            let now = unsafe { GetTickCount64() };
            now.saturating_sub(self.load_tick.load(Ordering::Relaxed))
        }

        pub fn cancelled(&self, generation: u32) -> bool {
            self.cancel_generation.load(Ordering::SeqCst) != generation
        }

        pub fn generation(&self) -> u32 {
            self.cancel_generation.load(Ordering::SeqCst)
        }

        /// Claims the claim loop for `generation`. Returns `false` when that
        /// same generation already owns it. A stale, cancelled generation never
        /// blocks a newer one: the old worker cannot arm anything because
        /// [`arm_session`] checks the generation.
        pub fn try_start_worker(&self, generation: u32) -> bool {
            let mut current = self.worker_generation.load(Ordering::SeqCst);
            loop {
                if current == generation {
                    return false;
                }
                match self.worker_generation.compare_exchange(
                    current,
                    generation,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                ) {
                    Ok(_) => return true,
                    Err(actual) => current = actual,
                }
            }
        }

        /// Releases the claim loop, but only if `generation` still owns it; a
        /// newer worker that already took over is left alone.
        pub fn finish_worker(&self, generation: u32) {
            let _ = self.worker_generation.compare_exchange(
                generation,
                NO_WORKER,
                Ordering::SeqCst,
                Ordering::SeqCst,
            );
        }
    }

    /// Outcome of an attempt to arm the session.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ArmResult {
        /// Tile shown, LogonUI notified.
        Armed,
        Cancelled,
        /// LogonUI has not advised us yet: retry shortly, never re-claim.
        Unavailable,
    }

    /// Arms the session and notifies LogonUI once.
    pub fn arm_session(session: &Arc<Session>, generation: u32) -> ArmResult {
        let (events, context) = {
            let mut state = session.lock();
            if !state.owner_alive || session.cancel_generation.load(Ordering::SeqCst) != generation
            {
                return ArmResult::Cancelled;
            }
            let Some(events) = state.events.clone() else {
                return ArmResult::Unavailable;
            };
            state.armed = true;
            (events, state.context)
        };
        // Cloning the interface added a reference; it is released when `events`
        // is dropped, after the callback (same as AddRef/Release).
        let _ = unsafe { events.CredentialsChanged(context) };
        ArmResult::Armed
    }

    /// Drops the tile and stops the worker.
    pub fn cancel_session(session: &Arc<Session>) {
        session.cancel_generation.fetch_add(1, Ordering::SeqCst);
        session.lock().armed = false;
    }

    pub fn session_is_armed(session: &Arc<Session>) -> bool {
        let state = session.lock();
        state.owner_alive && state.armed
    }

    /// Asks LogonUI to re-enumerate without arming (used after a failed
    /// serialization so the tile disappears instead of lingering).
    pub fn notify_session_changed(session: &Arc<Session>) {
        let borrowed = {
            let state = session.lock();
            if state.owner_alive {
                state.events.clone().map(|events| (events, state.context))
            } else {
                None
            }
        };
        if let Some((events, context)) = borrowed {
            let _ = unsafe { events.CredentialsChanged(context) };
        }
    }

    /// Consumes the arming for exactly one serialization attempt.
    pub fn consume_arming(session: &Arc<Session>) -> bool {
        let armed = {
            let mut state = session.lock();
            if state.owner_alive && state.armed {
                state.armed = false;
                true
            } else {
                false
            }
        };
        if !armed {
            return false;
        }
        // One grant, one attempt: the worker must not arm again.
        session.cancel_generation.fetch_add(1, Ordering::SeqCst);
        session.serialized.store(true, Ordering::SeqCst);
        true
    }

    pub fn session_scenario(session: &Arc<Session>) -> CREDENTIAL_PROVIDER_USAGE_SCENARIO {
        session.lock().cpus
    }

    /// Counts a failed logon only when our serialization was submitted, and
    /// disables the provider after the third failure.
    pub fn session_note_failed_logon(session: &Arc<Session>) {
        if !session.serialized.swap(false, Ordering::SeqCst) {
            log_event("report_result", "counted=0 reason=not_our_serialization");
            return;
        }
        let (fail_count, disabled) = record_failed_logon();
        let mut fields = format!("counted=1 fail_count={fail_count}");
        fields.push_str(if disabled {
            " disabled=1"
        } else {
            " disabled=0"
        });
        log_event("report_result", &fields);
        if disabled {
            cancel_session(session);
        }
    }

    /// UTF-8 from a NUL-terminated UTF-16 string.
    ///
    /// # Safety
    ///
    /// `text` must point at a NUL-terminated UTF-16 string that stays valid for
    /// the duration of the call.
    pub unsafe fn narrow(text: *const u16) -> String {
        if text.is_null() {
            return String::new();
        }
        let mut length = 0usize;
        // SAFETY: the caller passes a NUL-terminated wide string.
        while unsafe { *text.add(length) } != 0 {
            length += 1;
        }
        // SAFETY: the first `length` units are valid and not NUL.
        let wide = unsafe { core::slice::from_raw_parts(text, length) };
        String::from_utf16_lossy(wide)
    }

    /// UTF-8 SID of the configured account, empty when it cannot be resolved.
    pub fn configured_user_sid() -> String {
        #[repr(align(8))]
        struct AlignedSid([u8; SECURITY_MAX_SID_SIZE as usize]);

        let Ok(config) = load_config() else {
            return String::new();
        };

        let account = widen(&config.user);
        let domain = widen(&config.domain);
        if account.is_empty() || domain.is_empty() {
            return String::new();
        }

        // "." means the local SAM: qualify with the computer name so a domain
        // account with the same name can never be picked instead.
        let qualified = if domain == widen(".") {
            let mut computer = [0u16; MAX_COMPUTERNAME_LENGTH as usize + 1];
            let mut size = MAX_COMPUTERNAME_LENGTH + 1;
            // SAFETY: `computer` has room for `size` units.
            match unsafe { GetComputerNameW(Some(PWSTR(computer.as_mut_ptr())), &raw mut size) } {
                Ok(()) => {
                    let mut value = computer[..size as usize].to_vec();
                    value.push(u16::from(b'\\'));
                    value.extend_from_slice(&account);
                    value
                }
                Err(_) => account,
            }
        } else {
            let mut value = domain;
            value.push(u16::from(b'\\'));
            value.extend_from_slice(&account);
            value
        };
        // The only place a `PCWSTR` is built from the account name: `WideNul`
        // guarantees the terminator `LookupAccountNameW` reads.
        let qualified = WideNul::from_wide(&qualified);

        let mut sid = AlignedSid([0u8; SECURITY_MAX_SID_SIZE as usize]);
        let mut sid_size = SECURITY_MAX_SID_SIZE;
        let mut domain_name = [0u16; 256];
        let mut domain_size = domain_name.len() as u32;
        let mut use_kind = SID_NAME_USE::default();
        // SAFETY: every buffer is live and correctly sized for the call.
        let lookup = unsafe {
            LookupAccountNameW(
                PCWSTR::null(),
                qualified.as_pcwstr(),
                Some(PSID(sid.0.as_mut_ptr().cast())),
                &raw mut sid_size,
                Some(PWSTR(domain_name.as_mut_ptr())),
                &raw mut domain_size,
                &raw mut use_kind,
            )
        };
        if lookup.is_err() {
            return String::new();
        }

        let mut sid_text = PWSTR::null();
        // SAFETY: `sid` holds a SID returned by LookupAccountNameW.
        if unsafe { ConvertSidToStringSidW(PSID(sid.0.as_mut_ptr().cast()), &raw mut sid_text) }
            .is_err()
        {
            return String::new();
        }
        // SAFETY: ConvertSidToStringSidW produced a NUL-terminated wide string.
        let result = unsafe { narrow(sid_text.0) };
        // SAFETY: ConvertSidToStringSidW allocated the string with LocalAlloc.
        unsafe {
            let _ = LocalFree(Some(HLOCAL(sid_text.0.cast())));
        }
        result
    }

    /// Every optional COM method the tile does not expose.
    fn not_implemented<T>() -> WinResult<T> {
        guard(|| Err(E_NOTIMPL.into()), || Err(E_FAIL.into()))
    }

    /// The COM provider object: `ICredentialProvider` +
    /// `ICredentialProviderSetUserArray`.
    #[implement(ICredentialProvider, ICredentialProviderSetUserArray)]
    pub struct Provider {
        session: Arc<Session>,
        credential: Mutex<Option<ICredentialProviderCredential>>,
        user_array: Mutex<Option<ICredentialProviderUserArray>>,
        // Declared last so it is dropped last: the DLL stays mapped while the
        // provider (or any COM reference to it) is alive.
        _dll_ref: DllRef,
    }

    impl Provider {
        fn new(dll_ref: DllRef) -> Self {
            let provider = Self {
                session: Session::new(),
                credential: Mutex::new(None),
                user_array: Mutex::new(None),
                _dll_ref: dll_ref,
            };
            log_event("provider_load", "");
            provider
        }

        /// `ClassFactory::CreateInstance` body. `ppv` is always cleared first.
        ///
        /// # Safety
        ///
        /// COM calling convention: `ppv` is null or writable, `riid` is null or
        /// points to a valid IID.
        pub unsafe fn create_instance(
            riid: *const GUID,
            ppv: *mut *mut std::ffi::c_void,
        ) -> HRESULT {
            if ppv.is_null() {
                return E_INVALIDARG;
            }
            // SAFETY: `ppv` is writable per the COM contract.
            unsafe { *ppv = std::ptr::null_mut() };

            let dll_ref = DllRef::acquire();
            let provider = Self::new(dll_ref);
            let object: ICredentialProvider = provider.into();
            // SAFETY: the out pointer was validated above.
            unsafe { object.query(riid, ppv) }
        }

        fn start_worker_once(&self) {
            // A verified grant that is waiting for `GetSerialization` needs no
            // second claim; a later `Advise` must not add one.
            if session_is_armed(&self.session) {
                return;
            }
            let cpus = session_scenario(&self.session);
            // Defense in depth: `Advise` and `SetUsageScenario` both get here,
            // so only the two supported scenarios may start a claim.
            let purpose = match cpus {
                CPUS_LOGON => Purpose::Logon,
                CPUS_UNLOCK_WORKSTATION => Purpose::Unlock,
                _ => {
                    log_event("worker", "skipped=unsupported_scenario");
                    return;
                }
            };
            // Windows 11 calls CPUS_LOGON for the lock screen too, so the
            // scenario alone is not proof of a cold boot. A session of the
            // configured user selects unlock; without one, only a fresh boot
            // selects logon. A failed WTS query or an unsupported state is off.
            let mode = crate::gate::evaluate(purpose);
            if let Mode::Off(reason) = mode {
                log_event(
                    "scenario",
                    &format!(
                        "cpus={} mode=off reason={}",
                        purpose.as_str(),
                        reason.label()
                    ),
                );
                return;
            }
            if mode == Mode::Logon && !within_budget(self.session.elapsed_ms()) {
                log_event("worker", "skipped=budget_exhausted");
                return;
            }
            // A worker of the same generation already owns the loop. A
            // cancelled one (older generation) may still be finishing its last
            // bounded request; the new generation does not have to wait.
            let generation = self.session.generation();
            if !self.session.try_start_worker(generation) {
                return;
            }
            log_event(
                "scenario",
                &format!("cpus={} mode={}", purpose.as_str(), mode.label()),
            );
            start_worker_thread(&self.session, mode, generation);
        }

        fn set_usage_scenario(
            &self,
            cpus: CREDENTIAL_PROVIDER_USAGE_SCENARIO,
            _flags: u32,
        ) -> WinResult<()> {
            if cpus != CPUS_LOGON && cpus != CPUS_UNLOCK_WORKSTATION {
                // Every other scenario is deliberately unsupported.
                return Err(E_NOTIMPL.into());
            }
            // A scenario change invalidates any previous arming and stops a
            // worker of the previous mode.
            let changed = {
                let mut state = self.session.lock();
                let changed = state.cpus != cpus;
                state.cpus = cpus;
                changed
            };
            if changed {
                cancel_session(&self.session);
            }
            self.start_worker_once();
            Ok(())
        }

        // `Ref` is the COM ABI's borrowed-interface wrapper and is `Copy`;
        // taking it by reference would deviate from the vtable signature this
        // helper mirrors and from the `windows-core` conventions.
        #[allow(clippy::needless_pass_by_value)]
        fn advise(
            &self,
            pcpe: Ref<'_, ICredentialProviderEvents>,
            context: usize,
        ) -> WinResult<()> {
            let Some(events) = pcpe.cloned() else {
                return Err(E_INVALIDARG.into());
            };
            {
                let mut state = self.session.lock();
                // Assigning releases the previous interface reference.
                state.events = Some(events);
                state.context = context;
            }
            self.start_worker_once();
            Ok(())
        }

        fn unadvise(&self) -> WinResult<()> {
            log_event("cancel", "reason=unadvise");
            {
                let mut state = self.session.lock();
                state.events = None;
                state.context = 0;
                state.armed = false;
            }
            self.session
                .cancel_generation
                .fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn get_credential_count(
            &self,
            pdwcount: *mut u32,
            pdwdefault: *mut u32,
            pbautologonwithdefault: *mut BOOL,
        ) -> WinResult<()> {
            if pdwcount.is_null() || pdwdefault.is_null() || pbautologonwithdefault.is_null() {
                return Err(E_INVALIDARG.into());
            }
            // SAFETY: all three pointers were checked above.
            unsafe {
                pdwcount.write(0);
                pdwdefault.write(CREDENTIAL_PROVIDER_NO_DEFAULT);
                pbautologonwithdefault.write(BOOL(0));
            }
            // Only a verified grant of the current mode can show the tile.
            if !session_is_armed(&self.session) {
                return Ok(());
            }
            // SAFETY: same pointers, still valid.
            unsafe {
                pdwcount.write(1);
                pdwdefault.write(0);
                pbautologonwithdefault.write(BOOL(1));
            }
            Ok(())
        }

        fn get_credential_at(&self, index: u32) -> WinResult<ICredentialProviderCredential> {
            if index != 0 || !session_is_armed(&self.session) {
                return Err(E_INVALIDARG.into());
            }
            let mut slot = lock(&self.credential);
            if slot.is_none() {
                let credential: crate::bindings::Windows::Win32::UI::Shell::ICredentialProviderCredential2 =
                    Credential::new(Arc::clone(&self.session)).into();
                *slot = Some(credential.cast()?);
            }
            slot.as_ref().map_or_else(
                || Err(E_OUTOFMEMORY.into()),
                |credential| Ok(credential.clone()),
            )
        }
    }

    impl Drop for Provider {
        fn drop(&mut self) {
            {
                let mut state = self.session.lock();
                state.owner_alive = false;
                state.armed = false;
                // Dropping the interface releases it.
                state.events = None;
            }
            self.session
                .cancel_generation
                .fetch_add(1, Ordering::SeqCst);

            // The credential keeps its own Session reference and never calls
            // back into the provider, so releasing it here is safe.
            let _ = lock(&self.credential).take();
            let _ = lock(&self.user_array).take();

            log_event("provider_unload", "");
            // `_dll_ref` drops after this function returns.
        }
    }

    impl ICredentialProvider_Impl for Provider_Impl {
        fn SetUsageScenario(
            &self,
            cpus: CREDENTIAL_PROVIDER_USAGE_SCENARIO,
            dwflags: u32,
        ) -> WinResult<()> {
            guard(
                || self.set_usage_scenario(cpus, dwflags),
                || Err(E_FAIL.into()),
            )
        }

        fn SetSerialization(
            &self,
            _pcpcs: *const crate::bindings::Windows::Win32::UI::Shell::CREDENTIAL_PROVIDER_CREDENTIAL_SERIALIZATION,
        ) -> WinResult<()> {
            not_implemented()
        }

        fn Advise(
            &self,
            pcpe: Ref<'_, ICredentialProviderEvents>,
            upadvisecontext: usize,
        ) -> WinResult<()> {
            guard(|| self.advise(pcpe, upadvisecontext), || Err(E_FAIL.into()))
        }

        fn UnAdvise(&self) -> WinResult<()> {
            guard(|| self.unadvise(), || Err(E_FAIL.into()))
        }

        fn GetFieldDescriptorCount(&self) -> WinResult<u32> {
            guard(|| Ok(FIELD_COUNT), || Err(E_FAIL.into()))
        }

        fn GetFieldDescriptorAt(
            &self,
            dwindex: u32,
        ) -> WinResult<
            *mut crate::bindings::Windows::Win32::UI::Shell::CREDENTIAL_PROVIDER_FIELD_DESCRIPTOR,
        > {
            guard(
                || credential_field_descriptor_at(dwindex),
                || Err(E_FAIL.into()),
            )
        }

        fn GetCredentialCount(
            &self,
            pdwcount: *mut u32,
            pdwdefault: *mut u32,
            pbautologonwithdefault: *mut BOOL,
        ) -> WinResult<()> {
            guard(
                || self.get_credential_count(pdwcount, pdwdefault, pbautologonwithdefault),
                || Err(E_FAIL.into()),
            )
        }

        fn GetCredentialAt(&self, dwindex: u32) -> WinResult<ICredentialProviderCredential> {
            guard(|| self.get_credential_at(dwindex), || Err(E_FAIL.into()))
        }
    }

    impl ICredentialProviderSetUserArray_Impl for Provider_Impl {
        fn SetUserArray(&self, users: Ref<'_, ICredentialProviderUserArray>) -> WinResult<()> {
            guard(
                || {
                    let Some(users) = users.cloned() else {
                        return Err(E_INVALIDARG.into());
                    };
                    let mut slot = lock(&self.user_array);
                    *slot = Some(users);
                    drop(slot);
                    Ok(())
                },
                || Err(E_FAIL.into()),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{WORKER_BUDGET_MS, within_budget};

    #[test]
    fn the_budget_is_ninety_seconds_exclusive() {
        assert_eq!(WORKER_BUDGET_MS, 90_000);
        assert!(within_budget(0));
        assert!(within_budget(89_999));
        assert!(!within_budget(90_000));
        assert!(!within_budget(90_001));
    }
}
