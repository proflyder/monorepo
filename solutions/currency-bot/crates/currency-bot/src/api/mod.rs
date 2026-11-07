//! axum HTTP layer: the internal JSON API and the dedicated webhook listener.
//!
//! The routes, bodies and the error envelope are the Go service's. The
//! undocumented `ServeMux` catch-all (`GET /` answering every unknown path) is
//! deliberately *not* carried over: an unknown route is a 404.

mod error;
mod history;
mod json;
mod webhook;

use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use axum::extract::{Request, State};
use axum::http::header;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use home_secret::{Secret, Token};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::domain::clock::Clock;
use crate::domain::texts;
use crate::metrics::Metrics;
use crate::pipeline::{BoxFuture, MessageSender, Pipeline};
use crate::store::Store;

pub use error::ErrorEnvelope;
pub use webhook::handle_webhook;

/// The webhook body cap, matching Go's `maxWebhook`.
pub const MAX_WEBHOOK_BYTES: usize = 1 << 20;

/// Readiness probe used by `/readyz`.
pub trait Readiness: Send + Sync {
    fn check(&self) -> BoxFuture<'static, Result<(), String>>;
}

/// The production readiness check: only the SQLite store.
pub struct StoreReadiness {
    store: Arc<Store>,
}

impl StoreReadiness {
    #[must_use]
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }
}

impl Readiness for StoreReadiness {
    fn check(&self) -> BoxFuture<'static, Result<(), String>> {
        let store = Arc::clone(&self.store);
        Box::pin(async move { store.ping().map_err(|error| error.to_string()) })
    }
}

/// Always-ready stub used by contract scenarios without a store override.
pub struct AlwaysReady;

impl Readiness for AlwaysReady {
    fn check(&self) -> BoxFuture<'static, Result<(), String>> {
        Box::pin(async { Ok(()) })
    }
}

/// Always-unavailable stub for the readiness fixture.
pub struct NeverReady;

impl Readiness for NeverReady {
    fn check(&self) -> BoxFuture<'static, Result<(), String>> {
        Box::pin(async { Err("database is unavailable".to_owned()) })
    }
}

/// Size of the dedup window; a long-running process cannot grow unbounded.
const UPDATE_WINDOW: usize = 1024;

/// Sliding-window deduplication of Telegram `update_id`s.
#[derive(Debug, Default)]
pub struct UpdateDedup {
    seen: HashSet<i64>,
    ring: VecDeque<i64>,
}

impl UpdateDedup {
    /// Record `update_id`; `false` means it was already seen.
    pub fn mark_seen(&mut self, update_id: i64) -> bool {
        if update_id == 0 {
            return true;
        }
        if self.seen.contains(&update_id) {
            return false;
        }
        while self.ring.len() >= UPDATE_WINDOW {
            if let Some(oldest) = self.ring.pop_front() {
                self.seen.remove(&oldest);
            }
        }
        self.seen.insert(update_id);
        self.ring.push_back(update_id);
        true
    }
}

/// Everything the two listeners need.
pub struct ApiState {
    pub token: Token,
    pub webhook_secret: Secret,
    pub allowed_chats: Vec<String>,
    pub default_chat: String,
    pub pipeline: Arc<Pipeline>,
    pub store: Arc<Store>,
    pub sender: Arc<dyn MessageSender>,
    pub metrics: Arc<Metrics>,
    pub clock: Arc<dyn Clock>,
    pub readiness: Arc<dyn Readiness>,
    pub tasks: TaskTracker,
    pub cancel: CancellationToken,
    pub dedup: Mutex<UpdateDedup>,
    /// Run a webhook `/trigger` inline instead of acknowledging first. Tests
    /// use it to observe the effect deterministically; production keeps the
    /// acknowledge-first behaviour.
    pub dispatch_inline: bool,
}

impl std::fmt::Debug for ApiState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ApiState")
            .field("allowed_chats", &self.allowed_chats)
            .field("default_chat", &self.default_chat)
            .finish_non_exhaustive()
    }
}

impl ApiState {
    /// Whether a chat id is allowed to command the bot.
    #[must_use]
    pub fn is_allowed_chat(&self, chat_id: i64) -> bool {
        let chat_id = chat_id.to_string();
        self.allowed_chats.contains(&chat_id)
    }

    /// Returns `true` when the update is new.
    pub fn mark_update_seen(&self, update_id: i64) -> bool {
        self.dedup
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .mark_seen(update_id)
    }
}

/// The internal API listener.
pub fn api_router(state: Arc<ApiState>) -> Router {
    let protected = Router::new()
        .route(
            "/api/history",
            get(history::history).delete(history::delete_history),
        )
        .route("/api/latest", get(history::latest))
        .route("/api/trigger", post(history::trigger))
        .route_layer(middleware::from_fn_with_state(
            Arc::clone(&state),
            require_auth,
        ));
    let open = Router::new()
        .route("/", get(root))
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics));
    open.merge(protected)
        .layer(middleware::from_fn_with_state(Arc::clone(&state), observe))
        .with_state(state)
}

/// The webhook listener: exactly one route.
pub fn webhook_router(state: Arc<ApiState>) -> Router {
    Router::new()
        .route("/telegram/webhook", post(handle_webhook))
        .layer(middleware::from_fn_with_state(Arc::clone(&state), observe))
        .with_state(state)
}

async fn root() -> Response {
    (
        [(header::CONTENT_TYPE, "text/plain; charset=UTF-8")],
        texts::ROOT_BODY,
    )
        .into_response()
}

async fn healthz() -> Response {
    Json(serde_json::json!({ "status": "ok" })).into_response()
}

async fn readyz(State(state): State<Arc<ApiState>>) -> Response {
    match state.readiness.check().await {
        Ok(()) => Json(serde_json::json!({ "status": "ready" })).into_response(),
        Err(error) => {
            tracing::error!(%error, "readiness check failed");
            (
                http::StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "status": "unavailable" })),
            )
                .into_response()
        }
    }
}

async fn metrics(State(state): State<Arc<ApiState>>) -> Response {
    (
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        state.metrics.encode(),
    )
        .into_response()
}

/// Constant-time `Authorization: Bearer <API_TOKEN>`.
async fn require_auth(
    State(state): State<Arc<ApiState>>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_owned();
    let authorization = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    let authorized = authorization
        .map(str::trim)
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .is_some_and(|token| !token.is_empty() && state.token.ct_eq(token));
    if !authorized {
        tracing::warn!(path, "missing or invalid api token");
        return error::unauthorized(&state, &path);
    }
    next.run(request).await
}

/// Log every request and count it by the matched route pattern (never the raw
/// path, which a scanner could grow without bound).
async fn observe(State(state): State<Arc<ApiState>>, request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let started = Instant::now();
    let response = next.run(request).await;
    let status = response.status().as_u16();
    let route = matched_route(&path);
    state.metrics.observe_http(route, status);
    let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    if status >= 400 {
        tracing::warn!(
            event = "http_request",
            http_method = %method,
            http_path = path,
            http_status = status,
            duration_ms,
            "http request"
        );
    } else {
        tracing::info!(
            event = "http_request",
            http_method = %method,
            http_path = path,
            http_status = status,
            duration_ms,
            "http request"
        );
    }
    response
}

/// Label by the route template, never the raw path: a scanner hitting random
/// URLs must not create unbounded series. Unknown paths collapse to `other`.
fn matched_route(path: &str) -> &'static str {
    match path {
        "/" => "/",
        "/healthz" => "/healthz",
        "/readyz" => "/readyz",
        "/metrics" => "/metrics",
        "/api/history" => "/api/history",
        "/api/latest" => "/api/latest",
        "/api/trigger" => "/api/trigger",
        "/telegram/webhook" => "/telegram/webhook",
        _ => "other",
    }
}
