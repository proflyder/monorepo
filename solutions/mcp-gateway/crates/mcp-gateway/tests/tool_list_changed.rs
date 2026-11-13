//! Contract for `notifications/tools/list_changed` on a fresh MCP session.
//!
//! A client that reconnects after a gateway restart keeps its old tool registry
//! unless the server announces a change, so every initialized session must
//! receive exactly one `notifications/tools/list_changed` on the standalone GET
//! stream — no matter whether the client opens that stream before or after it
//! posts `notifications/initialized`.
//!
//! The test drives the real router in-process (no socket) and reads the GET
//! response body as the client would: an SSE frame stream.

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, Response, StatusCode, header};
use futures::StreamExt;
use jiff::Timestamp;
use mcp_gateway::domain::clock::{Clock, FixedClock};
use mcp_gateway::domain::state::{AgentMode, KeyedStore, PendingStore};
use mcp_gateway::metrics::Metrics;
use mcp_gateway::server;
use mcp_gateway::tools::infra_read::ReadGroup;
use mcp_gateway::tools::{Deps, TimerScheduler, ToolGroup};
use tower::ServiceExt;

const TOKEN: &str = "mcp-contract-token";
const SESSION_HEADER: &str = "mcp-session-id";
const INITIALIZE: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"tool-list-test","version":"1.0.0"}}}"#;
const INITIALIZED: &str = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
const TOOLS_LIST: &str = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;

const LIST_CHANGED: &str = "notifications/tools/list_changed";

fn deps() -> Arc<Deps> {
    let clock: Arc<dyn Clock> = Arc::new(FixedClock::new(
        "2026-11-01T12:00:00Z".parse::<Timestamp>().expect("clock"),
    ));
    Arc::new(Deps {
        catalog: None,
        prom: None,
        alerts: None,
        loki: None,
        kube: None,
        report: None,
        checks: None,
        pending: Arc::new(PendingStore::new(
            Duration::from_secs(90),
            Arc::clone(&clock),
        )),
        timers: Arc::new(TimerScheduler),
        metrics: Arc::new(Metrics::new()),
        clock: Arc::clone(&clock),
        location: jiff::tz::TimeZone::UTC,
        timezone_name: "UTC".to_owned(),
        scheduler: None,
        push: None,
        cooldown: Duration::from_secs(300),
        self_restart_delay: Duration::from_secs(20),
        query_timeout: Duration::from_secs(1),
        status_timeout: Duration::from_secs(2),
        report_timeout: Duration::from_secs(2),
        log_byte_cap: Arc::new(AtomicUsize::new(64 << 10)),
        bb: None,
        bb_targets: None,
        bb_pending: Arc::new(KeyedStore::new(Duration::from_secs(90), clock)),
        bb_mode: Arc::new(AgentMode::new()),
    })
}

fn app() -> Router {
    let deps = deps();
    let groups: Vec<Arc<dyn ToolGroup>> = vec![Arc::new(ReadGroup::new(Arc::clone(&deps)))];
    server::router(server::Options {
        auth_token: TOKEN.to_owned(),
        voice_token: None,
        voice: None,
        groups,
        deps,
        metrics: Arc::new(Metrics::new()),
        version: "contract".to_owned(),
        ready: true,
    })
}

fn post(session: Option<&str>, body: &str) -> Request<Body> {
    let mut builder = Request::post("/mcp")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/json, text/event-stream");
    if let Some(session) = session {
        builder = builder.header(SESSION_HEADER, session);
    }
    builder
        .body(Body::from(body.to_owned()))
        .expect("request builds")
}

fn get(session: &str) -> Request<Body> {
    Request::get("/mcp")
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .header(header::ACCEPT, "text/event-stream")
        .header(SESSION_HEADER, session)
        .body(Body::empty())
        .expect("request builds")
}

async fn initialize(app: &Router) -> String {
    let response = app
        .clone()
        .oneshot(post(None, INITIALIZE))
        .await
        .expect("service is infallible");
    assert_eq!(response.status(), StatusCode::OK, "initialize succeeds");
    let session = response
        .headers()
        .get(SESSION_HEADER)
        .and_then(|value| value.to_str().ok())
        .expect("initialize returns a session id")
        .to_owned();
    let body = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("initialize body");
    let text = String::from_utf8_lossy(&body);
    assert!(text.contains("\"result\""), "initialize result: {text}");
    session
}

async fn notify_initialized(app: &Router, session: &str) {
    let response = app
        .clone()
        .oneshot(post(Some(session), INITIALIZED))
        .await
        .expect("service is infallible");
    assert_eq!(
        response.status(),
        StatusCode::ACCEPTED,
        "initialized notification is accepted"
    );
    let _ = axum::body::to_bytes(response.into_body(), 1 << 20).await;
}

/// Opens the standalone SSE stream and returns its response.
async fn open_stream(app: &Router, session: &str) -> Response<Body> {
    let response = app
        .clone()
        .oneshot(get(session))
        .await
        .expect("service is infallible");
    assert_eq!(response.status(), StatusCode::OK, "GET opens the stream");
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert!(
        content_type.starts_with("text/event-stream"),
        "GET returns SSE, got {content_type:?}"
    );
    response
}

/// Reads the SSE stream and counts `notifications/tools/list_changed` frames.
///
/// The first frame has to arrive within [`FIRST_FRAME_WAIT`]; after that the
/// stream is watched for [`DUPLICATE_WAIT`] more to prove the notification was
/// sent once.
async fn count_list_changed(body: Body) -> usize {
    const FIRST_FRAME_WAIT: Duration = Duration::from_secs(3);
    const DUPLICATE_WAIT: Duration = Duration::from_millis(500);

    let mut stream = body.into_data_stream();
    let mut deadline = Instant::now() + FIRST_FRAME_WAIT;
    let mut buffer = String::new();
    let mut count = 0usize;
    loop {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        let Ok(Some(Ok(chunk))) = tokio::time::timeout(deadline - now, stream.next()).await else {
            break;
        };
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(end) = buffer.find("\n\n") {
            let frame: String = buffer.drain(..end + 2).collect();
            if frame.contains(LIST_CHANGED) {
                count += 1;
                if count == 1 {
                    deadline = std::cmp::min(deadline, Instant::now() + DUPLICATE_WAIT);
                }
            }
        }
    }
    assert!(
        count > 0,
        "no {LIST_CHANGED} frame within {FIRST_FRAME_WAIT:?}"
    );
    count
}

async fn tools_list_still_works(app: &Router, session: &str) {
    let response = app
        .clone()
        .oneshot(post(Some(session), TOOLS_LIST))
        .await
        .expect("service is infallible");
    assert_eq!(response.status(), StatusCode::OK, "tools/list works");
    let body = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("tools/list body");
    let text = String::from_utf8_lossy(&body);
    assert!(
        text.contains("\"tools\"") && !text.contains("\"error\""),
        "tools/list result: {text}"
    );
}

#[tokio::test]
async fn list_changed_reaches_a_stream_opened_after_initialized() {
    let app = app();
    let session = initialize(&app).await;
    notify_initialized(&app, &session).await;
    let stream = open_stream(&app, &session).await;
    assert_eq!(count_list_changed(stream.into_body()).await, 1);
    tools_list_still_works(&app, &session).await;
}

#[tokio::test]
async fn list_changed_reaches_a_stream_opened_before_initialized() {
    let app = app();
    let session = initialize(&app).await;
    let stream = open_stream(&app, &session).await;
    notify_initialized(&app, &session).await;
    assert_eq!(count_list_changed(stream.into_body()).await, 1);
    tools_list_still_works(&app, &session).await;
}
