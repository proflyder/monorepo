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
//! Neither mode blocks a LogonUI thread. Both stop promptly on `UnAdvise`:
//! every request checks [`Session::cancelled`] before and after it, and the
//! unlock claim runs on a short-lived module-pinned thread so the worker does
//! not have to wait for a long-poll answer.

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

/// Exponential back-off for the unlock loop: 5, 10, 20, 30, 30, ... seconds.
pub fn unlock_backoff_delay(attempt: u32) -> u32 {
    let shift = attempt.min(3);
    UNLOCK_BACKOFF_START_MS
        .saturating_mul(1 << shift)
        .min(UNLOCK_BACKOFF_MAX_MS)
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
    use std::time::{Duration, Instant};

    use logon_protocol::{
        ClaimOutcome, MachineSecret, Purpose, ServiceEndpoint, challenge_request_json,
        claim_mac_message_for_purpose, claim_request_json_for_purpose, endpoint_path,
        hmac_sha256_b64, parse_challenge_response, parse_service_url, redact_service_url,
        verify_claim_response_for_purpose,
    };

    use crate::bindings::Windows::Win32::Foundation::{CloseHandle, FreeLibrary, HMODULE};
    use crate::bindings::Windows::Win32::System::LibraryLoader::{
        FreeLibraryAndExitThread, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GetModuleHandleExW,
    };
    use crate::bindings::Windows::Win32::System::Threading::{CreateThread, THREAD_CREATION_FLAGS};
    use windows_core::PCWSTR;

    use crate::config::{Config, ConfigState, load_config, load_secret};
    use crate::ffi_guard::guard;
    use crate::gate::Mode;
    use crate::http::{HttpResult, post_json, post_json_with_receive_timeout};
    use crate::log::log_event;
    use crate::provider::{
        ACTIVE_WORKERS, ArmResult, Session, WORKER_BUDGET_MS, arm_session, within_budget,
    };
    use crate::worker::{
        POLL_INTERVAL_MS, RoundKind, RoundOutcome, UNLOCK_RECEIVE_TIMEOUT_MS, UNLOCK_WAIT_SECONDS,
        clamp_delay, retry_delay_for_status, unlock_backoff_delay,
    };

    fn sleep_interruptible(session: &Arc<Session>, generation: u32, milliseconds: u32) {
        let deadline = Instant::now() + Duration::from_millis(u64::from(milliseconds));
        while Instant::now() < deadline {
            if session.cancelled(generation) {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// Suppresses repeated identical error lines in the unlock loop, which can
    /// run all night. Every error kind is logged again at most every tenth
    /// repeat, and any different error is logged immediately.
    #[derive(Default)]
    struct ErrorDedup {
        last: String,
        repeats: u32,
    }

    impl ErrorDedup {
        /// `key` identifies the error kind; `fields` may contain per-round
        /// timings, so it cannot be the dedup key itself.
        fn log(&mut self, event: &str, key: &str, fields: &str) {
            if self.last == key {
                self.repeats = self.repeats.saturating_add(1);
                if !self.repeats.is_multiple_of(10) {
                    return;
                }
            } else {
                self.last.clear();
                self.last.push_str(key);
                self.repeats = 0;
            }
            log_event(event, fields);
        }
    }

    fn log_error(dedup: &mut Option<ErrorDedup>, event: &str, key: &str, fields: &str) {
        match dedup {
            Some(dedup) => dedup.log(event, key, fields),
            None => log_event(event, fields),
        }
    }

    /// Everything one round needs besides the session and its generation.
    #[derive(Clone, Copy)]
    struct RoundContext<'a> {
        config: &'a Config,
        endpoint: &'a ServiceEndpoint,
        secret: &'a MachineSecret,
        mode: Mode,
    }

    /// One challenge/claim exchange. Any unexpected answer ends the round
    /// without arming; only a verified grant arms the session.
    fn one_round(
        session: &Arc<Session>,
        generation: u32,
        context: &RoundContext<'_>,
        elapsed_ms: u64,
        dedup: &mut Option<ErrorDedup>,
    ) -> RoundOutcome {
        let RoundContext {
            config,
            endpoint,
            secret,
            mode,
        } = *context;
        let purpose = match mode {
            Mode::Unlock => Purpose::Unlock,
            _ => Purpose::Logon,
        };
        let prefix = format!("elapsed_ms={elapsed_ms}");

        if session.cancelled(generation) {
            return RoundOutcome::fatal();
        }
        let challenge_path = endpoint_path(endpoint, "/v1/challenge");
        let challenge = post_json(
            endpoint,
            &challenge_path,
            challenge_request_json(&config.machine).as_bytes(),
        );
        if session.cancelled(generation) {
            return RoundOutcome::fatal();
        }
        if !challenge.ok {
            log_error(
                dedup,
                "http",
                &format!("challenge-transport-{}", challenge.error),
                &format!(
                    "request=challenge ok=0 error={} ms={} {prefix}",
                    challenge.error, challenge.elapsed_ms
                ),
            );
            return RoundOutcome::retry_error(POLL_INTERVAL_MS);
        }
        if challenge.status != 200 {
            log_error(
                dedup,
                "http",
                &format!("challenge-status-{}", challenge.status),
                &format!(
                    "request=challenge ok=1 status={} ms={} {prefix}",
                    challenge.status, challenge.elapsed_ms
                ),
            );
            return match retry_delay_for_status(challenge.status) {
                Some(delay) => RoundOutcome::retry_error(delay),
                None => RoundOutcome::fatal(),
            };
        }

        let Ok(challenge_body) = std::str::from_utf8(&challenge.body) else {
            log_error(
                dedup,
                "challenge",
                "challenge-invalid-utf8",
                &format!("ok=0 error=invalid-utf8 {prefix}"),
            );
            return RoundOutcome::fatal();
        };
        let response = match parse_challenge_response(challenge_body) {
            Ok(response) => response,
            Err(error) => {
                log_error(
                    dedup,
                    "challenge",
                    &format!("challenge-{}", error.detail()),
                    &format!("ok=0 error={} {prefix}", error.detail()),
                );
                return RoundOutcome::fatal();
            }
        };

        let mac = hmac_sha256_b64(
            secret.key(),
            &claim_mac_message_for_purpose(&config.machine, &response.nonce, purpose),
        );
        if mac.is_empty() {
            log_error(
                dedup,
                "claim",
                "claim-hmac-failed",
                &format!("ok=0 error=hmac_failed {prefix}"),
            );
            return RoundOutcome::fatal();
        }

        let wait_seconds = if purpose == Purpose::Unlock {
            Some(UNLOCK_WAIT_SECONDS)
        } else {
            None
        };
        let claim_body = claim_request_json_for_purpose(
            &config.machine,
            &response.nonce,
            &mac,
            purpose,
            wait_seconds,
        );
        let claim_path = endpoint_path(endpoint, "/v1/claim");
        let claim = if purpose == Purpose::Unlock {
            match cancellable_post(
                session,
                generation,
                endpoint,
                &claim_path,
                claim_body.as_bytes(),
                UNLOCK_RECEIVE_TIMEOUT_MS,
            ) {
                Some(result) => result,
                // Cancelled while the long-poll was in flight.
                None => return RoundOutcome::fatal(),
            }
        } else {
            post_json(endpoint, &claim_path, claim_body.as_bytes())
        };
        if session.cancelled(generation) {
            return RoundOutcome::fatal();
        }
        if !claim.ok {
            log_error(
                dedup,
                "http",
                &format!("claim-transport-{}", claim.error),
                &format!(
                    "request=claim ok=0 error={} ms={} {prefix}",
                    claim.error, claim.elapsed_ms
                ),
            );
            return RoundOutcome::retry_error(POLL_INTERVAL_MS);
        }
        if claim.status != 200 {
            log_error(
                dedup,
                "http",
                &format!("claim-status-{}", claim.status),
                &format!(
                    "request=claim ok=1 status={} ms={} {prefix}",
                    claim.status, claim.elapsed_ms
                ),
            );
            return match retry_delay_for_status(claim.status) {
                Some(delay) => RoundOutcome::retry_error(delay),
                None => RoundOutcome::fatal(),
            };
        }

        let result = match std::str::from_utf8(&claim.body) {
            Ok(body) => verify_claim_response_for_purpose(
                body,
                secret.key(),
                &config.machine,
                &response.nonce,
                purpose,
            ),
            Err(_) => {
                log_error(
                    dedup,
                    "grant",
                    "grant-invalid-utf8",
                    &format!(
                        "result=malformed detail=invalid-utf8 ms={} {prefix}",
                        claim.elapsed_ms
                    ),
                );
                return RoundOutcome::retry_error(POLL_INTERVAL_MS);
            }
        };

        match result.outcome {
            ClaimOutcome::Granted => {
                log_event(
                    "grant",
                    &format!(
                        "result=granted purpose={} grant_id={} ms={} {prefix}",
                        purpose.as_str(),
                        result.grant_id,
                        claim.elapsed_ms
                    ),
                );
                // The grant is already consumed, so never claim again: only
                // wait a moment for Advise() and then give up.
                for _ in 0..20 {
                    match arm_session(session, generation) {
                        ArmResult::Armed => {
                            log_event("armed", "ok=1");
                            return RoundOutcome::armed();
                        }
                        ArmResult::Cancelled => {
                            log_event("armed", "ok=0 reason=cancelled");
                            return RoundOutcome::fatal();
                        }
                        ArmResult::Unavailable => {}
                    }
                    if session.cancelled(generation) {
                        log_event("armed", "ok=0 reason=cancelled");
                        return RoundOutcome::fatal();
                    }
                    sleep_interruptible(session, generation, 100);
                }
                log_event("armed", "ok=0 reason=no_events");
                RoundOutcome::fatal()
            }
            ClaimOutcome::Denied => {
                if purpose == Purpose::Logon {
                    log_event(
                        "grant",
                        &format!(
                            "result=denied purpose=logon ms={} {prefix}",
                            claim.elapsed_ms
                        ),
                    );
                }
                // Unlock denials are not logged: the long-poll is the pause,
                // and a lock screen can stay up all night.
                RoundOutcome::retry(if purpose == Purpose::Unlock {
                    0
                } else {
                    POLL_INTERVAL_MS
                })
            }
            ClaimOutcome::InvalidMac | ClaimOutcome::Malformed => {
                let label = if result.outcome == ClaimOutcome::InvalidMac {
                    "invalid_mac"
                } else {
                    "malformed"
                };
                log_error(
                    dedup,
                    "grant",
                    &format!("grant-{label}-{}", result.detail),
                    &format!(
                        "result={label} purpose={} detail={} ms={} {prefix}",
                        purpose.as_str(),
                        result.detail,
                        claim.elapsed_ms
                    ),
                );
                RoundOutcome::retry_error(POLL_INTERVAL_MS)
            }
        }
    }

    fn run_worker(session: &Arc<Session>, generation: u32, mode: Mode) {
        if mode == Mode::Unlock {
            log_event(
                "worker",
                &format!("phase=start mode=unlock wait_s={UNLOCK_WAIT_SECONDS}"),
            );
        } else {
            log_event(
                "worker",
                &format!("phase=start mode=logon budget_ms={WORKER_BUDGET_MS}"),
            );
        }

        let mut config = Config::default();
        let mut error = String::new();
        match load_config(&mut config, &mut error) {
            ConfigState::Disabled => {
                log_event("config", "state=disabled");
                return;
            }
            ConfigState::Invalid => {
                log_event("config", &format!("state=invalid error={error}"));
                return;
            }
            ConfigState::Ok => {
                log_event(
                    "config",
                    &format!(
                        "state=ok machine={} service={}",
                        config.machine,
                        redact_service_url(&config.service_url)
                    ),
                );
            }
        }

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

        let mut armed = false;
        let mut stopped = false;
        let mut dedup: Option<ErrorDedup> = if mode == Mode::Unlock {
            Some(ErrorDedup::default())
        } else {
            None
        };
        let mut error_attempt = 0u32;
        while !armed && !stopped {
            if session.cancelled(generation) {
                break;
            }
            let now = session.elapsed_ms();
            if mode == Mode::Logon && !within_budget(now) {
                break;
            }

            let context = RoundContext {
                config: &config,
                endpoint: &endpoint,
                secret: &secret,
                mode,
            };
            let outcome = one_round(session, generation, &context, now, &mut dedup);
            match outcome.kind {
                RoundKind::Armed => armed = true,
                RoundKind::Fatal => stopped = true,
                RoundKind::Retry => {
                    if mode == Mode::Unlock {
                        if outcome.error {
                            let delay = unlock_backoff_delay(error_attempt);
                            error_attempt = error_attempt.saturating_add(1);
                            sleep_interruptible(session, generation, delay);
                        } else {
                            // A denial is the normal long-poll result; the
                            // server already held the request for up to 25 s.
                            error_attempt = 0;
                        }
                    } else {
                        let now = session.elapsed_ms();
                        let left = WORKER_BUDGET_MS.saturating_sub(now);
                        let delay = clamp_delay(outcome.delay_ms, left);
                        if delay > 0 {
                            sleep_interruptible(session, generation, delay);
                        }
                    }
                }
            }
        }

        drop(secret); // `MachineSecret` zeroizes the key and the password.
        if armed {
            // `consume_arming` may already have advanced the generation by the
            // time this line runs; the round still ended by arming the tile.
            log_event("worker", "phase=armed");
        } else if session.cancelled(generation) {
            log_event("worker", "phase=cancelled");
        } else {
            log_event("worker", "phase=stopped");
        }
    }

    /// Arguments moved into the claim request thread.
    struct ClaimJob {
        endpoint: ServiceEndpoint,
        path: String,
        body: Vec<u8>,
        receive_timeout_ms: i32,
        sender: std::sync::mpsc::Sender<HttpResult>,
        module: HMODULE,
    }

    /// Short-lived thread that performs one long-poll claim.
    ///
    /// The module is pinned for exactly this thread's lifetime, so the DLL
    /// cannot unload under a request that outlives the worker.
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

        let result = post_json_with_receive_timeout(&endpoint, &path, &body, receive_timeout_ms);
        // Every Rust object with a `Drop` impl is gone before the module is
        // released. A send to a dropped receiver only discards the result.
        drop(endpoint);
        drop(path);
        drop(body);
        let _ = sender.send(result);
        drop(sender);
        ACTIVE_WORKERS.fetch_sub(1, Ordering::SeqCst);

        // SAFETY: releases the pin taken in `cancellable_post` and exits the
        // thread; it diverges, so no code from this module runs afterwards.
        unsafe { FreeLibraryAndExitThread(module, 0) }
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
    ) -> Option<HttpResult> {
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
        if pinned.is_err() {
            // Degraded but still bounded: cancel is checked right after.
            return Some(post_json_with_receive_timeout(
                endpoint,
                path,
                body,
                receive_timeout_ms,
            ));
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
        match thread {
            Ok(handle) => {
                // SAFETY: `handle` is open and owned here; closing it does not
                // stop the thread.
                unsafe {
                    let _ = CloseHandle(handle);
                }
            }
            Err(_) => {
                ACTIVE_WORKERS.fetch_sub(1, Ordering::SeqCst);
                // SAFETY: `parameter` came from `Box::into_raw` above.
                drop(unsafe { Box::from_raw(parameter.cast::<ClaimJob>()) });
                // SAFETY: balances the successful GetModuleHandleExW above.
                unsafe {
                    let _ = FreeLibrary(module);
                }
                return Some(post_json_with_receive_timeout(
                    endpoint,
                    path,
                    body,
                    receive_timeout_ms,
                ));
            }
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
    /// The module is pinned for exactly this thread's lifetime: every Rust
    /// object with a `Drop` impl is gone before `FreeLibraryAndExitThread`, so
    /// no code from this DLL can run after the module is released.
    unsafe extern "system" fn worker_thread_proc(parameter: *mut c_void) -> u32 {
        // SAFETY: `parameter` comes from `Box::into_raw(StartArgs)` in
        // `start_worker_thread` and this thread owns it.
        let StartArgs {
            session,
            module,
            mode,
            generation,
        } = *unsafe { Box::from_raw(parameter.cast::<StartArgs>()) };

        {
            guard(
                || run_worker(&session, generation, mode),
                || log_event("worker", "phase=exception"),
            );
            session.finish_worker(generation);
            // Bookkeeping for DllCanUnloadNow; the module pin above is what
            // actually makes unloading safe.
            ACTIVE_WORKERS.fetch_sub(1, Ordering::SeqCst);
        }
        // The only remaining Rust object is the session reference itself; drop
        // it before the module can disappear.
        drop(session);

        // SAFETY: releases the pin taken in `start_worker_thread` and exits the
        // thread. It diverges, so no code below (or after this call) runs in the
        // possibly unloaded module.
        unsafe { FreeLibraryAndExitThread(module, 0) }
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
        match thread {
            Ok(handle) => {
                // SAFETY: `handle` is open and owned here; closing it does not
                // stop the thread.
                unsafe {
                    let _ = CloseHandle(handle);
                }
            }
            Err(_) => {
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
