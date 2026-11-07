//! The single background poller: `POST /v1/challenge` -> `POST /v1/claim`.
//!
//! Two modes share the same exchange:
//!
//! - `logon` — purpose `logon`, 2 s poll, no long-poll, 90 s budget from
//!   provider load;
//! - `unlock` — purpose `unlock`, 25 s server long-poll with a 35 s receive
//!   timeout, no budget: the loop runs until the tile is armed or the session
//!   is cancelled.
//!
//! The exchange itself is portable and host-tested in [`crate::worker_core`];
//! this module owns only the WinHTTP, clock, log and thread adapters. Neither
//! mode blocks a LogonUI thread. Both stop promptly on `UnAdvise`: every
//! request checks the session before and after it, and the unlock claim runs on
//! a short-lived module-pinned thread so the worker does not have to wait for a
//! long-poll answer. Every thread-procedure body, including its cleanup, is
//! wrapped in [`crate::ffi_guard::guard_thread`].

/// Poll interval between rounds in logon mode.
pub const POLL_INTERVAL_MS: u32 = 2000;
/// Back-off for `429`/`5xx` in logon mode, as documented in `README.md`.
pub const BACKOFF_INTERVAL_MS: u32 = 4000;

/// Server long-poll for the unlock claim.
pub const UNLOCK_WAIT_SECONDS: u32 = 25;
/// WinHTTP receive timeout for the unlock claim; must exceed
/// [`UNLOCK_WAIT_SECONDS`] plus network latency.
pub const UNLOCK_RECEIVE_TIMEOUT_MS: i32 = 35_000;
/// First pause after a network error, `429` or `5xx` in unlock mode.
pub const UNLOCK_BACKOFF_START_MS: u32 = 5_000;
/// Back-off cap in unlock mode.
pub const UNLOCK_BACKOFF_MAX_MS: u32 = 30_000;
/// Minimum duration of one unlock round, measured from the challenge request
/// to the claim answer. A healthy server holds the claim for
/// [`UNLOCK_WAIT_SECONDS`], so this only paces a broken or misconfigured
/// server that denies instantly; otherwise the loop would create a claim
/// thread per round without pause. Two seconds matches the logon poll and caps
/// the client at 30 claims/min, inside the server's documented 60/min limit.
pub const UNLOCK_MIN_ROUND_MS: u32 = 2_000;

/// Exponential back-off for the unlock loop: 5, 10, 20, 30, 30, ... seconds.
pub fn unlock_backoff_delay(attempt: u32) -> u32 {
    let shift = attempt.min(3);
    UNLOCK_BACKOFF_START_MS
        .saturating_mul(1 << shift)
        .min(UNLOCK_BACKOFF_MAX_MS)
}

/// Pause before the next unlock round. `retry_ms` is the error back-off, or
/// `None` for a plain denial, where the long-poll itself is the pause. A round
/// shorter than [`UNLOCK_MIN_ROUND_MS`] is padded to the minimum.
pub fn unlock_round_delay(round_ms: u64, retry_ms: Option<u32>) -> u32 {
    let remaining = UNLOCK_MIN_ROUND_MS.saturating_sub(round_ms.min(u64::from(u32::MAX)) as u32);
    retry_ms.map_or(remaining, |delay| delay.max(remaining))
}

/// How a round ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundKind {
    /// The tile is armed; the worker stops.
    Armed,
    Retry,
    Fatal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoundOutcome {
    pub kind: RoundKind,
    pub delay_ms: u32,
    /// True when the round failed (network, `429`, `5xx`, bad answer); the
    /// unlock loop turns this into a growing back-off. A plain denial is not
    /// an error: the long-poll already paced the next round.
    pub error: bool,
}

impl RoundOutcome {
    pub const fn retry(delay_ms: u32) -> Self {
        Self {
            kind: RoundKind::Retry,
            delay_ms,
            error: false,
        }
    }

    pub const fn retry_error(delay_ms: u32) -> Self {
        Self {
            kind: RoundKind::Retry,
            delay_ms,
            error: true,
        }
    }

    pub const fn fatal() -> Self {
        Self {
            kind: RoundKind::Fatal,
            delay_ms: 0,
            error: false,
        }
    }

    pub const fn armed() -> Self {
        Self {
            kind: RoundKind::Armed,
            delay_ms: 0,
            error: false,
        }
    }
}

/// A non-200 status is a back-off only for 429 and 5xx; every other status is
/// fatal (`None`), as documented in `README.md`.
pub fn retry_delay_for_status(status: i32) -> Option<u32> {
    if status == 429 || status >= 500 {
        Some(BACKOFF_INTERVAL_MS)
    } else {
        None
    }
}

pub fn clamp_delay(delay_ms: u32, remaining_ms: u64) -> u32 {
    u64::from(delay_ms).min(remaining_ms) as u32
}

#[cfg(windows)]
pub use windows_impl::start_worker_thread;

#[cfg(windows)]
mod windows_impl {
    use core::ffi::c_void;
    use std::sync::Arc;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    use crate::bindings::Windows::Win32::Foundation::{CloseHandle, FreeLibrary, HMODULE};
    use crate::bindings::Windows::Win32::System::LibraryLoader::{
        FreeLibraryAndExitThread, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GetModuleHandleExW,
    };
    use crate::bindings::Windows::Win32::System::SystemInformation::GetTickCount64;
    use crate::bindings::Windows::Win32::System::Threading::{CreateThread, THREAD_CREATION_FLAGS};
    use windows_core::PCWSTR;

    use crate::config::{
        ConfigError, ServiceEndpoint, load_config, load_secret, parse_service_url,
        redact_service_url,
    };
    use crate::ffi_guard::{guard, guard_thread};
    use crate::gate::Mode;
    use crate::http::{HttpOutcome, post_json_with_receive_timeout};
    use crate::log::log_event;
    use crate::provider::{ACTIVE_WORKERS, ArmResult, Session, arm_session, cancel_session};
    use crate::worker_core::{
        ArmOutcome, Clock, EventLog, LoopContext, LoopEnd, PostRequest, Target, Transport,
        run_exchange,
    };

    struct SystemClock;

    impl Clock for SystemClock {
        fn now_ms(&self) -> u64 {
            // SAFETY: GetTickCount64 takes no arguments.
            unsafe { GetTickCount64() }
        }

        fn sleep_ms(&self, milliseconds: u32) {
            std::thread::sleep(Duration::from_millis(u64::from(milliseconds)));
        }
    }

    struct WindowsLog;

    impl EventLog for WindowsLog {
        fn event(&self, event: &str, fields: &str) {
            log_event(event, fields);
        }
    }

    struct SessionTarget<'a> {
        session: &'a Arc<Session>,
        generation: u32,
    }

    impl Target for SessionTarget<'_> {
        fn cancelled(&self) -> bool {
            self.session.cancelled(self.generation)
        }

        fn arm(&self) -> ArmOutcome {
            match arm_session(self.session, self.generation) {
                ArmResult::Armed => ArmOutcome::Armed,
                ArmResult::Cancelled => ArmOutcome::Cancelled,
                ArmResult::Unavailable => ArmOutcome::Unavailable,
            }
        }
    }

    struct HttpTransport<'a> {
        session: &'a Arc<Session>,
        generation: u32,
    }

    impl Transport for HttpTransport<'_> {
        fn post(&self, request: PostRequest<'_>) -> Option<HttpOutcome> {
            if request.cancellable {
                cancellable_post(
                    self.session,
                    self.generation,
                    request.endpoint,
                    request.path,
                    request.body,
                    request.receive_timeout_ms,
                )
            } else {
                Some(post_json_with_receive_timeout(
                    request.endpoint,
                    request.path,
                    request.body,
                    request.receive_timeout_ms,
                ))
            }
        }
    }

    fn run_worker(session: &Arc<Session>, generation: u32, mode: Mode) {
        let start = if mode == Mode::Unlock {
            format!(
                "phase=start mode=unlock wait_s={}",
                super::UNLOCK_WAIT_SECONDS
            )
        } else {
            format!(
                "phase=start mode=logon budget_ms={}",
                crate::provider::WORKER_BUDGET_MS
            )
        };
        log_event("worker", &start);

        let config = match load_config() {
            Ok(config) => {
                log_event(
                    "config",
                    &format!(
                        "state=ok machine={} service={}",
                        config.machine,
                        redact_service_url(&config.service_url)
                    ),
                );
                config
            }
            Err(ConfigError::Disabled) => {
                log_event("config", "state=disabled");
                return;
            }
            Err(error) => {
                log_event("config", &format!("state=invalid error={}", error.detail()));
                return;
            }
        };

        let secret = match load_secret() {
            Ok(secret) => secret,
            Err(detail) => {
                log_event("secret", &format!("ok=0 error={detail}"));
                return;
            }
        };
        log_event("secret", "ok=1");

        let endpoint = match parse_service_url(&config.service_url) {
            Ok(endpoint) => endpoint,
            Err(error) => {
                log_event("config", &format!("state=invalid error={}", error.detail()));
                return;
            }
        };

        let context = LoopContext {
            config: &config,
            endpoint: &endpoint,
            secret: &secret,
            mode,
            origin_ms: session.load_tick.load(Ordering::Relaxed),
        };
        let target = SessionTarget {
            session,
            generation,
        };
        let transport = HttpTransport {
            session,
            generation,
        };
        let end = run_exchange(&context, &target, &transport, &SystemClock, &WindowsLog);

        drop(secret); // `MachineSecret` zeroizes the key and the password.
        let phase = match end {
            LoopEnd::Armed => "phase=armed",
            LoopEnd::Cancelled => "phase=cancelled",
            LoopEnd::Stopped => "phase=stopped",
        };
        log_event("worker", phase);
    }

    /// Pins this DLL for as long as a thread may execute its code.
    ///
    /// The only exit is [`ModulePin::exit`]. There is deliberately no `Drop`:
    /// a destructor could only run on an unwind that is already an abort, and
    /// releasing the pin then would unmap code that is still executing. On that
    /// impossible path the reference leaks, which is harmless.
    struct ModulePin(HMODULE);

    impl ModulePin {
        fn exit(self) -> ! {
            let Self(module) = self;
            // SAFETY: the pin was taken by `GetModuleHandleExW(FROM_ADDRESS)` in
            // `start_worker_thread`/`cancellable_post`; this releases it and
            // exits the thread, so no code from this module runs afterwards.
            unsafe { FreeLibraryAndExitThread(module, 0) }
        }
    }

    /// Arguments moved into the claim request thread.
    struct ClaimJob {
        endpoint: ServiceEndpoint,
        path: String,
        body: Vec<u8>,
        receive_timeout_ms: i32,
        sender: std::sync::mpsc::Sender<HttpOutcome>,
        module: HMODULE,
    }

    /// Short-lived thread that performs one long-poll claim.
    ///
    /// The module is pinned for exactly this thread's lifetime, so the DLL
    /// cannot unload under a request that outlives the worker. A panic in the
    /// request, the send or the cleanup is contained; the pin and the worker
    /// count are still released exactly once.
    unsafe extern "system" fn claim_thread_proc(parameter: *mut c_void) -> u32 {
        // SAFETY: `parameter` comes from `Box::into_raw` in `cancellable_post`
        // and this thread owns it.
        let ClaimJob {
            endpoint,
            path,
            body,
            receive_timeout_ms,
            sender,
            module,
        } = *unsafe { Box::from_raw(parameter.cast::<ClaimJob>()) };
        let pin = ModulePin(module);

        let result = guard_thread(
            || post_json_with_receive_timeout(&endpoint, &path, &body, receive_timeout_ms),
            || {
                // Bookkeeping for DllCanUnloadNow; the pin above is what
                // actually makes unloading safe.
                ACTIVE_WORKERS.fetch_sub(1, Ordering::SeqCst);
            },
            || {},
        );

        let mut sender = Some(sender);
        let mut result = result;
        let _ = guard_thread(
            move || {
                if let Some(sender) = sender.take() {
                    let outcome = result.take().unwrap_or(HttpOutcome::Transport {
                        error: "panic",
                        elapsed_ms: 0,
                    });
                    // A send to a dropped receiver only discards the result.
                    let _ = sender.send(outcome);
                }
            },
            || {},
            || {},
        );

        // Every Rust object with a `Drop` impl is gone before the module is
        // released.
        let mut leftovers = Some((endpoint, path, body));
        let _ = guard_thread(|| drop(leftovers.take()), || {}, || {});
        pin.exit()
    }

    /// Posts on a separate module-pinned thread and returns `None` as soon as
    /// the session is cancelled.
    ///
    /// This is deliberately not implemented by closing the WinHTTP request
    /// handle from another thread: Microsoft documents that closing a handle
    /// of a *synchronous* request is unsafe and creates a race. Abandoning a
    /// bounded (35 s) request thread is safe — it cannot arm anything and does
    /// not touch the session — and the worker stops within 100 ms.
    fn cancellable_post(
        session: &Arc<Session>,
        generation: u32,
        endpoint: &ServiceEndpoint,
        path: &str,
        body: &[u8],
        receive_timeout_ms: i32,
    ) -> Option<HttpOutcome> {
        // +1 module reference for as long as the request thread may run.
        let mut module = HMODULE::default();
        // SAFETY: `claim_thread_proc` is an address inside this DLL, so
        // GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS resolves to this module.
        let pinned = unsafe {
            GetModuleHandleExW(
                GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
                PCWSTR(claim_thread_proc as *const u16),
                &raw mut module,
            )
        };
        let direct = || post_json_with_receive_timeout(endpoint, path, body, receive_timeout_ms);
        if pinned.is_err() {
            // Degraded but still bounded: cancel is checked right after.
            return Some(direct());
        }

        let (sender, receiver) = std::sync::mpsc::channel();
        let job = Box::new(ClaimJob {
            endpoint: endpoint.clone(),
            path: path.to_owned(),
            body: body.to_vec(),
            receive_timeout_ms,
            sender,
            module,
        });
        let parameter = Box::into_raw(job).cast::<c_void>();
        ACTIVE_WORKERS.fetch_add(1, Ordering::SeqCst);
        // SAFETY: the procedure has the LPTHREAD_START_ROUTINE signature, and
        // on success the new thread takes ownership of `parameter`.
        let thread = unsafe {
            CreateThread(
                None,
                0,
                Some(claim_thread_proc),
                Some(parameter),
                THREAD_CREATION_FLAGS(0),
                None,
            )
        };
        if let Ok(handle) = thread {
            // SAFETY: `handle` is open and owned here; closing it does not
            // stop the thread.
            unsafe {
                let _ = CloseHandle(handle);
            }
        } else {
            ACTIVE_WORKERS.fetch_sub(1, Ordering::SeqCst);
            // SAFETY: `parameter` came from `Box::into_raw` above.
            drop(unsafe { Box::from_raw(parameter.cast::<ClaimJob>()) });
            // SAFETY: balances the successful GetModuleHandleExW above.
            unsafe {
                let _ = FreeLibrary(module);
            }
            return Some(direct());
        }

        loop {
            if session.cancelled(generation) {
                return None;
            }
            match receiver.recv_timeout(Duration::from_millis(100)) {
                Ok(result) => return Some(result),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return None,
            }
        }
    }

    /// Arguments moved into the worker thread.
    ///
    /// The last `Arc<Session>` is owned here, so the session outlives the
    /// thread's use of it; `module` is the pinned DLL handle released with
    /// `FreeLibraryAndExitThread`.
    struct StartArgs {
        session: Arc<Session>,
        module: HMODULE,
        mode: Mode,
        generation: u32,
    }

    /// Worker thread procedure. Owns `parameter` and never returns.
    ///
    /// The whole body — the loop and its cleanup — is contained. A panic
    /// cancels the session (fail closed), `finish_worker` and the worker count
    /// are released exactly once, and the module pin is always released by
    /// `ModulePin::exit`.
    unsafe extern "system" fn worker_thread_proc(parameter: *mut c_void) -> u32 {
        // SAFETY: `parameter` comes from `Box::into_raw(StartArgs)` in
        // `start_worker_thread` and this thread owns it.
        let StartArgs {
            session,
            module,
            mode,
            generation,
        } = *unsafe { Box::from_raw(parameter.cast::<StartArgs>()) };
        let pin = ModulePin(module);

        let _ = guard_thread(
            || run_worker(&session, generation, mode),
            || {
                // Two independent steps: a panic in the first must not skip
                // the worker-count decrement.
                let _ = guard_thread(|| session.finish_worker(generation), || {}, || {});
                let _ = guard_thread(
                    || {
                        ACTIVE_WORKERS.fetch_sub(1, Ordering::SeqCst);
                    },
                    || {},
                    || {},
                );
            },
            || {
                // Existing journal line.
                guard(|| log_event("worker", "phase=exception"), || {});
                cancel_session(&session);
            },
        );

        // The only remaining Rust object is the session reference itself; drop
        // it before the module can disappear. A panic in its destructor is
        // contained as well.
        let mut session = Some(session);
        let _ = guard_thread(|| drop(session.take()), || {}, || {});

        pin.exit()
    }

    /// Creates the single worker thread with a raw `CreateThread`.
    ///
    /// This deliberately does not use `std::thread`. A `std::thread` keeps
    /// running std's thread epilogue (`thread_start`, `Packet`, TLS
    /// destructors) after the closure returns, i.e. after `ACTIVE_WORKERS` has
    /// been decremented. LogonUI could then call `DllCanUnloadNow`, get `S_OK`
    /// and `FreeLibrary` while that epilogue is still executing our code. A raw
    /// thread plus `GetModuleHandleExW(FROM_ADDRESS)` pins the module until the
    /// very last instruction of this module has run.
    pub fn start_worker_thread(session: &Arc<Session>, mode: Mode, generation: u32) {
        // +1 module reference for as long as the thread may execute our code.
        let mut module = HMODULE::default();
        // SAFETY: `worker_thread_proc` is an address inside this DLL, so
        // GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS resolves to this module.
        let pinned = unsafe {
            GetModuleHandleExW(
                GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
                PCWSTR(worker_thread_proc as *const u16),
                &raw mut module,
            )
        };
        if pinned.is_err() {
            session.finish_worker(generation);
            log_event("worker", "phase=start_failed");
            return;
        }

        let args = Box::new(StartArgs {
            session: Arc::clone(session),
            module,
            mode,
            generation,
        });
        let parameter = Box::into_raw(args).cast::<c_void>();
        ACTIVE_WORKERS.fetch_add(1, Ordering::SeqCst);
        // SAFETY: the procedure has the LPTHREAD_START_ROUTINE signature, and
        // on success the new thread takes ownership of `parameter`.
        let thread = unsafe {
            CreateThread(
                None,
                0,
                Some(worker_thread_proc),
                Some(parameter),
                THREAD_CREATION_FLAGS(0),
                None,
            )
        };
        if let Ok(handle) = thread {
            // SAFETY: `handle` is open and owned here; closing it does not
            // stop the thread.
            unsafe {
                let _ = CloseHandle(handle);
            }
        } else {
            ACTIVE_WORKERS.fetch_sub(1, Ordering::SeqCst);
            // The thread never started, so it did not take the argument.
            // SAFETY: `parameter` came from `Box::into_raw` above.
            drop(unsafe { Box::from_raw(parameter.cast::<StartArgs>()) });
            // SAFETY: balances the successful GetModuleHandleExW above.
            unsafe {
                let _ = FreeLibrary(module);
            }
            session.finish_worker(generation);
            log_event("worker", "phase=start_failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limits_and_server_errors_back_off() {
        assert_eq!(retry_delay_for_status(429), Some(BACKOFF_INTERVAL_MS));
        assert_eq!(retry_delay_for_status(500), Some(BACKOFF_INTERVAL_MS));
        assert_eq!(retry_delay_for_status(503), Some(BACKOFF_INTERVAL_MS));
        assert_eq!(retry_delay_for_status(200), None);
        assert_eq!(retry_delay_for_status(400), None);
        assert_eq!(retry_delay_for_status(401), None);
        assert_eq!(retry_delay_for_status(404), None);
        assert_eq!(retry_delay_for_status(0), None);
    }

    #[test]
    fn delays_never_exceed_the_remaining_budget() {
        assert_eq!(clamp_delay(POLL_INTERVAL_MS, 5000), 2000);
        assert_eq!(clamp_delay(POLL_INTERVAL_MS, 1000), 1000);
        assert_eq!(clamp_delay(BACKOFF_INTERVAL_MS, 0), 0);
        assert_eq!(BACKOFF_INTERVAL_MS, 4000);
        assert_eq!(POLL_INTERVAL_MS, 2000);
    }

    #[test]
    fn unlock_backoff_starts_at_five_and_caps_at_thirty_seconds() {
        assert_eq!(unlock_backoff_delay(0), 5000);
        assert_eq!(unlock_backoff_delay(1), 10_000);
        assert_eq!(unlock_backoff_delay(2), 20_000);
        assert_eq!(unlock_backoff_delay(3), 30_000);
        assert_eq!(unlock_backoff_delay(4), 30_000);
        assert_eq!(unlock_backoff_delay(u32::MAX), 30_000);
    }

    #[test]
    fn an_unlock_round_is_padded_to_the_minimum_interval() {
        assert_eq!(UNLOCK_MIN_ROUND_MS, 2000);
        assert_eq!(unlock_round_delay(0, None), 2000);
        assert_eq!(unlock_round_delay(500, None), 1500);
        assert_eq!(unlock_round_delay(1999, None), 1);
        assert_eq!(unlock_round_delay(2000, None), 0);
        // A real long-poll already outlasts the minimum.
        assert_eq!(unlock_round_delay(25_000, None), 0);
        // An error back-off is never shortened by the minimum.
        assert_eq!(unlock_round_delay(0, Some(5000)), 5000);
        assert_eq!(unlock_round_delay(1000, Some(500)), 1000);
    }

    #[test]
    fn the_long_poll_is_bounded_and_shorter_than_the_receive_timeout() {
        assert_eq!(UNLOCK_WAIT_SECONDS, 25);
        assert_eq!(UNLOCK_RECEIVE_TIMEOUT_MS, 35_000);
        assert!(UNLOCK_RECEIVE_TIMEOUT_MS > (UNLOCK_WAIT_SECONDS as i32) * 1000);
    }

    #[test]
    fn outcome_constructors_keep_the_documented_kinds() {
        assert_eq!(RoundOutcome::retry(1).kind, RoundKind::Retry);
        assert_eq!(RoundOutcome::retry(7).delay_ms, 7);
        assert!(!RoundOutcome::retry(7).error);
        assert_eq!(RoundOutcome::retry_error(7).kind, RoundKind::Retry);
        assert!(RoundOutcome::retry_error(7).error);
        assert_eq!(RoundOutcome::fatal().kind, RoundKind::Fatal);
        assert_eq!(RoundOutcome::armed().kind, RoundKind::Armed);
        assert_eq!(RoundOutcome::armed().delay_ms, 0);
        assert!(!RoundOutcome::armed().error);
    }
}
