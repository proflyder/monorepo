//! Portable challenge/claim exchange.
//!
//! Everything a round decides — pacing, retries, back-off, budget, dedup and
//! the arming step — lives here, behind small ports. The ports have Win32
//! implementations in [`crate::worker`] and scripted fakes in the tests, so the
//! exchange is exercised on the host without Windows, network or real sleeps.
//!
//! The wire format, request bodies and log lines are unchanged from the
//! previous Windows-only implementation: only their composition is portable.

// The production consumers of these ports are the `#[cfg(windows)]` adapters;
// host builds exercise them through `cfg(test)`.
#![cfg_attr(not(any(windows, test)), allow(dead_code))]

use wakegrant_protocol::{
    ClaimOutcome, MachineSecret, Purpose, challenge_request_json, claim_mac_message_for_purpose,
    claim_request_json_for_purpose, hmac_sha256_b64, parse_challenge_response,
    verify_claim_response_for_purpose,
};

use crate::config::{Config, ServiceEndpoint, endpoint_path};
use crate::gate::Mode;
use crate::http::{DEFAULT_RECEIVE_TIMEOUT_MS, HttpOutcome};
use crate::provider::{WORKER_BUDGET_MS, within_budget};
use crate::worker::{
    POLL_INTERVAL_MS, RoundKind, RoundOutcome, UNLOCK_RECEIVE_TIMEOUT_MS, UNLOCK_WAIT_SECONDS,
    clamp_delay, unlock_backoff_delay, unlock_round_delay,
};

/// One HTTP POST the exchange wants to perform.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PostRequest<'a> {
    pub(crate) endpoint: &'a ServiceEndpoint,
    pub(crate) path: &'a str,
    pub(crate) body: &'a [u8],
    pub(crate) receive_timeout_ms: i32,
    /// True for the unlock claim, which runs on its own module-pinned thread so
    /// a long-poll can be abandoned when the session is cancelled.
    pub(crate) cancellable: bool,
}

/// HTTP transport port. `None` means the request was cancelled in flight.
pub(crate) trait Transport {
    fn post(&self, request: PostRequest<'_>) -> Option<HttpOutcome>;
}

/// Monotonic millisecond clock and sleep port.
pub(crate) trait Clock {
    fn now_ms(&self) -> u64;
    fn sleep_ms(&self, milliseconds: u32);
}

/// Event journal port; production appends the documented log lines.
pub(crate) trait EventLog {
    fn event(&self, event: &str, fields: &str);
}

/// What the tile did when the round tried to arm it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArmOutcome {
    Armed,
    Cancelled,
    /// LogonUI has not advised the provider yet; retry shortly, never re-claim.
    Unavailable,
}

/// The session and tile port.
pub(crate) trait Target {
    fn cancelled(&self) -> bool;
    fn arm(&self) -> ArmOutcome;
}

/// Why the exchange loop ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LoopEnd {
    Armed,
    Cancelled,
    Stopped,
}

/// Everything a loop needs besides its ports.
pub(crate) struct LoopContext<'a> {
    pub(crate) config: &'a Config,
    pub(crate) endpoint: &'a ServiceEndpoint,
    pub(crate) secret: &'a MachineSecret,
    pub(crate) mode: Mode,
    /// Clock value the provider was loaded at; the logon budget counts from it.
    pub(crate) origin_ms: u64,
}

/// Suppresses repeated identical error lines in the unlock loop, which can run
/// all night. Every error kind is logged again at most every tenth repeat, and
/// any different error is logged immediately.
#[derive(Default)]
struct ErrorDedup {
    last: String,
    repeats: u32,
}

impl ErrorDedup {
    /// `key` identifies the error kind; `fields` may contain per-round timings,
    /// so it cannot be the dedup key itself.
    fn log(&mut self, log: &dyn EventLog, event: &str, key: &str, fields: &str) {
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
        log.event(event, fields);
    }
}

/// The unlock loop dedups identical errors; the short logon loop does not.
struct ErrorReporter {
    dedup: ErrorDedup,
    enabled: bool,
}

impl ErrorReporter {
    fn new(mode: Mode) -> Self {
        Self {
            dedup: ErrorDedup::default(),
            enabled: mode == Mode::Unlock,
        }
    }

    fn error(&mut self, log: &dyn EventLog, event: &str, key: &str, fields: &str) {
        if self.enabled {
            self.dedup.log(log, event, key, fields);
        } else {
            log.event(event, fields);
        }
    }
}

/// Sleeps up to `milliseconds` in short steps, returning as soon as the
/// session is cancelled.
fn sleep_interruptible(target: &dyn Target, clock: &dyn Clock, milliseconds: u32) -> bool {
    let deadline = clock.now_ms() + u64::from(milliseconds);
    loop {
        if target.cancelled() {
            return false;
        }
        let now = clock.now_ms();
        if now >= deadline {
            return true;
        }
        clock.sleep_ms((deadline - now).min(100) as u32);
    }
}

/// Logs a transport failure or a non-200 status and turns it into the round
/// outcome the loops expect; challenge and claim share it.
fn http_response(
    outcome: HttpOutcome,
    request: &str,
    prefix: &str,
    errors: &mut ErrorReporter,
    log: &dyn EventLog,
) -> Result<(Vec<u8>, u32), RoundOutcome> {
    match outcome {
        HttpOutcome::Transport { error, elapsed_ms } => {
            errors.error(
                log,
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
            errors.error(
                log,
                "http",
                &format!("{request}-status-{status}"),
                &format!("request={request} ok=1 status={status} ms={elapsed_ms} {prefix}"),
            );
            Err(crate::worker::retry_delay_for_status(status)
                .map_or_else(RoundOutcome::fatal, RoundOutcome::retry_error))
        }
    }
}

fn post(
    transport: &dyn Transport,
    endpoint: &ServiceEndpoint,
    path: &str,
    body: &[u8],
    receive_timeout_ms: i32,
    cancellable: bool,
) -> Option<HttpOutcome> {
    transport.post(PostRequest {
        endpoint,
        path,
        body,
        receive_timeout_ms,
        cancellable,
    })
}

/// One challenge/claim exchange. Any unexpected answer ends the round without
/// arming; only a verified grant arms the session.
fn one_round(
    context: &LoopContext<'_>,
    target: &dyn Target,
    transport: &dyn Transport,
    clock: &dyn Clock,
    log: &dyn EventLog,
    errors: &mut ErrorReporter,
    elapsed_ms: u64,
) -> RoundOutcome {
    let LoopContext {
        config,
        endpoint,
        secret,
        mode,
        ..
    } = *context;
    let purpose = if mode == Mode::Unlock {
        Purpose::Unlock
    } else {
        Purpose::Logon
    };
    let prefix = format!("elapsed_ms={elapsed_ms}");
    if target.cancelled() {
        return RoundOutcome::fatal();
    }

    let challenge_path = endpoint_path(endpoint, "/v1/challenge");
    let challenge = post(
        transport,
        endpoint,
        &challenge_path,
        challenge_request_json(&config.machine).as_bytes(),
        DEFAULT_RECEIVE_TIMEOUT_MS,
        false,
    );
    let Some(challenge) = challenge else {
        return RoundOutcome::fatal();
    };
    if target.cancelled() {
        return RoundOutcome::fatal();
    }
    let (challenge_body, _) = match http_response(challenge, "challenge", &prefix, errors, log) {
        Ok(value) => value,
        Err(outcome) => return outcome,
    };

    let Ok(challenge_body) = std::str::from_utf8(&challenge_body) else {
        errors.error(
            log,
            "challenge",
            "challenge-invalid-utf8",
            &format!("ok=0 error=invalid-utf8 {prefix}"),
        );
        return RoundOutcome::fatal();
    };
    let response = match parse_challenge_response(challenge_body) {
        Ok(response) => response,
        Err(error) => {
            errors.error(
                log,
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
    let receive_timeout_ms = if purpose == Purpose::Unlock {
        UNLOCK_RECEIVE_TIMEOUT_MS
    } else {
        DEFAULT_RECEIVE_TIMEOUT_MS
    };
    let claim = post(
        transport,
        endpoint,
        &claim_path,
        claim_body.as_bytes(),
        receive_timeout_ms,
        purpose == Purpose::Unlock,
    );
    // Cancelled while the long-poll was in flight.
    let Some(claim) = claim else {
        return RoundOutcome::fatal();
    };
    if target.cancelled() {
        return RoundOutcome::fatal();
    }
    let (claim_body, claim_ms) = match http_response(claim, "claim", &prefix, errors, log) {
        Ok(value) => value,
        Err(outcome) => return outcome,
    };

    let Ok(body) = std::str::from_utf8(&claim_body) else {
        errors.error(
            log,
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
            log.event(
                "grant",
                &format!(
                    "result=granted purpose={} grant_id={} ms={claim_ms} {prefix}",
                    purpose.as_str(),
                    result.grant_id
                ),
            );
            // The grant is already consumed, so never claim again: only wait a
            // moment for Advise() and then give up.
            for _ in 0..20 {
                match target.arm() {
                    ArmOutcome::Armed => {
                        log.event("armed", "ok=1");
                        return RoundOutcome::armed();
                    }
                    ArmOutcome::Cancelled => {
                        log.event("armed", "ok=0 reason=cancelled");
                        return RoundOutcome::fatal();
                    }
                    ArmOutcome::Unavailable => {}
                }
                if target.cancelled() {
                    log.event("armed", "ok=0 reason=cancelled");
                    return RoundOutcome::fatal();
                }
                sleep_interruptible(target, clock, 100);
            }
            log.event("armed", "ok=0 reason=no_events");
            RoundOutcome::fatal()
        }
        ClaimOutcome::Denied => {
            if purpose == Purpose::Logon {
                log.event(
                    "grant",
                    &format!("result=denied purpose=logon ms={claim_ms} {prefix}"),
                );
            }
            // Unlock denials are not logged: the long-poll is the pause, and a
            // lock screen can stay up all night.
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
            errors.error(
                log,
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

/// Runs challenge/claim rounds until the tile is armed, the session is
/// cancelled or the logon budget is exhausted.
pub(crate) fn run_exchange(
    context: &LoopContext<'_>,
    target: &dyn Target,
    transport: &dyn Transport,
    clock: &dyn Clock,
    log: &dyn EventLog,
) -> LoopEnd {
    let mut errors = ErrorReporter::new(context.mode);
    let mut error_attempt = 0u32;
    loop {
        if target.cancelled() {
            return LoopEnd::Cancelled;
        }
        let now = clock.now_ms().saturating_sub(context.origin_ms);
        if context.mode == Mode::Logon && !within_budget(now) {
            // A cancel racing the budget check must still log phase=cancelled.
            return if target.cancelled() {
                LoopEnd::Cancelled
            } else {
                LoopEnd::Stopped
            };
        }

        let round_started = clock.now_ms();
        let outcome = one_round(context, target, transport, clock, log, &mut errors, now);
        let round_ms = clock.now_ms().saturating_sub(round_started);
        match outcome.kind {
            RoundKind::Armed => return LoopEnd::Armed,
            RoundKind::Fatal => {
                return if target.cancelled() {
                    LoopEnd::Cancelled
                } else {
                    LoopEnd::Stopped
                };
            }
            RoundKind::Retry => {
                let delay = if context.mode == Mode::Unlock {
                    if outcome.error {
                        let backoff = unlock_backoff_delay(error_attempt);
                        error_attempt = error_attempt.saturating_add(1);
                        unlock_round_delay(round_ms, Some(backoff))
                    } else {
                        // A denial is the normal long-poll result; the server
                        // already held the request. An instant denial is paced
                        // by the minimum round interval.
                        error_attempt = 0;
                        unlock_round_delay(round_ms, None)
                    }
                } else {
                    let now = clock.now_ms().saturating_sub(context.origin_ms);
                    let left = WORKER_BUDGET_MS.saturating_sub(now);
                    clamp_delay(outcome.delay_ms, left)
                };
                if delay > 0 && !sleep_interruptible(target, clock, delay) {
                    return LoopEnd::Cancelled;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;
    use std::rc::Rc;

    use wakegrant_protocol::{
        Purpose, b64_encode, grant_mac_message_for_purpose, hmac_sha256_b64, parse_secret_json,
    };

    use super::{
        ArmOutcome, Clock, EventLog, LoopContext, LoopEnd, PostRequest, Target, Transport,
        run_exchange,
    };
    use crate::config::{Config, ServiceEndpoint};
    use crate::gate::Mode;
    use crate::http::HttpOutcome;

    const MACHINE: &str = "komp";

    struct FakeClock {
        now: Cell<u64>,
    }

    impl FakeClock {
        fn new(now: u64) -> Rc<Self> {
            Rc::new(Self {
                now: Cell::new(now),
            })
        }

        fn advance(&self, milliseconds: u32) {
            self.now.set(self.now.get() + u64::from(milliseconds));
        }
    }

    impl Clock for FakeClock {
        fn now_ms(&self) -> u64 {
            self.now.get()
        }

        fn sleep_ms(&self, milliseconds: u32) {
            self.advance(milliseconds);
        }
    }

    enum Reply {
        Response {
            status: i32,
            body: String,
            elapsed_ms: u32,
        },
        Transport {
            error: &'static str,
            elapsed_ms: u32,
        },
        Cancelled(Rc<Cell<bool>>),
    }

    struct FakeTransport {
        clock: Rc<FakeClock>,
        replies: RefCell<VecDeque<Reply>>,
        request_times: RefCell<Vec<u64>>,
        bodies: RefCell<Vec<String>>,
        requests: RefCell<Vec<(String, String, i32, bool)>>,
    }

    impl FakeTransport {
        fn new(clock: Rc<FakeClock>, replies: Vec<Reply>) -> Rc<Self> {
            Rc::new(Self {
                clock,
                replies: RefCell::new(replies.into()),
                request_times: RefCell::new(Vec::new()),
                bodies: RefCell::new(Vec::new()),
                requests: RefCell::new(Vec::new()),
            })
        }
    }

    impl Transport for FakeTransport {
        fn post(&self, request: PostRequest<'_>) -> Option<HttpOutcome> {
            let reply = self.replies.borrow_mut().pop_front()?;
            if let Reply::Cancelled(flag) = &reply {
                flag.set(true);
                return None;
            }
            self.request_times.borrow_mut().push(self.clock.now_ms());
            self.requests.borrow_mut().push((
                request.endpoint.host.clone(),
                request.path.to_owned(),
                request.receive_timeout_ms,
                request.cancellable,
            ));
            self.bodies
                .borrow_mut()
                .push(String::from_utf8_lossy(request.body).into_owned());
            match reply {
                Reply::Response {
                    status,
                    body,
                    elapsed_ms,
                } => {
                    self.clock.advance(elapsed_ms);
                    Some(HttpOutcome::Response {
                        status,
                        body: body.into_bytes(),
                        elapsed_ms,
                    })
                }
                Reply::Transport { error, elapsed_ms } => {
                    self.clock.advance(elapsed_ms);
                    Some(HttpOutcome::Transport { error, elapsed_ms })
                }
                Reply::Cancelled(_) => None,
            }
        }
    }

    struct FakeTarget {
        cancelled: Rc<Cell<bool>>,
        arm_results: RefCell<VecDeque<ArmOutcome>>,
        armed: Cell<u32>,
    }

    impl FakeTarget {
        fn new() -> Rc<Self> {
            Rc::new(Self {
                cancelled: Rc::new(Cell::new(false)),
                arm_results: RefCell::new(VecDeque::new()),
                armed: Cell::new(0),
            })
        }

        fn with_arm_results(results: Vec<ArmOutcome>) -> Rc<Self> {
            let target = Self::new();
            target.arm_results.borrow_mut().extend(results);
            target
        }
    }

    impl Target for FakeTarget {
        fn cancelled(&self) -> bool {
            self.cancelled.get()
        }

        fn arm(&self) -> ArmOutcome {
            let outcome = self
                .arm_results
                .borrow_mut()
                .pop_front()
                .unwrap_or(ArmOutcome::Armed);
            if outcome == ArmOutcome::Armed {
                self.armed.set(self.armed.get() + 1);
            } else if outcome == ArmOutcome::Cancelled {
                self.cancelled.set(true);
            }
            outcome
        }
    }

    #[derive(Default)]
    struct FakeLog {
        lines: RefCell<Vec<String>>,
    }

    impl FakeLog {
        fn saw(&self, text: &str) -> bool {
            self.lines.borrow().iter().any(|line| line.contains(text))
        }
    }

    impl EventLog for FakeLog {
        fn event(&self, event: &str, fields: &str) {
            self.lines.borrow_mut().push(format!("{event} {fields}"));
        }
    }

    struct Fixture {
        config: Config,
        endpoint: ServiceEndpoint,
        secret: wakegrant_protocol::MachineSecret,
        nonce: String,
    }

    fn fixture() -> Fixture {
        let nonce = b64_encode(&[0u8; 32]);
        let secret = parse_secret_json(&format!(
            r#"{{"version":1,"password":"pw","key":"{}"}}"#,
            b64_encode(&[7u8; 32])
        ))
        .expect("test secret");
        Fixture {
            config: Config {
                service_url: "http://192.0.2.27:30130".to_owned(),
                machine: MACHINE.to_owned(),
                user: "pl".to_owned(),
                domain: ".".to_owned(),
            },
            endpoint: ServiceEndpoint {
                tls: false,
                host: "192.0.2.27".to_owned(),
                port: 30130,
                prefix: String::new(),
            },
            secret,
            nonce,
        }
    }

    fn challenge_body(fixture: &Fixture) -> String {
        format!(r#"{{"nonce":"{}"}}"#, fixture.nonce)
    }

    fn claim_body(fixture: &Fixture, granted: bool, purpose: Purpose) -> String {
        let grant_id = if granted { "grant-1" } else { "" };
        let mac = hmac_sha256_b64(
            fixture.secret.key(),
            &grant_mac_message_for_purpose(MACHINE, &fixture.nonce, granted, grant_id, purpose),
        );
        if granted {
            format!(r#"{{"granted":true,"grantId":"{grant_id}","mac":"{mac}"}}"#)
        } else {
            format!(r#"{{"granted":false,"mac":"{mac}"}}"#)
        }
    }

    fn context<'a>(fixture: &'a Fixture, mode: Mode) -> LoopContext<'a> {
        LoopContext {
            config: &fixture.config,
            endpoint: &fixture.endpoint,
            secret: &fixture.secret,
            mode,
            origin_ms: 0,
        }
    }

    fn ok(body: String, elapsed_ms: u32) -> Reply {
        Reply::Response {
            status: 200,
            body,
            elapsed_ms,
        }
    }

    fn status(value: i32, elapsed_ms: u32) -> Reply {
        Reply::Response {
            status: value,
            body: String::new(),
            elapsed_ms,
        }
    }

    #[test]
    fn a_verified_grant_arms_exactly_once() {
        let fixture = fixture();
        let clock = FakeClock::new(0);
        let transport = FakeTransport::new(
            Rc::clone(&clock),
            vec![
                ok(challenge_body(&fixture), 3),
                ok(claim_body(&fixture, true, Purpose::Logon), 4),
            ],
        );
        let target = FakeTarget::new();
        let log = FakeLog::default();

        assert_eq!(
            run_exchange(
                &context(&fixture, Mode::Logon),
                target.as_ref(),
                transport.as_ref(),
                clock.as_ref(),
                &log,
            ),
            LoopEnd::Armed
        );
        assert_eq!(target.armed.get(), 1);
        assert_eq!(transport.request_times.borrow().len(), 2);
        assert_eq!(
            transport.requests.borrow().as_slice(),
            [
                (
                    "192.0.2.27".to_owned(),
                    "/v1/challenge".to_owned(),
                    4000,
                    false,
                ),
                ("192.0.2.27".to_owned(), "/v1/claim".to_owned(), 4000, false),
            ]
        );
        assert!(log.saw("grant result=granted"));
        assert!(log.saw("armed ok=1"));
    }

    #[test]
    fn a_logon_denial_waits_the_poll_interval() {
        let fixture = fixture();
        let clock = FakeClock::new(0);
        let transport = FakeTransport::new(
            Rc::clone(&clock),
            vec![
                ok(challenge_body(&fixture), 0),
                ok(claim_body(&fixture, false, Purpose::Logon), 10),
                ok(challenge_body(&fixture), 0),
                ok(claim_body(&fixture, false, Purpose::Logon), 10),
            ],
        );
        let target = FakeTarget::new();
        let log = FakeLog::default();

        run_exchange(
            &context(&fixture, Mode::Logon),
            target.as_ref(),
            transport.as_ref(),
            clock.as_ref(),
            &log,
        );
        // First claim ended at 10 ms; the second challenge starts 2 s later.
        assert_eq!(*transport.request_times.borrow(), vec![0, 0, 2010, 2010]);
        assert_eq!(target.armed.get(), 0);
        assert!(log.saw("grant result=denied"));
    }

    #[test]
    fn the_logon_budget_stops_the_loop() {
        let fixture = fixture();
        let clock = FakeClock::new(89_990);
        let transport = FakeTransport::new(
            Rc::clone(&clock),
            vec![
                ok(challenge_body(&fixture), 0),
                ok(claim_body(&fixture, false, Purpose::Logon), 50),
            ],
        );
        let target = FakeTarget::new();
        let log = FakeLog::default();

        assert_eq!(
            run_exchange(
                &context(&fixture, Mode::Logon),
                target.as_ref(),
                transport.as_ref(),
                clock.as_ref(),
                &log,
            ),
            LoopEnd::Stopped
        );
        assert_eq!(transport.request_times.borrow().len(), 2);
    }

    #[test]
    fn a_long_poll_denial_resumes_without_an_extra_pause() {
        let fixture = fixture();
        let clock = FakeClock::new(0);
        let transport = FakeTransport::new(
            Rc::clone(&clock),
            vec![
                ok(challenge_body(&fixture), 5),
                ok(claim_body(&fixture, false, Purpose::Unlock), 25_000),
                ok(challenge_body(&fixture), 5),
                ok(claim_body(&fixture, false, Purpose::Unlock), 25_000),
            ],
        );
        let target = FakeTarget::new();
        let log = FakeLog::default();

        run_exchange(
            &context(&fixture, Mode::Unlock),
            target.as_ref(),
            transport.as_ref(),
            clock.as_ref(),
            &log,
        );
        // The 25 s long-poll is the pause; the next challenge is immediate.
        assert_eq!(
            *transport.request_times.borrow(),
            vec![0, 5, 25_005, 25_010]
        );
        assert!(!transport.requests.borrow()[0].3);
        assert!(transport.requests.borrow()[1].3);
        assert_eq!(transport.requests.borrow()[1].2, 35_000);
    }

    #[test]
    fn an_arm_cancelled_by_the_session_is_fail_closed() {
        let fixture = fixture();
        let clock = FakeClock::new(0);
        let transport = FakeTransport::new(
            Rc::clone(&clock),
            vec![
                ok(challenge_body(&fixture), 0),
                ok(claim_body(&fixture, true, Purpose::Logon), 0),
            ],
        );
        let target = FakeTarget::with_arm_results(vec![ArmOutcome::Cancelled]);
        let log = FakeLog::default();

        assert_eq!(
            run_exchange(
                &context(&fixture, Mode::Logon),
                target.as_ref(),
                transport.as_ref(),
                clock.as_ref(),
                &log,
            ),
            LoopEnd::Cancelled
        );
        assert_eq!(target.armed.get(), 0);
        assert!(log.saw("armed ok=0 reason=cancelled"));
    }

    #[test]
    fn an_instant_unlock_denial_waits_the_minimum_round() {
        let fixture = fixture();
        let clock = FakeClock::new(0);
        let transport = FakeTransport::new(
            Rc::clone(&clock),
            vec![
                ok(challenge_body(&fixture), 0),
                ok(claim_body(&fixture, false, Purpose::Unlock), 0),
                ok(challenge_body(&fixture), 0),
                ok(claim_body(&fixture, false, Purpose::Unlock), 0),
            ],
        );
        let target = FakeTarget::new();
        let log = FakeLog::default();

        run_exchange(
            &context(&fixture, Mode::Unlock),
            target.as_ref(),
            transport.as_ref(),
            clock.as_ref(),
            &log,
        );
        assert_eq!(*transport.request_times.borrow(), vec![0, 0, 2_000, 2_000]);
    }

    #[test]
    fn unlock_errors_back_off_and_retry() {
        let fixture = fixture();
        let clock = FakeClock::new(0);
        let transport = FakeTransport::new(
            Rc::clone(&clock),
            vec![
                ok(challenge_body(&fixture), 0),
                status(429, 100),
                ok(challenge_body(&fixture), 0),
                Reply::Transport {
                    error: "receive_failed",
                    elapsed_ms: 100,
                },
            ],
        );
        let target = FakeTarget::new();
        let log = FakeLog::default();

        run_exchange(
            &context(&fixture, Mode::Unlock),
            target.as_ref(),
            transport.as_ref(),
            clock.as_ref(),
            &log,
        );
        // 429 -> 5 s, transport -> 10 s; each round also takes 100 ms.
        assert_eq!(*transport.request_times.borrow(), vec![0, 0, 5_100, 5_100]);
        assert!(log.saw("status=429"));
        assert!(log.saw("error=receive_failed"));
    }

    #[test]
    fn logon_errors_retry_before_the_budget_ends() {
        let fixture = fixture();
        let clock = FakeClock::new(0);
        let transport = FakeTransport::new(
            Rc::clone(&clock),
            vec![
                ok(challenge_body(&fixture), 0),
                status(503, 100),
                ok(challenge_body(&fixture), 0),
                ok(claim_body(&fixture, false, Purpose::Logon), 0),
            ],
        );
        let target = FakeTarget::new();
        let log = FakeLog::default();

        run_exchange(
            &context(&fixture, Mode::Logon),
            target.as_ref(),
            transport.as_ref(),
            clock.as_ref(),
            &log,
        );
        // 5xx waits BACKOFF_INTERVAL_MS (4 s), a denial waits POLL_INTERVAL_MS.
        assert_eq!(*transport.request_times.borrow(), vec![0, 0, 4_100, 4_100]);
    }

    #[test]
    fn repeated_unlock_errors_are_deduped() {
        let fixture = fixture();
        let clock = FakeClock::new(0);
        let mut replies = Vec::new();
        for _ in 0..12 {
            replies.push(ok(challenge_body(&fixture), 0));
            replies.push(Reply::Transport {
                error: "receive_failed",
                elapsed_ms: 0,
            });
        }
        let transport = FakeTransport::new(Rc::clone(&clock), replies);
        let target = FakeTarget::new();
        let log = FakeLog::default();

        run_exchange(
            &context(&fixture, Mode::Unlock),
            target.as_ref(),
            transport.as_ref(),
            clock.as_ref(),
            &log,
        );
        let logged = log
            .lines
            .borrow()
            .iter()
            .filter(|line| line.contains("error=receive_failed"))
            .count();
        // The first error and every tenth repeat; the rest are suppressed.
        assert_eq!(logged, 2);
    }

    #[test]
    fn a_cancelled_claim_ends_the_loop_without_arming() {
        let fixture = fixture();
        let clock = FakeClock::new(0);
        let target = FakeTarget::new();
        let transport = FakeTransport::new(
            Rc::clone(&clock),
            vec![
                ok(challenge_body(&fixture), 5),
                Reply::Cancelled(Rc::clone(&target.cancelled)),
            ],
        );
        let log = FakeLog::default();

        assert_eq!(
            run_exchange(
                &context(&fixture, Mode::Unlock),
                target.as_ref(),
                transport.as_ref(),
                clock.as_ref(),
                &log,
            ),
            LoopEnd::Cancelled
        );
        assert_eq!(target.armed.get(), 0);
        assert_eq!(transport.request_times.borrow().len(), 1);
    }

    #[test]
    fn an_invalid_mac_never_arms() {
        let fixture = fixture();
        let clock = FakeClock::new(0);
        let transport = FakeTransport::new(
            Rc::clone(&clock),
            vec![
                ok(challenge_body(&fixture), 0),
                ok(
                    r#"{"granted":true,"grantId":"g","mac":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="}"#
                        .to_owned(),
                    0,
                ),
            ],
        );
        let target = FakeTarget::new();
        let log = FakeLog::default();

        run_exchange(
            &context(&fixture, Mode::Logon),
            target.as_ref(),
            transport.as_ref(),
            clock.as_ref(),
            &log,
        );
        assert_eq!(target.armed.get(), 0);
        assert!(log.saw("result=invalid_mac"));
    }

    #[test]
    fn arming_waits_for_advise_and_gives_up_without_reclaiming() {
        let fixture = fixture();
        let clock = FakeClock::new(0);
        let transport = FakeTransport::new(
            Rc::clone(&clock),
            vec![
                ok(challenge_body(&fixture), 0),
                ok(claim_body(&fixture, true, Purpose::Logon), 0),
            ],
        );
        let target = FakeTarget::with_arm_results(vec![
            ArmOutcome::Unavailable,
            ArmOutcome::Unavailable,
            ArmOutcome::Armed,
        ]);
        let log = FakeLog::default();

        assert_eq!(
            run_exchange(
                &context(&fixture, Mode::Logon),
                target.as_ref(),
                transport.as_ref(),
                clock.as_ref(),
                &log,
            ),
            LoopEnd::Armed
        );
        assert_eq!(target.armed.get(), 1);
        // The grant is never claimed twice.
        assert_eq!(transport.request_times.borrow().len(), 2);
    }
}
