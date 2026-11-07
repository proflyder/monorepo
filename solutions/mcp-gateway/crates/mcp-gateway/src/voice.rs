//! The `/v1/voice/*` HTTP surface.
//!
//! Ports `solutions/mcp-gateway/internal/voice/voice.go`. The routes are:
//!
//! * `POST /v1/voice/utterance` — route one spoken phrase;
//! * `POST /v1/voice/result` — long-poll the answer of a bb turn;
//! * `POST /v1/voice/updates` — poll dispatcher turns the owner did not start;
//! * `GET  /v1/voice/state` — debug view of the agent-conversation switch.
//!
//! The surface is authenticated by a dedicated voice bearer, never the MCP
//! token, and is intentionally separate from `/mcp`. Bodies are decoded with
//! unknown fields rejected, exactly like the Go server; the typed wire
//! contract lives in `packages/rust/voice-contract`.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Extension};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get, post};
use serde::de::DeserializeOwned;
use serde_json::json;
use voice_contract::{
    ResultRequest, ResultResponse, Route, StateResponse, UpdateItem, UpdatesRequest,
    UpdatesResponse, UtteranceRequest, UtteranceResponse,
};

use crate::bb::{
    self, BbApi, BbFuture, Error as BbError, SendRequest, SendResult, TurnResultRequest,
    TurnResultResult, UpdateItem as BbUpdateItem, UpdatesRequest as BbUpdatesRequest,
    UpdatesResult, speech_for_error,
};
use crate::domain::clock::Clock;
use crate::domain::state::AgentMode;
use crate::metrics::SharedMetrics;

/// Accepted profiles.
pub const PROFILE_PERSONAL: &str = "personal";
/// Accepted profiles.
pub const PROFILE_WORK: &str = "work";

/// Speech budget accepted from the proxy, in Unicode scalar values.
pub const MAX_TEXT_RUNES: usize = 2000;

/// The long-poll ceiling; the proxy must stay below the satellite watchdog.
pub const DEFAULT_MAX_WAIT: Duration = Duration::from_secs(25);

/// Default plugin wait: one bridge call must stay below the 15 s SSH timeout.
pub const DEFAULT_UPDATES_WAIT: Duration = Duration::from_secs(10);

/// Maximum accepted request body, matching the Go decoder.
pub const MAX_BODY_BYTES: usize = 16 << 10;

/// Phrases spoken by the proxy path.
pub mod speech {
    /// A phrase was delivered to the agent thread.
    pub const AGENT_SENT: &str = "Передал агенту.";
    /// A phrase was delivered to the work thread.
    pub const WORK_SENT: &str = "Передал в рабочий тред.";
    /// Agent mode was turned off.
    pub const MODE_OFF: &str = "Выключил режим агента.";
    /// The work dispatcher thread is not configured.
    pub const WORK_MISSING: &str = "Рабочий тред не настроен.";
    /// The utterance limiter rejected the phrase.
    pub const RATE_LIMITED: &str = "Слишком часто, подожди немного.";
    /// The request could not be decoded.
    pub const BAD_REQUEST: &str = "Не понял запрос.";
    /// The turn failed.
    pub const TURN_FAILED: &str = "Агент завершил работу с ошибкой.";
    /// The agent waits for input.
    pub const WAITING_INPUT: &str = "Агент ждёт ответа.";
    /// The turn is still running.
    pub const TURN_RUNNING: &str = "Агент ещё работает.";
    /// The turn finished without a spoken line.
    pub const DONE: &str = "Готово.";
}

/// Exit phrases end agent conversation mode. They match a whole utterance.
const EXIT_PHRASES: [&str; 3] = [
    "выйди из режима агента",
    "выключи режим агента",
    "закончи разговор с агентом",
];

/// Derives the plugin wait from the SSH bridge timeout.
///
/// One `updates` call must finish before the bridge times out, otherwise every
/// long-poll returns `status: "failed"` when `MCP_BB_TIMEOUT` is short. One
/// second of headroom is left, capped at [`DEFAULT_UPDATES_WAIT`].
#[must_use]
pub fn updates_wait_for(bridge_timeout: Duration) -> Duration {
    let wait = bridge_timeout.saturating_sub(Duration::from_secs(1));
    wait.clamp(Duration::from_secs(1), DEFAULT_UPDATES_WAIT)
}

/// The bb subset the voice route needs.
pub trait Dispatcher: Send + Sync {
    /// Delivers one phrase.
    fn send(&self, request: SendRequest) -> BbFuture<'_, Result<SendResult, BbError>>;
    /// Reads one turn's outcome.
    fn turn_result(
        &self,
        request: TurnResultRequest,
    ) -> BbFuture<'_, Result<TurnResultResult, BbError>>;
    /// Polls dispatcher updates.
    fn updates(&self, request: BbUpdatesRequest) -> BbFuture<'_, Result<UpdatesResult, BbError>>;
}

impl<T: BbApi + ?Sized> Dispatcher for T {
    fn send(&self, request: SendRequest) -> BbFuture<'_, Result<SendResult, BbError>> {
        BbApi::send(self, request)
    }

    fn turn_result(
        &self,
        request: TurnResultRequest,
    ) -> BbFuture<'_, Result<TurnResultResult, BbError>> {
        BbApi::turn_result(self, request)
    }

    fn updates(&self, request: BbUpdatesRequest) -> BbFuture<'_, Result<UpdatesResult, BbError>> {
        BbApi::updates(self, request)
    }
}

/// Adapts a whole [`BbApi`] bridge to the voice [`Dispatcher`] subset.
///
/// Rust does not coerce between unrelated trait objects, so the composition
/// root wraps the shared bridge in this zero-cost adapter.
#[derive(Clone)]
pub struct BbDispatcher {
    inner: Arc<dyn BbApi>,
}

impl BbDispatcher {
    /// Wraps one bridge.
    #[must_use]
    pub fn new(inner: Arc<dyn BbApi>) -> Self {
        Self { inner }
    }
}

impl Dispatcher for BbDispatcher {
    fn send(&self, request: SendRequest) -> BbFuture<'_, Result<SendResult, BbError>> {
        self.inner.send(request)
    }

    fn turn_result(
        &self,
        request: TurnResultRequest,
    ) -> BbFuture<'_, Result<TurnResultResult, BbError>> {
        self.inner.turn_result(request)
    }

    fn updates(&self, request: BbUpdatesRequest) -> BbFuture<'_, Result<UpdatesResult, BbError>> {
        self.inner.updates(request)
    }
}

/// A global utterance limiter.
pub trait Limiter: Send + Sync {
    /// Whether the phrase may be accepted now.
    fn allow(&self) -> bool;
}

/// Accepts every phrase; used by tests and by disabled limits.
#[derive(Debug, Clone, Copy, Default)]
pub struct AllowAll;

impl Limiter for AllowAll {
    fn allow(&self) -> bool {
        true
    }
}

#[derive(Debug)]
struct BucketState {
    tokens: f64,
    last: Instant,
}

/// Token bucket: `interval` per token, up to `burst` tokens.
///
/// The default is 30 utterances per minute with burst 10 — enough for a
/// conversation and far below anything that could spawn or spam threads.
#[derive(Debug)]
pub struct TokenBucket {
    interval: Duration,
    burst: f64,
    state: Mutex<BucketState>,
}

impl TokenBucket {
    /// Builds a bucket with a refill interval and a capacity.
    #[must_use]
    pub fn new(interval: Duration, burst: u32) -> Self {
        let burst = f64::from(burst.max(1));
        Self {
            interval,
            burst,
            state: Mutex::new(BucketState {
                tokens: burst,
                last: Instant::now(),
            }),
        }
    }

    /// The production limiter: one utterance per two seconds, burst 10.
    #[must_use]
    pub fn production() -> Self {
        Self::new(Duration::from_secs(2), 10)
    }
}

impl Limiter for TokenBucket {
    fn allow(&self) -> bool {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let now = Instant::now();
        let elapsed = now.duration_since(state.last).as_secs_f64();
        let interval = self.interval.as_secs_f64();
        if interval > 0.0 {
            state.tokens = (state.tokens + elapsed / interval).min(self.burst);
        }
        state.last = now;
        if state.tokens >= 1.0 {
            state.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Everything the voice routes need.
pub struct Options {
    /// bb dispatcher.
    pub dispatcher: Arc<dyn Dispatcher>,
    /// Permanent work dispatcher thread; empty fails closed.
    pub work_thread_id: String,
    /// Agent conversation switch.
    pub mode: Arc<AgentMode>,
    /// Gateway metrics.
    pub metrics: SharedMetrics,
    /// Wall clock.
    pub clock: Arc<dyn Clock>,
    /// Utterance limiter.
    pub limiter: Arc<dyn Limiter>,
    /// Result poll interval.
    pub poll_interval: Duration,
    /// Result long-poll ceiling.
    pub max_wait: Duration,
    /// One `updates` plugin wait.
    pub updates_wait: Duration,
}

struct Handler {
    dispatcher: Arc<dyn Dispatcher>,
    work_thread_id: String,
    mode: Arc<AgentMode>,
    metrics: SharedMetrics,
    clock: Arc<dyn Clock>,
    limiter: Arc<dyn Limiter>,
    poll_interval: Duration,
    max_wait: Duration,
    updates_wait: Duration,
}

impl Handler {
    fn new(options: Options) -> Self {
        Self {
            dispatcher: options.dispatcher,
            work_thread_id: options.work_thread_id.trim().to_owned(),
            mode: options.mode,
            metrics: options.metrics,
            clock: options.clock,
            limiter: options.limiter,
            poll_interval: if options.poll_interval.is_zero() {
                Duration::from_secs(1)
            } else {
                options.poll_interval
            },
            max_wait: if options.max_wait.is_zero() {
                DEFAULT_MAX_WAIT
            } else {
                options.max_wait
            },
            updates_wait: if options.updates_wait.is_zero() {
                DEFAULT_UPDATES_WAIT
            } else {
                options.updates_wait
            },
        }
    }

    fn observe(&self, route: &str, profile: &str, outcome: &str, thread_id: &str, chars: usize) {
        self.metrics.observe_voice(route, outcome);
        // Never log the spoken text: only its size and routing metadata.
        tracing::info!(
            event = "voice_utterance",
            route = route,
            profile = profile,
            outcome = outcome,
            thread_id = thread_id,
            chars = chars,
            "voice utterance"
        );
    }
}

/// Builds the voice router. Authentication is applied by the HTTP surface.
///
/// The router is generic over the outer state so the composition root can
/// merge it into the stateful gateway router; the handler is shared through an
/// [`Extension`].
pub fn router<S>(options: Options) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let handler = Arc::new(Handler::new(options));
    Router::<S>::new()
        .route("/v1/voice/utterance", post(utterance))
        .route("/v1/voice/result", post(result))
        .route("/v1/voice/updates", post(updates))
        .route("/v1/voice/state", get(state))
        .route("/v1/voice/{*rest}", any(not_found))
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(Extension(handler))
}

async fn not_found() -> Response {
    StatusCode::NOT_FOUND.into_response()
}

async fn utterance(Extension(handler): Extension<Arc<Handler>>, body: Bytes) -> Response {
    let Ok(request) = decode::<UtteranceRequest>(&body) else {
        return bad_request();
    };
    let text = request.text.trim().to_owned();
    let chars = text.chars().count();
    if text.is_empty() || chars > MAX_TEXT_RUNES {
        return write(
            StatusCode::BAD_REQUEST,
            UtteranceResponse::local(speech::BAD_REQUEST),
        );
    }
    let Some(profile) = normalize_profile(&request.profile) else {
        return write(
            StatusCode::BAD_REQUEST,
            UtteranceResponse::local(speech::BAD_REQUEST),
        );
    };
    if !handler.limiter.allow() {
        handler.observe("utterance", profile, "rate_limited", "", chars);
        return write(
            StatusCode::TOO_MANY_REQUESTS,
            UtteranceResponse {
                route: Route::Bb,
                ok: None,
                thread_id: None,
                after_seq: None,
                speech: Some(speech::RATE_LIMITED.to_owned()),
            },
        );
    }

    if is_exit(&text) {
        handler.mode.exit();
        handler.observe("exit", profile, "success", "", chars);
        return write(StatusCode::OK, UtteranceResponse::local(speech::MODE_OFF));
    }

    if profile == PROFILE_WORK {
        if handler.work_thread_id.is_empty() {
            handler.observe("work", profile, "failure", "", chars);
            return write(
                StatusCode::OK,
                UtteranceResponse {
                    route: Route::Bb,
                    ok: None,
                    thread_id: None,
                    after_seq: None,
                    speech: Some(speech::WORK_MISSING.to_owned()),
                },
            );
        }
        let thread_id = handler.work_thread_id.clone();
        return send(
            &handler,
            &thread_id,
            &text,
            profile,
            "work",
            speech::WORK_SENT,
            chars,
        )
        .await;
    }

    // Agent conversation mode is a personal-profile feature. Work phrases
    // always go to the work dispatcher; the mode stays on.
    if let Some(thread_id) = handler.mode.active() {
        return send(
            &handler,
            &thread_id,
            &text,
            profile,
            "agent",
            speech::AGENT_SENT,
            chars,
        )
        .await;
    }

    handler.observe("hermes", profile, "success", "", chars);
    write(StatusCode::OK, UtteranceResponse::hermes())
}

/// Delivers one phrase to bb. Failures always stay on the bb route: a
/// work-profile phrase must never fall through to Hermes.
async fn send(
    handler: &Handler,
    thread_id: &str,
    text: &str,
    profile: &str,
    route: &str,
    speech_line: &str,
    chars: usize,
) -> Response {
    let idempotency_key = format!("voice-{}", handler.clock.now().as_nanosecond());
    let result = handler
        .dispatcher
        .send(SendRequest {
            thread_id: thread_id.to_owned(),
            text: text.to_owned(),
            idempotency_key,
        })
        .await;
    match result {
        Err(error) => {
            handler.observe(route, profile, "failure", thread_id, chars);
            write(
                StatusCode::OK,
                UtteranceResponse {
                    route: Route::Bb,
                    ok: Some(false),
                    thread_id: Some(thread_id.to_owned()),
                    after_seq: None,
                    speech: Some(speech_for_error(&error).to_owned()),
                },
            )
        }
        Ok(result) => {
            handler.observe(route, profile, "success", thread_id, chars);
            write(
                StatusCode::OK,
                UtteranceResponse {
                    route: Route::Bb,
                    ok: Some(true),
                    thread_id: Some(thread_id.to_owned()),
                    after_seq: Some(result.after_seq),
                    speech: Some(speech_line.to_owned()),
                },
            )
        }
    }
}

async fn result(Extension(handler): Extension<Arc<Handler>>, body: Bytes) -> Response {
    let Ok(request) = decode::<ResultRequest>(&body) else {
        return bad_request();
    };
    if request.thread_id.trim().is_empty() || request.after_seq < 0 {
        return write(
            StatusCode::BAD_REQUEST,
            ResultResponse {
                status: "failed".to_owned(),
                speech: Some(speech::BAD_REQUEST.to_owned()),
                text: None,
                completed_seq: None,
            },
        );
    }

    let max_wait = wait_seconds(handler.max_wait);
    let wait = if request.wait_seconds <= 0 {
        max_wait
    } else {
        i64::from(request.wait_seconds).min(max_wait)
    };
    let deadline = handler
        .clock
        .now()
        .as_nanosecond()
        .saturating_add(i128::from(wait) * 1_000_000_000);

    loop {
        let outcome = handler
            .dispatcher
            .turn_result(TurnResultRequest {
                thread_id: request.thread_id.clone(),
                after_seq: request.after_seq,
            })
            .await;
        let turn = match outcome {
            Ok(turn) => turn,
            Err(error) => {
                handler.observe("result", "", "failure", &request.thread_id, 0);
                return write(
                    StatusCode::OK,
                    ResultResponse {
                        status: "failed".to_owned(),
                        speech: Some(speech_for_error(&error).to_owned()),
                        text: None,
                        completed_seq: None,
                    },
                );
            }
        };
        if turn.status == "running" || turn.status == "starting" {
            if handler.clock.now().as_nanosecond() >= deadline {
                handler.observe("result", "", "running", &request.thread_id, 0);
                return write(
                    StatusCode::OK,
                    ResultResponse {
                        status: "running".to_owned(),
                        speech: None,
                        text: None,
                        completed_seq: None,
                    },
                );
            }
            tokio::time::sleep(handler.poll_interval).await;
            continue;
        }
        handler.observe("result", "", &turn.status, &request.thread_id, 0);
        let text = (!turn.text.is_empty()).then(|| turn.text.clone());
        return write(
            StatusCode::OK,
            ResultResponse {
                status: turn.status.clone(),
                speech: Some(turn_speech(&turn)),
                text,
                completed_seq: turn.completed_seq,
            },
        );
    }
}

async fn updates(Extension(handler): Extension<Arc<Handler>>, body: Bytes) -> Response {
    let Ok(request) = decode::<UpdatesRequest>(&body) else {
        return bad_request();
    };
    if request.after_seq.is_some_and(|after| after < 0) {
        return write(
            StatusCode::BAD_REQUEST,
            UpdatesResponse {
                cursor: None,
                items: Vec::new(),
                status: Some("failed".to_owned()),
                speech: Some(speech::BAD_REQUEST.to_owned()),
            },
        );
    }

    let mut wait = wait_seconds(handler.max_wait);
    if let Some(requested) = request.wait_seconds
        && requested > 0
    {
        wait = i64::from(requested).min(wait);
    }
    // One SSH bridge call must stay below the bridge timeout; the proxy uses
    // the returned cursor and calls again, so a shorter answer is fine.
    wait = wait.min(wait_seconds(handler.updates_wait));

    let outcome = handler
        .dispatcher
        .updates(BbUpdatesRequest {
            after_seq: request.after_seq,
            wait_seconds: Some(i32::try_from(wait).unwrap_or(i32::MAX)),
        })
        .await;
    match outcome {
        Err(error) => {
            handler.observe("updates", "", "failure", "", 0);
            write(
                StatusCode::OK,
                UpdatesResponse {
                    cursor: None,
                    items: Vec::new(),
                    status: Some("failed".to_owned()),
                    speech: Some(speech_for_error(&error).to_owned()),
                },
            )
        }
        Ok(result) => {
            handler.observe("updates", "", "ok", "", 0);
            write(
                StatusCode::OK,
                UpdatesResponse {
                    cursor: Some(result.cursor),
                    items: result.items.into_iter().map(update_item).collect(),
                    status: None,
                    speech: None,
                },
            )
        }
    }
}

async fn state(Extension(handler): Extension<Arc<Handler>>) -> Response {
    let thread_id = handler.mode.active();
    write(
        StatusCode::OK,
        StateResponse {
            agent_mode: thread_id.is_some(),
            thread_id,
        },
    )
}

fn update_item(item: BbUpdateItem) -> UpdateItem {
    UpdateItem {
        seq: item.seq,
        spoken: item.spoken,
        completed_at: item.completed_at,
    }
}

fn turn_speech(turn: &TurnResultResult) -> String {
    let spoken = turn
        .spoken
        .as_deref()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    match turn.status.as_str() {
        "failed" => speech::TURN_FAILED.to_owned(),
        "waiting_input" => spoken.map_or_else(|| speech::WAITING_INPUT.to_owned(), str::to_owned),
        _ => spoken.map_or_else(|| speech::DONE.to_owned(), str::to_owned),
    }
}

fn wait_seconds(duration: Duration) -> i64 {
    i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
}

/// Maps the proxy's Russian or English value onto the two profiles. An unknown
/// value is rejected (fail closed).
fn normalize_profile(raw: &str) -> Option<&'static str> {
    match bb::normalize(raw).as_str() {
        "" | "personal" | "личный" | "личный профиль" => Some(PROFILE_PERSONAL),
        "work" | "рабочий" | "рабочий профиль" => Some(PROFILE_WORK),
        _ => None,
    }
}

/// Matches a whole utterance, not a fragment, so a task that merely mentions
/// agents is never mistaken for the exit command.
fn is_exit(text: &str) -> bool {
    let normalized = bb::normalize(text);
    EXIT_PHRASES.contains(&normalized.as_str())
}

fn decode<T: DeserializeOwned>(body: &[u8]) -> Result<T, ()> {
    serde_json::from_slice(body).map_err(|_| ())
}

fn bad_request() -> Response {
    write_json(
        StatusCode::BAD_REQUEST,
        &json!({"error": "invalid request"}),
    )
}

fn write<T: serde::Serialize>(status: StatusCode, value: T) -> Response {
    let body = serde_json::to_vec(&value).expect("serializing a voice response");
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(body))
        .expect("valid response")
}

fn write_json(status: StatusCode, value: &serde_json::Value) -> Response {
    let body = serde_json::to_vec(value).expect("serializing a JSON response");
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(body))
        .expect("valid response")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updates_wait_leaves_headroom_and_caps_at_ten_seconds() {
        assert_eq!(
            updates_wait_for(Duration::from_secs(15)),
            Duration::from_secs(10)
        );
        assert_eq!(
            updates_wait_for(Duration::from_secs(5)),
            Duration::from_secs(4)
        );
        assert_eq!(
            updates_wait_for(Duration::from_secs(1)),
            Duration::from_secs(1)
        );
        assert_eq!(
            updates_wait_for(Duration::from_secs(20)),
            Duration::from_secs(10)
        );
    }

    #[test]
    fn profiles_and_exit_phrases_follow_the_go_rules() {
        assert_eq!(normalize_profile("Личный"), Some(PROFILE_PERSONAL));
        assert_eq!(normalize_profile("Рабочий профиль"), Some(PROFILE_WORK));
        assert_eq!(normalize_profile("чужой"), None);
        assert!(is_exit("Выключи режим агента!"));
        assert!(!is_exit("не выходи из режима агента"));
    }
}
