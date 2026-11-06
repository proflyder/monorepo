//! Minimal Connect-JSON client for `ricardo.v1.SchedulerService`.
//!
//! It covers only the durable event surface used by executors and producers:
//! `EnqueueTask`, `ListDueEvents`, `AckEvent` and `GetStatus`. It never owns
//! schedule state, never mutates a due event itself and never interprets a
//! payload. The generated request/response types are exposed unchanged so a
//! caller sees exactly the proto fields.

use std::time::Duration;

use connectrpc::client::{ClientConfig, HttpClient};
use connectrpc::{CodecFormat, ConnectError, ErrorCode, Protocol};
use home_secret::Token;
use http::Uri;

use crate::v1::{
    AckEventRequest, AckOutcome, DueEvent, EnqueueTaskRequest, GetStatusRequest, GetStatusResponse,
    ListDueEventsRequest, SchedulerServiceClient,
};

/// The proto limit for `AckEventRequest.detail`.
const MAX_ACK_DETAIL: usize = 1024;

/// A scheduler call failure. `Debug` and `Display` never print the bearer:
/// the Connect message is redacted against the configured token and bounded.
#[derive(Debug, thiserror::Error)]
pub enum SchedulerError {
    #[error("scheduler: invalid base URL")]
    InvalidUrl,
    #[error("scheduler: {code:?}: {message}")]
    Rpc { code: ErrorCode, message: String },
}

impl SchedulerError {
    /// Whether a retry after a lease expiry can reasonably succeed.
    #[must_use]
    pub fn is_transient(&self) -> bool {
        match self {
            Self::InvalidUrl => false,
            Self::Rpc { code, .. } => matches!(
                code,
                ErrorCode::Unavailable
                    | ErrorCode::DeadlineExceeded
                    | ErrorCode::Internal
                    | ErrorCode::ResourceExhausted
                    | ErrorCode::Canceled
                    | ErrorCode::Unknown
            ),
        }
    }

    /// Convenience predicate for the worker loop.
    #[must_use]
    pub fn is_permanent(&self) -> bool {
        !self.is_transient()
    }
}

/// Thin typed scheduler client with a fixed bearer and timeout.
#[derive(Clone)]
pub struct SchedulerClient {
    inner: SchedulerServiceClient<HttpClient>,
    token: Token,
}

impl std::fmt::Debug for SchedulerClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("home_proto::SchedulerClient")
            .field("token", &self.token)
            .finish()
    }
}

impl SchedulerClient {
    /// Build a client. `base_url` is the scheduler base
    /// (`http://127.0.0.1:8098`); a trailing `/ricardo.v1.SchedulerService`
    /// is tolerated, like the Go client.
    pub fn new(base_url: &str, token: Token, timeout: Duration) -> Result<Self, SchedulerError> {
        let base = base_url.trim().trim_end_matches('/');
        let base = base
            .strip_suffix("/ricardo.v1.SchedulerService")
            .unwrap_or(base);
        let uri: Uri = base.parse().map_err(|_| SchedulerError::InvalidUrl)?;
        if uri.scheme_str().is_none() || uri.authority().is_none() {
            return Err(SchedulerError::InvalidUrl);
        }
        let config = ClientConfig::new(uri)
            .with_protocol(Protocol::Connect)
            .with_codec_format(CodecFormat::Json)
            .with_default_timeout(timeout)
            .with_default_header("authorization", format!("Bearer {}", token.expose()));
        Ok(Self {
            inner: SchedulerServiceClient::new(HttpClient::plaintext(), config),
            token,
        })
    }

    /// Store one durable generic task event and return the stored event.
    pub async fn enqueue_task(
        &self,
        request: EnqueueTaskRequest,
    ) -> Result<DueEvent, SchedulerError> {
        let response = self
            .inner
            .enqueue_task(request)
            .await
            .map_err(|error| self.map_error(&error))?;
        let owned = response.into_owned();
        Ok(owned.event.into_option().unwrap_or_default())
    }

    /// Lease due events of one queue. The caller must always set `queue`: an
    /// empty value means "all queues" on the server.
    pub async fn list_due_events(
        &self,
        request: ListDueEventsRequest,
    ) -> Result<Vec<DueEvent>, SchedulerError> {
        let response = self
            .inner
            .list_due_events(request)
            .await
            .map_err(|error| self.map_error(&error))?;
        Ok(response.into_owned().events)
    }

    /// Report a terminal outcome. `detail` is truncated to the proto limit.
    ///
    /// A replay of a terminal ack is idempotent on the server: it returns the
    /// already-known event instead of failing.
    pub async fn ack_event(
        &self,
        event_id: &str,
        consumer_id: &str,
        outcome: AckOutcome,
        detail: &str,
    ) -> Result<DueEvent, SchedulerError> {
        let request = AckEventRequest {
            event_id: event_id.to_owned(),
            consumer_id: consumer_id.to_owned(),
            outcome: outcome.into(),
            detail: truncate_detail(detail),
            ..Default::default()
        };
        let response = self
            .inner
            .ack_event(request)
            .await
            .map_err(|error| self.map_error(&error))?;
        Ok(response
            .into_owned()
            .event
            .into_option()
            .unwrap_or_default())
    }

    /// Read scheduler state without leasing anything.
    pub async fn get_status(&self) -> Result<GetStatusResponse, SchedulerError> {
        let response = self
            .inner
            .get_status(GetStatusRequest::default())
            .await
            .map_err(|error| self.map_error(&error))?;
        Ok(response.into_owned())
    }

    fn map_error(&self, error: &ConnectError) -> SchedulerError {
        SchedulerError::Rpc {
            code: error.code,
            message: sanitize(
                error.message.as_deref().unwrap_or_default(),
                self.token.expose(),
            ),
        }
    }
}

fn truncate_detail(detail: &str) -> String {
    let detail = detail.trim();
    if detail.chars().count() <= MAX_ACK_DETAIL {
        return detail.to_owned();
    }
    detail.chars().take(MAX_ACK_DETAIL).collect()
}

fn sanitize(message: &str, token: &str) -> String {
    let mut message = message.trim().to_owned();
    if !token.is_empty() {
        message = message.replace(token, "***");
    }
    if message.chars().count() > 200 {
        message = message.chars().take(200).collect::<String>() + "…";
    }
    message
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use axum::{
        Json, Router,
        body::Bytes,
        extract::State,
        http::{HeaderMap, StatusCode, Uri, header},
        response::{IntoResponse, Response},
        routing::post,
    };
    use home_secret::Token;
    use serde_json::{Value, json};

    use super::{SchedulerClient, SchedulerError, truncate_detail};
    use crate::v1::{
        AckOutcome, DueEvent, EnqueueTaskRequest, GetStatusRequest, ListDueEventsRequest,
    };

    #[derive(Default)]
    struct Calls {
        paths: Vec<String>,
        auth: Vec<String>,
        bodies: Vec<Value>,
    }

    async fn rpc(
        State(calls): State<Arc<Mutex<Calls>>>,
        uri: Uri,
        headers: HeaderMap,
        body: Bytes,
    ) -> Response {
        let path = uri.path().to_owned();
        let auth = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let value: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
        {
            let mut calls = calls.lock().expect("lock");
            calls.paths.push(path.clone());
            calls.auth.push(auth);
            calls.bodies.push(value);
        }
        let payload = if path.ends_with("/EnqueueTask") {
            json!({"event": event_json()})
        } else if path.ends_with("/ListDueEvents") {
            json!({"events": [event_json()]})
        } else if path.ends_with("/AckEvent") {
            json!({"event": event_json()})
        } else if path.ends_with("/GetStatus") {
            json!({"version": "test", "nowUnix": "1730000000", "ready": true})
        } else {
            return StatusCode::NOT_FOUND.into_response();
        };
        ([(header::CONTENT_TYPE, "application/json")], Json(payload)).into_response()
    }

    fn event_json() -> Value {
        json!({
            "id": "ev_1",
            "taskType": "test.ping",
            "queue": "tasks",
            "payloadJson": "{}",
            "state": "EVENT_STATE_LEASED",
            "action": "EVENT_ACTION_FIRE"
        })
    }

    async fn serve() -> (String, Arc<Mutex<Calls>>) {
        let calls = Arc::new(Mutex::new(Calls::default()));
        let app = Router::new().fallback(post(rpc)).with_state(calls.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        (format!("http://{address}"), calls)
    }

    fn client(base: &str) -> SchedulerClient {
        SchedulerClient::new(base, Token::new("sched-token"), Duration::from_secs(1))
            .expect("client")
    }

    #[tokio::test]
    async fn enqueue_posts_connect_json_with_bearer() {
        let (base, calls) = serve().await;
        let event = client(&base)
            .enqueue_task(EnqueueTaskRequest {
                task_type: "test.ping".to_owned(),
                payload_json: "{}".to_owned(),
                queue: "tasks".to_owned(),
                ..Default::default()
            })
            .await
            .expect("enqueue");
        assert_eq!(event.id, "ev_1");
        let calls = calls.lock().expect("lock");
        assert_eq!(
            calls.paths,
            vec!["/ricardo.v1.SchedulerService/EnqueueTask"]
        );
        assert_eq!(calls.auth, vec!["Bearer sched-token"]);
        assert_eq!(calls.bodies[0]["taskType"], "test.ping");
        assert_eq!(calls.bodies[0]["queue"], "tasks");
    }

    #[tokio::test]
    async fn list_due_events_sends_the_queue_and_decodes_the_event() {
        let (base, calls) = serve().await;
        let events = client(&base)
            .list_due_events(ListDueEventsRequest {
                consumer_id: "executor".to_owned(),
                lease_seconds: 60,
                limit: 10,
                queue: "home".to_owned(),
                ..Default::default()
            })
            .await
            .expect("list");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, "ev_1");
        assert_eq!(events[0].task_type, "test.ping");
        let calls = calls.lock().expect("lock");
        assert_eq!(calls.bodies[0]["consumerId"], "executor");
        assert_eq!(calls.bodies[0]["queue"], "home");
        assert_eq!(calls.bodies[0]["leaseSeconds"], 60);
    }

    #[tokio::test]
    async fn ack_and_status_round_trip() {
        let (base, calls) = serve().await;
        let client = client(&base);
        let event = client
            .ack_event(
                "ev_1",
                "executor",
                AckOutcome::ACK_OUTCOME_DELIVERED,
                "done",
            )
            .await
            .expect("ack");
        assert_eq!(event.id, "ev_1");
        let status = client.get_status().await.expect("status");
        assert_eq!(status.version, "test");
        assert!(status.ready);
        let calls = calls.lock().expect("lock");
        assert_eq!(calls.bodies[0]["eventId"], "ev_1");
        assert_eq!(calls.bodies[0]["outcome"], "ACK_OUTCOME_DELIVERED");
        assert_eq!(calls.bodies[0]["detail"], "done");
        assert_eq!(calls.bodies[1], json!({}));
    }

    #[tokio::test]
    async fn base_url_may_carry_the_service_suffix() {
        let (base, _calls) = serve().await;
        let client = SchedulerClient::new(
            &format!("{base}/ricardo.v1.SchedulerService/"),
            Token::new("sched-token"),
            Duration::from_secs(1),
        )
        .expect("client");
        client.get_status().await.expect("status");
    }

    #[tokio::test]
    async fn unauthorized_is_permanent_and_does_not_leak_the_token() {
        async fn deny() -> StatusCode {
            StatusCode::UNAUTHORIZED
        }
        let app = Router::new().fallback(post(deny));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        let client = SchedulerClient::new(
            &format!("http://{address}"),
            Token::new("sched-token"),
            Duration::from_secs(1),
        )
        .expect("client");
        let error = client.get_status().await.expect_err("401");
        assert!(matches!(error, SchedulerError::Rpc { .. }));
        assert!(error.is_permanent());
        assert!(!error.to_string().contains("sched-token"));
    }

    #[test]
    fn invalid_url_is_rejected() {
        let error = SchedulerClient::new(
            "not a url",
            Token::new("sched-token"),
            Duration::from_secs(1),
        )
        .expect_err("invalid");
        assert!(matches!(error, SchedulerError::InvalidUrl));
    }

    #[test]
    fn ack_detail_is_bounded_to_the_proto_limit() {
        let long = "я".repeat(2000);
        assert_eq!(truncate_detail(&long).chars().count(), 1024);
        assert_eq!(truncate_detail("  done  "), "done");
    }

    #[test]
    fn debug_never_prints_the_token() {
        let client = SchedulerClient::new(
            "http://127.0.0.1:1",
            Token::new("sched-token"),
            Duration::from_secs(1),
        )
        .expect("client");
        assert!(!format!("{client:?}").contains("sched-token"));
    }

    // Keep the generated types compiled in this test target even when the
    // integration tests are ignored.
    #[allow(dead_code)]
    fn _types(_: DueEvent, _: GetStatusRequest) {}
}
