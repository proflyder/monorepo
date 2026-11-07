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

    use wakegrant_protocol::{
        ClaimOutcome, MachineSecret, Purpose, challenge_request_json,
        claim_mac_message_for_purpose, claim_request_json_for_purpose, hmac_sha256_b64,
        parse_challenge_response, verify_claim_response_for_purpose,
    };

    use crate::bindings::Windows::Win32::Foundation::{CloseHandle, FreeLibrary, HMODULE};
    use crate::bindings::Windows::Win32::System::LibraryLoader::{
        FreeLibraryAndExitThread, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GetModuleHandleExW,
    };
    use crate::bindings::Windows::Win32::System::Threading::{CreateThread, THREAD_CREATION_FLAGS};
    use windows_core::PCWSTR;

    use crate::config::{
        Config, ConfigError, ServiceEndpoint, endpoint_path, load_config, load_secret,
        parse_service_url, redact_service_url,
    };
    use crate::ffi_guard::guard;
    use crate::gate::Mode;
    use crate::http::{HttpOutcome, post_json, post_json_with_receive_timeout};
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

    /// Logs a transport failure or a non-200 status and turns it into the
    /// round outcome the loops expect; challenge and claim share it.
    fn http_response(
        outcome: HttpOutcome,
        request: &str,
        prefix: &str,
        dedup: &mut Option<ErrorDedup>,
    ) -> Result<(Vec<u8>, u32), RoundOutcome> {
        match outcome {
            HttpOutcome::Transport { error, elapsed_ms } => {
                log_error(
                    dedup,
                    "http",
                    &format!("{request}-transport-{error}"),
                    &format!("request={request} ok=0 error={error} ms={elapsed_ms} {prefix}"),
                );
                Err(RoundOutcome::retry_error(POLL_INTERVAL_MS))
            }
            HttpOutcome::Response {
                status: 200,
                body,
                elapsed_ms,
            } => Ok((body, elapsed_ms)),
            HttpOutcome::Response {
                status, elapsed_ms, ..
            } => {
                log_error(
                    dedup,
                    "http",
                    &format!("{request}-status-{status}"),
                    &format!("request={request} ok=1 status={status} ms={elapsed_ms} {prefix}"),
                );
                Err(retry_delay_for_status(status)
                    .map_or_else(RoundOutcome::fatal, RoundOutcome::retry_error))
            }
        }
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
        let purpose = if mode == Mode::Unlock {
            Purpose::Unlock
        } else {
            Purpose::Logon
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
        let (challenge_body, _) = match http_response(challenge, "challenge", &prefix, dedup) {
            Ok(value) => value,
            Err(outcome) => return outcome,
        };

        let Ok(challenge_body) = std::str::from_utf8(&challenge_body) else {
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
        let wait_seconds = (purpose == Purpose::Unlock).then_some(UNLOCK_WAIT_SECONDS);
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
        let (claim_body, claim_ms) = match http_response(claim, "claim", &prefix, dedup) {
            Ok(value) => value,
            Err(outcome) => return outcome,
        };

        let Ok(body) = std::str::from_utf8(&claim_body) else {
            log_error(
                dedup,
                "grant",
                "grant-invalid-utf8",
                &format!("result=malformed detail=invalid-utf8 ms={claim_ms} {prefix}"),
            );
            return RoundOutcome::retry_error(POLL_INTERVAL_MS);
        };
        let result = verify_claim_response_for_purpose(
            body,
            secret.key(),
            &config.machine,
            &response.nonce,
            purpose,
        );

        match result.outcome {
            ClaimOutcome::Granted => {
                log_event(
                    "grant",
                    &format!(
                        "result=granted purpose={} grant_id={} ms={claim_ms} {prefix}",
                        purpose.as_str(),
                        result.grant_id
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
                        &format!("result=denied purpose=logon ms={claim_ms} {prefix}"),
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
                        "result={label} purpose={} detail={} ms={claim_ms} {prefix}",
                        purpose.as_str(),
                        result.detail
                    ),
                );
                RoundOutcome::retry_error(POLL_INTERVAL_MS)
            }
        }
    }

    fn run_worker(session: &Arc<Session>, generation: u32, mode: Mode) {
        let start = if mode == Mode::Unlock {
            format!("phase=start mode=unlock wait_s={UNLOCK_WAIT_SECONDS}")
        } else {
            format!("phase=start mode=logon budget_ms={WORKER_BUDGET_MS}")
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
        // `consume_arming` may already have advanced the generation by the time
        // this line runs; the round still ended by arming the tile.
        let phase = if armed {
            "phase=armed"
        } else if session.cancelled(generation) {
            "phase=cancelled"
        } else {
            "phase=stopped"
        };
        log_event("worker", phase);
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
