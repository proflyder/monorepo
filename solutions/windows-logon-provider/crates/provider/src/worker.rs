//! The single background poller.
//!
//! Reads the configuration, then loops `POST /v1/challenge` -> `POST /v1/claim`
//! until a valid grant is armed, the 90 s budget since provider load is
//! exhausted, or the session is cancelled. It never blocks a LogonUI thread and
//! never survives past its budget.
//!
//! The retry/budget decisions are plain functions so they are testable on the
//! build host.

/// Poll interval between rounds.
pub const POLL_INTERVAL_MS: u32 = 2000;
/// Back-off for `429`/`5xx`, matching the C++ worker.
pub const BACKOFF_INTERVAL_MS: u32 = 4000;

/// How a round ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundKind {
    /// The tile is armed; the worker stops.
    Armed,
    /// Sleep for `delay_ms` and try again.
    Retry,
    /// Stop the worker.
    Fatal,
}

/// Result of one challenge/claim exchange.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoundOutcome {
    pub kind: RoundKind,
    pub delay_ms: u32,
}

impl RoundOutcome {
    pub const fn retry(delay_ms: u32) -> Self {
        Self {
            kind: RoundKind::Retry,
            delay_ms,
        }
    }

    pub const fn fatal() -> Self {
        Self {
            kind: RoundKind::Fatal,
            delay_ms: 0,
        }
    }

    pub const fn armed() -> Self {
        Self {
            kind: RoundKind::Armed,
            delay_ms: 0,
        }
    }
}

/// A non-200 status is a back-off only for 429 and 5xx; every other status is
/// fatal (`None`), exactly like the C++ switch.
pub fn retry_delay_for_status(status: i32) -> Option<u32> {
    if status == 429 || status >= 500 {
        Some(BACKOFF_INTERVAL_MS)
    } else {
        None
    }
}

/// Clamps a sleep to the remaining budget so the thread never outlives it.
pub fn clamp_delay(delay_ms: u32, remaining_ms: u64) -> u32 {
    u64::from(delay_ms).min(remaining_ms) as u32
}

#[cfg(windows)]
pub use windows_impl::start_worker_thread;

#[cfg(windows)]
mod windows_impl {
    use std::sync::Arc;
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

    use logon_protocol::{
        ClaimOutcome, challenge_request_json, claim_mac_message, endpoint_path, hmac_sha256_b64,
        parse_challenge_response, parse_service_url, redact_service_url, verify_claim_response,
    };

    use crate::config::{Config, ConfigState, load_config, load_secret};
    use crate::ffi_guard::guard;
    use crate::http::post_json;
    use crate::log::log_event;
    use crate::provider::{
        ACTIVE_WORKERS, ArmResult, Session, WORKER_BUDGET_MS, arm_session, within_budget,
    };
    use crate::worker::{
        POLL_INTERVAL_MS, RoundKind, RoundOutcome, clamp_delay, retry_delay_for_status,
    };

    /// Keeps `ACTIVE_WORKERS` accurate for `DllCanUnloadNow`.
    struct ActiveWorker;

    impl Drop for ActiveWorker {
        fn drop(&mut self) {
            ACTIVE_WORKERS.fetch_sub(1, Ordering::SeqCst);
        }
    }

    fn sleep_interruptible(session: &Arc<Session>, generation: u32, milliseconds: u32) {
        let deadline = Instant::now() + Duration::from_millis(u64::from(milliseconds));
        while Instant::now() < deadline {
            if session.cancelled(generation) {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// One challenge/claim exchange. Any unexpected answer ends the round
    /// without arming; only a verified grant arms the session.
    fn one_round(
        session: &Arc<Session>,
        generation: u32,
        config: &Config,
        endpoint: &logon_protocol::ServiceEndpoint,
        secret: &logon_protocol::MachineSecret,
        elapsed_ms: u64,
    ) -> RoundOutcome {
        let prefix = format!("elapsed_ms={elapsed_ms}");

        let challenge_path = endpoint_path(endpoint, "/v1/challenge");
        let challenge = post_json(
            endpoint,
            &challenge_path,
            challenge_request_json(&config.machine).as_bytes(),
        );
        if !challenge.ok {
            log_event(
                "http",
                &format!(
                    "request=challenge ok=0 error={} ms={} {prefix}",
                    challenge.error, challenge.elapsed_ms
                ),
            );
            return RoundOutcome::retry(POLL_INTERVAL_MS);
        }
        if challenge.status != 200 {
            log_event(
                "http",
                &format!(
                    "request=challenge ok=1 status={} ms={} {prefix}",
                    challenge.status, challenge.elapsed_ms
                ),
            );
            return match retry_delay_for_status(challenge.status) {
                Some(delay) => RoundOutcome::retry(delay),
                None => RoundOutcome::fatal(),
            };
        }

        let Ok(challenge_body) = std::str::from_utf8(&challenge.body) else {
            log_event("challenge", &format!("ok=0 error=invalid-utf8 {prefix}"));
            return RoundOutcome::fatal();
        };
        let response = match parse_challenge_response(challenge_body) {
            Ok(response) => response,
            Err(error) => {
                log_event(
                    "challenge",
                    &format!("ok=0 error={} {prefix}", error.detail()),
                );
                return RoundOutcome::fatal();
            }
        };

        let mac = hmac_sha256_b64(
            secret.key(),
            &claim_mac_message(&config.machine, &response.nonce),
        );
        if mac.is_empty() {
            log_event("claim", &format!("ok=0 error=hmac_failed {prefix}"));
            return RoundOutcome::fatal();
        }

        // machine, nonce and mac are restricted to [a-z0-9-] and base64
        // characters, so they need no JSON escaping.
        let claim_body = format!(
            "{{\"machine\":\"{}\",\"nonce\":\"{}\",\"mac\":\"{}\"}}",
            config.machine, response.nonce, mac
        );
        let claim_path = endpoint_path(endpoint, "/v1/claim");
        let claim = post_json(endpoint, &claim_path, claim_body.as_bytes());
        if !claim.ok {
            log_event(
                "http",
                &format!(
                    "request=claim ok=0 error={} ms={} {prefix}",
                    claim.error, claim.elapsed_ms
                ),
            );
            return RoundOutcome::retry(POLL_INTERVAL_MS);
        }
        if claim.status != 200 {
            log_event(
                "http",
                &format!(
                    "request=claim ok=1 status={} ms={} {prefix}",
                    claim.status, claim.elapsed_ms
                ),
            );
            return match retry_delay_for_status(claim.status) {
                Some(delay) => RoundOutcome::retry(delay),
                None => RoundOutcome::fatal(),
            };
        }

        let result = match std::str::from_utf8(&claim.body) {
            Ok(body) => verify_claim_response(body, secret.key(), &config.machine, &response.nonce),
            Err(_) => {
                log_event(
                    "grant",
                    &format!(
                        "result=malformed detail=invalid-utf8 ms={} {prefix}",
                        claim.elapsed_ms
                    ),
                );
                return RoundOutcome::retry(POLL_INTERVAL_MS);
            }
        };

        match result.outcome {
            ClaimOutcome::Granted => {
                log_event(
                    "grant",
                    &format!(
                        "result=granted grant_id={} ms={} {prefix}",
                        result.grant_id, claim.elapsed_ms
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
                log_event(
                    "grant",
                    &format!("result=denied ms={} {prefix}", claim.elapsed_ms),
                );
                RoundOutcome::retry(POLL_INTERVAL_MS)
            }
            ClaimOutcome::InvalidMac | ClaimOutcome::Malformed => {
                let label = if result.outcome == ClaimOutcome::InvalidMac {
                    "invalid_mac"
                } else {
                    "malformed"
                };
                log_event(
                    "grant",
                    &format!(
                        "result={label} detail={} ms={} {prefix}",
                        result.detail, claim.elapsed_ms
                    ),
                );
                RoundOutcome::retry(POLL_INTERVAL_MS)
            }
        }
    }

    fn run_worker(session: &Arc<Session>, generation: u32) {
        log_event(
            "worker",
            &format!("phase=start budget_ms={WORKER_BUDGET_MS}"),
        );

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
        while !armed && !stopped {
            if session.cancelled(generation) {
                break;
            }
            let now = session.elapsed_ms();
            if !within_budget(now) {
                break;
            }

            let outcome = one_round(session, generation, &config, &endpoint, &secret, now);
            match outcome.kind {
                RoundKind::Armed => armed = true,
                RoundKind::Fatal => stopped = true,
                RoundKind::Retry => {
                    let now = session.elapsed_ms();
                    let left = WORKER_BUDGET_MS.saturating_sub(now);
                    let delay = clamp_delay(outcome.delay_ms, left);
                    if delay > 0 {
                        sleep_interruptible(session, generation, delay);
                    }
                }
            }
        }

        drop(secret); // `MachineSecret` zeroizes the key and the password.
        if session.cancelled(generation) {
            log_event("worker", "phase=cancelled");
        } else if armed {
            log_event("worker", "phase=armed");
        } else {
            log_event("worker", "phase=stopped");
        }
    }

    fn worker_thread_proc(session: Arc<Session>) {
        let _active = ActiveWorker;
        let generation = session.generation();
        guard(
            || run_worker(&session, generation),
            || log_event("worker", "phase=exception"),
        );
        session.running.store(false, Ordering::SeqCst);
    }

    /// Creates the single worker thread. The caller must have set
    /// `session.running`.
    pub fn start_worker_thread(session: &Arc<Session>) {
        let worker_session = Arc::clone(session);
        let reset_session = Arc::clone(session);
        ACTIVE_WORKERS.fetch_add(1, Ordering::SeqCst);
        // `std::thread` uses CreateThread underneath. This is not the LogonUI
        // thread, exactly like the C++ `CreateThread` call.
        let spawned = std::thread::Builder::new()
            .name("ricardo-logon-worker".to_owned())
            .spawn(move || worker_thread_proc(worker_session));
        match spawned {
            Ok(handle) => {
                // Detach: the worker is bounded by the budget and never joined.
                drop(handle);
            }
            Err(_) => {
                ACTIVE_WORKERS.fetch_sub(1, Ordering::SeqCst);
                reset_session.running.store(false, Ordering::SeqCst);
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
    fn outcome_constructors_keep_the_documented_kinds() {
        assert_eq!(RoundOutcome::retry(1).kind, RoundKind::Retry);
        assert_eq!(RoundOutcome::retry(7).delay_ms, 7);
        assert_eq!(RoundOutcome::fatal().kind, RoundKind::Fatal);
        assert_eq!(RoundOutcome::armed().kind, RoundKind::Armed);
        assert_eq!(RoundOutcome::armed().delay_ms, 0);
    }
}
