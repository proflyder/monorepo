//! Narrow Home Assistant REST client shared by the home services.
//!
//! It calls scripts and services, reads one entity state, renders a template
//! and probes `/api/`. The scheduler owns schedule state; Home Assistant only
//! performs the side effect, so nothing here knows about schedules, queues or
//! MCP.
//!
//! The crate is project-agnostic: callers pass entity ids, scripts and
//! variables in. The bearer token is a [`home_secret::Token`] and never appears
//! in `Debug`, an error message or a log line.
//!
//! Failures are classified exactly like `packages/go/httpapi`:
//! [`Error::kind`] is [`ErrorKind::Transient`] for network failures, timeouts,
//! `408`, `425`, `429` and `5xx` (a retried lease may succeed), and
//! [`ErrorKind::Permanent`] for every other `4xx` and for a malformed answer.

use std::fmt;
use std::time::Duration;

use home_secret::Token;
use serde::Deserialize;
use serde_json::{Map, Value};

const STATES_PATH: &str = "/api/states";
const SERVICE_PATH: &str = "/api/services";
const TEMPLATE_PATH: &str = "/api/template";
const PING_PATH: &str = "/api/";
const MAX_STATE_BYTES: usize = 1 << 20;
const MAX_MESSAGE: usize = 200;

/// How a failed call may be retried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// A network failure, timeout, throttling or a `5xx`: retrying later may
    /// succeed.
    Transient,
    /// A `4xx`, a malformed answer or an invalid argument: retrying the same
    /// call cannot succeed.
    Permanent,
}

/// One Home Assistant call failure. It carries no URL, headers or response
/// body: only the operation, a safe status/reason and a bounded message.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{op}: request failed")]
    Transport {
        op: &'static str,
        #[source]
        source: reqwest::Error,
    },
    #[error("{op}: HTTP {status}")]
    Status {
        op: &'static str,
        status: u16,
        message: String,
    },
    #[error("{op}: invalid response")]
    Decode { op: &'static str },
    #[error("{op}: {detail}")]
    Invalid { op: &'static str, detail: String },
}

impl Error {
    /// Whether a retry may succeed.
    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::Transport { .. } => ErrorKind::Transient,
            Self::Status { status, .. } => match *status {
                408 | 425 | 429 => ErrorKind::Transient,
                status if status >= 500 => ErrorKind::Transient,
                _ => ErrorKind::Permanent,
            },
            Self::Decode { .. } | Self::Invalid { .. } => ErrorKind::Permanent,
        }
    }

    /// Convenience predicate for the worker loop.
    #[must_use]
    pub fn is_transient(&self) -> bool {
        self.kind() == ErrorKind::Transient
    }
}

/// A Home Assistant REST client with a fixed base URL and timeout.
#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base_url: String,
    token: Token,
}

impl fmt::Debug for Client {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("home_ha::Client")
            .field("base_url", &self.base_url)
            .field("token", &self.token)
            .finish()
    }
}

impl Client {
    /// Build a client. `base_url` must not contain credentials or a path.
    ///
    /// The workspace builds `reqwest` with rustls and without a default crypto
    /// provider; building a client panics ("No provider set") unless one is
    /// installed. The process-wide ring provider is installed once here; a
    /// provider installed earlier is kept and the error is ignored, so this is
    /// safe to call for every client.
    pub fn new(
        base_url: impl Into<String>,
        token: Token,
        timeout: Duration,
    ) -> Result<Self, Error> {
        let _already_installed = rustls::crypto::ring::default_provider().install_default();
        let http = reqwest::Client::builder()
            .timeout(timeout)
            // An authenticated client must not forward its bearer to a
            // redirect target; a 3xx is surfaced as a status error instead.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|source| Error::Transport {
                op: "build client",
                source,
            })?;
        Ok(Self {
            http,
            base_url: base_url.into().trim().trim_end_matches('/').to_owned(),
            token,
        })
    }

    /// Read the state string of one entity. An unknown entity, a non-object
    /// body or a missing/non-string state is [`Error::Decode`].
    pub async fn state(&self, entity: &str) -> Result<String, Error> {
        const OP: &str = "read state";
        let entity = entity.trim();
        if !valid_entity(entity) {
            return Err(Error::Invalid {
                op: OP,
                detail: "empty or invalid entity".to_owned(),
            });
        }
        let url = format!("{}{STATES_PATH}/{entity}", self.base_url);
        let request = self
            .http
            .get(url)
            .bearer_auth(self.token.expose())
            .build()
            .map_err(|source| Error::Transport { op: OP, source })?;
        let response = self.send(OP, request).await?;
        let body = response
            .bytes()
            .await
            .map_err(|source| Error::Transport { op: OP, source })?;
        if body.len() > MAX_STATE_BYTES {
            return Err(Error::Decode { op: OP });
        }
        let payload: StatePayload =
            serde_json::from_slice(&body).map_err(|_| Error::Decode { op: OP })?;
        payload.state.ok_or(Error::Decode { op: OP })
    }

    /// Invoke `<domain>.<service>` with `data`. The caller decides which call
    /// is allowed; this client performs no allowlisting of its own.
    pub async fn call_service(
        &self,
        domain: &str,
        service: &str,
        data: Map<String, Value>,
    ) -> Result<(), Error> {
        const OP: &str = "call service";
        let (domain, service) = match (valid_identifier(domain), valid_identifier(service)) {
            (true, true) => (domain, service),
            _ => {
                return Err(Error::Invalid {
                    op: OP,
                    detail: "empty or invalid service".to_owned(),
                });
            }
        };
        let url = format!("{SERVICE_PATH}/{domain}/{service}");
        self.post_json(OP, &url, Value::Object(data)).await
    }

    /// Run a script entity with variables. `script` may be a bare script name
    /// or a full `script.<name>` entity id.
    pub async fn call_script(
        &self,
        script: &str,
        variables: Map<String, Value>,
    ) -> Result<(), Error> {
        const OP: &str = "call script";
        let entity = script_entity(script);
        if entity == "script." {
            return Err(Error::Invalid {
                op: OP,
                detail: "empty script entity".to_owned(),
            });
        }
        let mut body = Map::new();
        body.insert("entity_id".to_owned(), Value::String(entity));
        if !variables.is_empty() {
            body.insert("variables".to_owned(), Value::Object(variables));
        }
        self.post_json(
            OP,
            &format!("{SERVICE_PATH}/script/turn_on"),
            Value::Object(body),
        )
        .await
    }

    /// Render a Home Assistant template and return the text. The template is
    /// always built by the caller, never taken from an untrusted client.
    pub async fn render_template(&self, template: &str) -> Result<String, Error> {
        const OP: &str = "render template";
        let mut body = Map::new();
        body.insert("template".to_owned(), Value::String(template.to_owned()));
        let payload = encode(OP, &Value::Object(body))?;
        let request = self
            .http
            .post(format!("{}{TEMPLATE_PATH}", self.base_url))
            .bearer_auth(self.token.expose())
            .header("content-type", "application/json")
            .body(payload)
            .build()
            .map_err(|source| Error::Transport { op: OP, source })?;
        let response = self.send(OP, request).await?;
        response
            .text()
            .await
            .map_err(|source| Error::Transport { op: OP, source })
    }

    /// Verify HTTP reachability and the token with the read-only `/api/`
    /// endpoint.
    pub async fn ping(&self) -> Result<(), Error> {
        const OP: &str = "ping HA";
        let request = self
            .http
            .get(format!("{}{PING_PATH}", self.base_url))
            .bearer_auth(self.token.expose())
            .build()
            .map_err(|source| Error::Transport { op: OP, source })?;
        self.send(OP, request).await.map(|_| ())
    }

    async fn post_json(&self, op: &'static str, path: &str, body: Value) -> Result<(), Error> {
        let payload = encode(op, &body)?;
        let request = self
            .http
            .post(format!("{}{path}", self.base_url))
            .bearer_auth(self.token.expose())
            .header("content-type", "application/json")
            .body(payload)
            .build()
            .map_err(|source| Error::Transport { op, source })?;
        self.send(op, request).await.map(|_| ())
    }

    async fn send(
        &self,
        op: &'static str,
        request: reqwest::Request,
    ) -> Result<reqwest::Response, Error> {
        // The body of a failed response is never read or logged: it may carry
        // entity attributes or other deployment detail.
        let response = self
            .http
            .execute(request)
            .await
            .map_err(|source| Error::Transport { op, source })?;
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status().as_u16();
        let reason = response.status().canonical_reason().unwrap_or("HTTP error");
        Err(Error::Status {
            op,
            status,
            message: sanitize(reason, self.token.expose()),
        })
    }
}

#[derive(Deserialize)]
struct StatePayload {
    #[serde(default)]
    state: Option<String>,
}

fn encode(op: &'static str, value: &Value) -> Result<Vec<u8>, Error> {
    serde_json::to_vec(value).map_err(|_| Error::Invalid {
        op,
        detail: "encode request".to_owned(),
    })
}

/// Normalize a configured script name into a Home Assistant entity id.
#[must_use]
pub fn script_entity(script: &str) -> String {
    let script = script.trim();
    if script.contains('.') {
        script.to_owned()
    } else {
        format!("script.{script}")
    }
}

/// Domain and service names are protocol identifiers, not free text; rejecting
/// anything else keeps the path unambiguous without URL-encoding.
fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

/// Entity ids are `<domain>.<object_id>`; both parts use the Home Assistant
/// slug alphabet. Validating keeps the `/api/states/<entity>` path unambiguous
/// without URL-encoding.
fn valid_entity(entity: &str) -> bool {
    !entity.is_empty()
        && entity
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.'))
}

/// Bound and redact a server-provided message.
fn sanitize(message: &str, token: &str) -> String {
    let mut message = message.trim().to_owned();
    if !token.is_empty() {
        message = message.replace(token, "***");
    }
    if message.len() > MAX_MESSAGE {
        message.truncate(MAX_MESSAGE);
        message.push('…');
    }
    message
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use axum::{
        Json, Router,
        extract::State,
        http::{HeaderMap, StatusCode},
        response::{IntoResponse, Response},
        routing::{get, post},
    };
    use home_secret::Token;
    use serde_json::{Value, json};

    use super::{Client, Error, ErrorKind, script_entity};

    async fn serve(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        format!("http://{address}")
    }

    fn authorized(headers: &HeaderMap) -> bool {
        headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            == Some("Bearer voice-token")
    }

    async fn state_route(headers: HeaderMap) -> Response {
        if !authorized(&headers) {
            return StatusCode::UNAUTHORIZED.into_response();
        }
        Json(json!({"entity_id": "sensor.station", "state": "idle"})).into_response()
    }

    async fn unauthorized() -> StatusCode {
        StatusCode::UNAUTHORIZED
    }

    async fn failure() -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }

    async fn slow() -> &'static str {
        tokio::time::sleep(Duration::from_millis(200)).await;
        "late"
    }

    async fn template(Json(body): Json<Value>) -> String {
        format!("rendered:{}", body["template"].as_str().unwrap_or(""))
    }

    async fn service(
        State(calls): State<std::sync::Arc<std::sync::Mutex<Vec<Value>>>>,
        Json(body): Json<Value>,
    ) -> StatusCode {
        calls.lock().expect("lock").push(body);
        StatusCode::OK
    }

    fn client(base_url: &str) -> Client {
        Client::new(base_url, Token::new("voice-token"), Duration::from_secs(1)).expect("client")
    }

    #[tokio::test]
    async fn ping_accepts_an_authenticated_home_assistant() {
        let app = Router::new().route("/api/", get(|| async { "ok" }));
        let base = serve(app).await;
        client(&base).ping().await.expect("ping");
    }

    #[tokio::test]
    async fn state_reads_the_state_string() {
        let app = Router::new().route("/api/states/sensor.station", get(state_route));
        let base = serve(app).await;
        assert_eq!(
            client(&base).state("sensor.station").await.expect("state"),
            "idle"
        );
    }

    #[tokio::test]
    async fn unauthorized_is_permanent_and_names_only_the_status() {
        let app = Router::new().route("/api/states/sensor.station", get(unauthorized));
        let base = serve(app).await;
        let error = client(&base)
            .state("sensor.station")
            .await
            .expect_err("401");
        assert!(matches!(error, Error::Status { status: 401, .. }));
        assert_eq!(error.kind(), ErrorKind::Permanent);
        let rendered = error.to_string();
        assert!(rendered.contains("401"));
        assert!(!rendered.contains("voice-token"));
    }

    #[tokio::test]
    async fn server_error_is_transient() {
        let app = Router::new().route("/api/", get(failure));
        let base = serve(app).await;
        let error = client(&base).ping().await.expect_err("500");
        assert!(error.is_transient());
    }

    #[tokio::test]
    async fn timeout_is_transient_and_contains_no_token() {
        let app = Router::new().route("/api/", get(slow));
        let base = serve(app).await;
        let client = Client::new(&base, Token::new("voice-token"), Duration::from_millis(50))
            .expect("client");
        let error = client.ping().await.expect_err("timeout");
        assert_eq!(error.kind(), ErrorKind::Transient);
        assert!(!error.to_string().contains("voice-token"));
    }

    #[tokio::test]
    async fn malformed_state_is_permanent() {
        let app = Router::new().route(
            "/api/states/sensor.station",
            get(|| async { Json(json!({"state": 42})) }),
        );
        let base = serve(app).await;
        let error = client(&base)
            .state("sensor.station")
            .await
            .expect_err("decode");
        assert!(matches!(error, Error::Decode { .. }));
        assert_eq!(error.kind(), ErrorKind::Permanent);
    }

    #[tokio::test]
    async fn template_returns_the_rendered_text() {
        let app = Router::new().route("/api/template", post(template));
        let base = serve(app).await;
        assert_eq!(
            client(&base)
                .render_template("1 + 1")
                .await
                .expect("template"),
            "rendered:1 + 1"
        );
    }

    #[tokio::test]
    async fn service_call_posts_the_data_object() {
        let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let app = Router::new()
            .route("/api/services/media_player/play_media", post(service))
            .with_state(calls.clone());
        let base = serve(app).await;
        let mut data = serde_json::Map::new();
        data.insert("entity_id".to_owned(), json!("media_player.station"));
        client(&base)
            .call_service("media_player", "play_media", data)
            .await
            .expect("service");
        let calls = calls.lock().expect("lock");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0]["entity_id"], "media_player.station");
    }

    #[tokio::test]
    async fn script_call_normalizes_the_entity_and_keeps_variables() {
        let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let app = Router::new()
            .route("/api/services/script/turn_on", post(service))
            .with_state(calls.clone());
        let base = serve(app).await;
        let mut variables = serde_json::Map::new();
        variables.insert("message".to_owned(), json!("Готово."));
        client(&base)
            .call_script("announce", variables)
            .await
            .expect("script");
        let calls = calls.lock().expect("lock");
        assert_eq!(calls[0]["entity_id"], "script.announce");
        assert_eq!(calls[0]["variables"]["message"], "Готово.");
    }

    #[tokio::test]
    async fn empty_script_or_invalid_service_is_rejected_before_io() {
        let app = Router::new();
        let base = serve(app).await;
        let client = client(&base);
        let error = client
            .call_script("  ", serde_json::Map::new())
            .await
            .expect_err("empty script");
        assert!(matches!(error, Error::Invalid { .. }));
        let error = client
            .call_service("media player", "play", serde_json::Map::new())
            .await
            .expect_err("invalid domain");
        assert!(matches!(error, Error::Invalid { .. }));
        let error = client
            .state("sensor/../secret")
            .await
            .expect_err("invalid entity");
        assert!(matches!(error, Error::Invalid { .. }));
    }

    #[test]
    fn script_entity_matches_the_go_helper() {
        assert_eq!(script_entity("announce"), "script.announce");
        assert_eq!(script_entity("script.announce"), "script.announce");
        assert_eq!(script_entity("  "), "script.");
    }

    #[test]
    fn debug_never_prints_the_token() {
        let client = Client::new(
            "http://127.0.0.1:1",
            Token::new("voice-token"),
            Duration::from_secs(1),
        )
        .expect("client");
        let rendered = format!("{client:?}");
        assert!(!rendered.contains("voice-token"));
        assert!(rendered.contains("Token(***)"));
    }
}
