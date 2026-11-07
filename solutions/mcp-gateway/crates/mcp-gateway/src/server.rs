//! HTTP surface: authenticated MCP, the separate voice bearer, health,
//! readiness and the Prometheus endpoint.
//!
//! `rmcp`'s Streamable HTTP transport is wrapped in a thin compatibility
//! layer for the two observable Go-SDK behaviours the frozen fixtures pin:
//!
//! * every SSE frame carries the explicit `event: message` line (rmcp writes
//!   the implicit-event form `data: …`), and
//! * the Go error envelopes for a missing session, a wrong content type and a
//!   non-initialize request without a session.
//!
//! Everything else (tool results, resources, auth) is the SDK's own output.

use std::collections::BTreeMap;
use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Instant;

use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, Method, Request, Response, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::IntoResponse;
use axum::routing::{any, any_service, get};
use http_body_util::BodyExt as _;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use serde_json::Value;
use tower::Service;

use crate::mcp::GatewayHandler;
use crate::metrics::SharedMetrics;
use crate::tools::{Deps, ToolGroup};

/// The MCP bearer realm.
pub const MCP_REALM: &str = "mcp-gateway";
/// The voice bearer realm.
pub const VOICE_REALM: &str = "mcp-gateway-voice";

const MAX_MCP_BODY: usize = 4 << 20;

/// Everything the HTTP surface needs.
pub struct Options {
    /// MCP bearer token.
    pub auth_token: String,
    /// Voice bearer token; `None` disables `/v1/voice/*` like the Go server.
    pub voice_token: Option<String>,
    /// Voice routes mounted behind the separate bearer; `None` keeps a stub.
    pub voice: Option<Router<ObserveState>>,
    /// Enabled tool groups.
    pub groups: Vec<Arc<dyn ToolGroup>>,
    /// Shared tool dependencies.
    pub deps: Arc<Deps>,
    /// Gateway metrics.
    pub metrics: SharedMetrics,
    /// Reported implementation version.
    pub version: String,
    /// Readiness flag.
    pub ready: bool,
}

#[derive(Clone)]
struct Auth {
    token: String,
    realm: &'static str,
}

/// Builds the complete axum router.
pub fn router(options: Options) -> Router {
    let handler = GatewayHandler::new(Arc::clone(&options.deps), &options.groups, options.version);
    let config = StreamableHttpServerConfig::default()
        .disable_allowed_hosts()
        .with_json_response(false);
    let service = StreamableHttpService::new(
        move || Ok(handler.clone()),
        Arc::new(LocalSessionManager::default()),
        config,
    );

    let mcp = Router::new()
        .route("/mcp", any_service(McpService { inner: service }))
        .route_layer(middleware::from_fn_with_state(
            Auth {
                token: options.auth_token,
                realm: MCP_REALM,
            },
            require_bearer,
        ));

    let mut app = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics))
        .merge(mcp);

    let state = ObserveState {
        metrics: options.metrics,
        ready: options.ready,
    };

    if let Some(token) = options.voice_token {
        let voice = options
            .voice
            .unwrap_or_else(|| Router::new().route("/v1/voice/{*rest}", any(voice_stub)));
        let voice = voice.route_layer(middleware::from_fn_with_state(
            Auth {
                token,
                realm: VOICE_REALM,
            },
            require_bearer,
        ));
        app = app.merge(voice);
    }

    app.layer(middleware::from_fn_with_state(state.clone(), observe))
        .with_state(state)
}

/// Shared middleware state of the HTTP surface.
#[derive(Clone)]
pub struct ObserveState {
    metrics: SharedMetrics,
    ready: bool,
}

async fn healthz() -> Response<Body> {
    json_response(StatusCode::OK, &serde_json::json!({"status": "ok"}))
}

async fn readyz(State(state): State<ObserveState>) -> Response<Body> {
    if state.ready {
        json_response(StatusCode::OK, &serde_json::json!({"status": "ready"}))
    } else {
        json_response(
            StatusCode::SERVICE_UNAVAILABLE,
            &serde_json::json!({"status": "unavailable"}),
        )
    }
}

async fn metrics(State(state): State<ObserveState>) -> Response<Body> {
    let body = state.metrics.render();
    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )
        .body(Body::from(body))
        .expect("valid response")
}

async fn voice_stub() -> Response<Body> {
    json_response(
        StatusCode::NOT_FOUND,
        &serde_json::json!({"error": "voice route not found"}),
    )
}

fn json_response(status: StatusCode, value: &Value) -> Response<Body> {
    let body = serde_json::to_vec(value).expect("serializing a JSON response");
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .expect("valid response")
}

fn text_response(status: StatusCode, body: &'static str) -> Response<Body> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from(body))
        .expect("valid response")
}

async fn require_bearer(
    State(auth): State<Auth>,
    request: Request<Body>,
    next: Next,
) -> Response<Body> {
    let presented = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .unwrap_or_default();
    if presented.is_empty() || !constant_time_eq(presented, &auth.token) {
        tracing::warn!(
            path = %request.uri().path(),
            realm = auth.realm,
            "missing or invalid bearer token"
        );
        return Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .header(
                header::WWW_AUTHENTICATE,
                format!("Bearer realm=\"{}\"", auth.realm),
            )
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"error":"unauthorized"}"#))
            .expect("valid response");
    }
    next.run(request).await
}

/// Constant-time byte comparison without an extra dependency.
fn constant_time_eq(left: &str, right: &str) -> bool {
    let left = left.as_bytes();
    let right = right.as_bytes();
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for (a, b) in left.iter().zip(right) {
        difference |= a ^ b;
    }
    difference == 0
}

async fn observe(
    State(state): State<ObserveState>,
    request: Request<Body>,
    next: Next,
) -> Response<Body> {
    let started = Instant::now();
    let path = request.uri().path().to_owned();
    let response = next.run(request).await;
    let status = response.status().as_u16();
    let route = route_of(&path);
    state.metrics.observe_http(route, status);
    let level = if status >= 400 {
        tracing::Level::WARN
    } else {
        tracing::Level::INFO
    };
    let _ = level;
    if status >= 400 {
        tracing::warn!(
            event = "http_request",
            http_path = route,
            http_status = status,
            duration_ms = started.elapsed().as_millis() as u64,
            "http request"
        );
    } else {
        tracing::info!(
            event = "http_request",
            http_path = route,
            http_status = status,
            duration_ms = started.elapsed().as_millis() as u64,
            "http request"
        );
    }
    response
}

fn route_of(path: &str) -> &str {
    match path {
        "/mcp" => "/mcp",
        "/v1/voice/utterance" => "/v1/voice/utterance",
        "/v1/voice/result" => "/v1/voice/result",
        "/v1/voice/updates" => "/v1/voice/updates",
        "/v1/voice/state" => "/v1/voice/state",
        "/healthz" => "/healthz",
        "/readyz" => "/readyz",
        "/metrics" => "/metrics",
        _ => "other",
    }
}

// --- Streamable HTTP compatibility wrapper --------------------------------

#[derive(Clone)]
struct McpService {
    inner: StreamableHttpService<GatewayHandler, LocalSessionManager>,
}

impl Service<Request<Body>> for McpService {
    type Response = Response<Body>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Response<Body>, Infallible>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Infallible>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let mut inner = self.inner.clone();
        Box::pin(async move {
            let method = request.method().clone();
            let headers = request.headers().clone();

            if method == Method::POST {
                let Some(content_type) = headers
                    .get(header::CONTENT_TYPE)
                    .and_then(|value| value.to_str().ok())
                else {
                    return Ok(text_response(
                        StatusCode::UNSUPPORTED_MEDIA_TYPE,
                        "Content-Type must be 'application/json'\n",
                    ));
                };
                let essence = content_type
                    .split_once(';')
                    .map_or(content_type, |(value, _)| value)
                    .trim();
                if !essence.eq_ignore_ascii_case("application/json") {
                    return Ok(text_response(
                        StatusCode::UNSUPPORTED_MEDIA_TYPE,
                        "Content-Type must be 'application/json'\n",
                    ));
                }

                let (parts, body) = request.into_parts();
                let Ok(bytes) = axum::body::to_bytes(body, MAX_MCP_BODY).await else {
                    return Ok(text_response(StatusCode::BAD_REQUEST, "Bad Request\n"));
                };
                let has_session = parts.headers.contains_key("mcp-session-id");
                let message: Option<Value> = serde_json::from_slice(&bytes).ok();
                let method_name = message
                    .as_ref()
                    .and_then(|value| value.get("method"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                if !has_session
                    && !method_name.is_empty()
                    && method_name != "initialize"
                    && let Some(id) = message
                        .as_ref()
                        .and_then(|value| value.get("id"))
                        .filter(|value| !value.is_null())
                {
                    let text = format!(
                        "event: message\ndata: {{\"jsonrpc\":\"2.0\",\"id\":{},\"error\":{{\"code\":0,\"message\":{}}}}}\n\n",
                        serde_json::to_string(id).unwrap_or_else(|_| "null".to_owned()),
                        serde_json::to_string(&format!(
                            "method {method_name:?} is invalid during session initialization"
                        ))
                        .unwrap_or_else(|_| "\"\"".to_owned())
                    );
                    return Ok(sse_response(text));
                }

                let mut parts = parts;
                if !parts.headers.contains_key(header::HOST) {
                    parts
                        .headers
                        .insert(header::HOST, "localhost".parse().expect("host"));
                }
                let request = Request::from_parts(parts, Body::from(bytes));
                let response = inner.call(request).await.expect("infallible service");
                let (parts, body) = response.into_parts();
                if is_event_stream(&parts.headers) {
                    let bytes = match body.collect().await {
                        Ok(collected) => collected.to_bytes(),
                        Err(_) => {
                            return Ok(text_response(StatusCode::BAD_GATEWAY, "Bad Gateway\n"));
                        }
                    };
                    let text = String::from_utf8_lossy(&bytes);
                    return Ok(Response::from_parts(
                        parts,
                        Body::from(explicit_event_message(&text)),
                    ));
                }
                return Ok(Response::from_parts(parts, Body::new(body)));
            }

            if method == Method::GET && !headers.contains_key("mcp-session-id") {
                return Ok(text_response(
                    StatusCode::BAD_REQUEST,
                    "Bad Request: GET requires an Mcp-Session-Id header\n",
                ));
            }

            let mut request = request;
            if !request.headers().contains_key(header::HOST) {
                request
                    .headers_mut()
                    .insert(header::HOST, "localhost".parse().expect("host"));
            }
            let response = inner.call(request).await.expect("infallible service");
            if method == Method::DELETE {
                let (mut parts, body) = response.into_parts();
                let _ = body.collect().await;
                parts.status = StatusCode::NO_CONTENT;
                return Ok(Response::from_parts(parts, Body::empty()));
            }
            let (parts, body) = response.into_parts();
            Ok(Response::from_parts(parts, Body::new(body)))
        })
    }
}

fn is_event_stream(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("text/event-stream"))
}

/// Adds the explicit `event: message` line the Go SDK writes.
///
/// The SDK also emits a `id:`/`retry:` primed keep-alive frame on POST
/// responses; the Go SDK sends only the message frame, so metadata-only frames
/// are dropped and a trailing `id:` line is not forwarded.
fn explicit_event_message(body: &str) -> String {
    let mut out = String::with_capacity(body.len() + 16);
    for frame in body.split_inclusive("\n\n") {
        let content = frame.strip_suffix("\n\n").unwrap_or(frame);
        let mut data = String::new();
        for line in content.lines() {
            if let Some(rest) = line.strip_prefix("data:") {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(rest.strip_prefix(' ').unwrap_or(rest));
            }
        }
        if data.is_empty() {
            continue;
        }
        out.push_str("event: message\ndata: ");
        out.push_str(&data);
        out.push_str("\n\n");
    }
    if out.is_empty() {
        out.push_str(body);
    }
    out
}

fn sse_response(body: String) -> Response<Body> {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header("X-Accel-Buffering", "no")
        .body(Body::from(body))
        .expect("valid response")
}

/// Unused import guard for `BTreeMap`; kept out of the public surface.
#[allow(dead_code)]
fn _unused(_: &BTreeMap<String, String>) {}

#[allow(dead_code)]
fn _into_response(_: impl IntoResponse) {}

#[cfg(test)]
mod compat_tests {
    use super::explicit_event_message;

    #[test]
    fn drops_the_priming_frame() {
        let body = "\nid: 0\nretry: 3000\n\nevent: message\ndata: {\"a\":1}\n\n";
        assert_eq!(
            explicit_event_message(body),
            "event: message\ndata: {\"a\":1}\n\n"
        );
    }
}
