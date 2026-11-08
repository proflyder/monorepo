//! Minimal Connect-JSON client for `home.v1.ScenarioService`.
//!
//! The scenario executor only reads definitions: it never creates, edits or
//! enables a scenario. `RunScenario` is a producer-side call and stays out of
//! this client on purpose, so an executor cannot queue more work for itself.

use std::time::Duration;

use connectrpc::client::ClientConfig;
use connectrpc::{CodecFormat, ConnectError, ErrorCode, Protocol};
use home_secret::Token;
use http::Uri;

use crate::transport::ReqwestTransport;
use crate::v1::{GetScenarioRequest, Scenario, ScenarioServiceClient};

/// A scenario call failure. `Debug` and `Display` never print the bearer: the
/// Connect message is redacted against the configured token and bounded.
#[derive(Debug, thiserror::Error)]
pub enum ScenarioError {
    #[error("scenario: invalid base URL")]
    InvalidUrl,
    #[error("scenario: cannot build the HTTP client")]
    Transport,
    #[error("scenario: unusable CA")]
    Certificate,
    #[error("scenario: {code:?}: {message}")]
    Rpc { code: ErrorCode, message: String },
}

impl ScenarioError {
    /// Whether a retry after a lease expiry can reasonably succeed.
    #[must_use]
    pub fn is_transient(&self) -> bool {
        match self {
            Self::InvalidUrl | Self::Transport | Self::Certificate => false,
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

    /// Convenience predicate for the executor loop.
    #[must_use]
    pub fn is_permanent(&self) -> bool {
        !self.is_transient()
    }

    /// `GetScenario` returned `NOT_FOUND`.
    #[must_use]
    pub fn is_not_found(&self) -> bool {
        matches!(
            self,
            Self::Rpc {
                code: ErrorCode::NotFound,
                ..
            }
        )
    }
}

/// Thin typed scenario client with a fixed bearer and timeout.
#[derive(Clone)]
pub struct ScenarioClient {
    inner: ScenarioServiceClient<ReqwestTransport>,
    token: Token,
}

impl std::fmt::Debug for ScenarioClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("home_proto::ScenarioClient")
            .field("token", &self.token)
            .finish()
    }
}

impl ScenarioClient {
    /// Build a client. `base_url` is the scheduler base
    /// (`http://127.0.0.1:8098`); a trailing service suffix is tolerated.
    pub fn new(base_url: &str, token: Token, timeout: Duration) -> Result<Self, ScenarioError> {
        Self::build(base_url, token, timeout, || {
            ReqwestTransport::plaintext().map_err(|_| ScenarioError::Transport)
        })
    }

    /// Build a client that trusts the given PEM bundle in addition to the
    /// transport's roots (the workspace reqwest compiles none in).
    pub fn new_with_ca_pem(
        base_url: &str,
        token: Token,
        timeout: Duration,
        ca_pem: &[u8],
    ) -> Result<Self, ScenarioError> {
        Self::build(base_url, token, timeout, || {
            ReqwestTransport::with_ca_pem(ca_pem).map_err(|_| ScenarioError::Certificate)
        })
    }

    /// Like [`ScenarioClient::new_with_ca_pem`], but reads the bundle from
    /// `path`. The file content is never logged.
    pub fn new_with_ca_file(
        base_url: &str,
        token: Token,
        timeout: Duration,
        path: impl AsRef<std::path::Path>,
    ) -> Result<Self, ScenarioError> {
        let pem = std::fs::read(path).map_err(|_| ScenarioError::Certificate)?;
        Self::new_with_ca_pem(base_url, token, timeout, &pem)
    }

    fn build(
        base_url: &str,
        token: Token,
        timeout: Duration,
        transport: impl FnOnce() -> Result<ReqwestTransport, ScenarioError>,
    ) -> Result<Self, ScenarioError> {
        let base = base_url.trim().trim_end_matches('/');
        let base = base
            .strip_suffix("/home.v1.ScenarioService")
            .or_else(|| base.strip_suffix("/home.v1.SchedulerService"))
            .unwrap_or(base);
        let uri: Uri = base.parse().map_err(|_| ScenarioError::InvalidUrl)?;
        if uri.scheme_str().is_none() || uri.authority().is_none() {
            return Err(ScenarioError::InvalidUrl);
        }
        let transport = transport()?;
        let config = ClientConfig::new(uri)
            .with_protocol(Protocol::Connect)
            .with_codec_format(CodecFormat::Json)
            .with_default_timeout(timeout)
            .with_default_header("authorization", format!("Bearer {}", token.expose()));
        Ok(Self {
            inner: ScenarioServiceClient::new(transport, config),
            token,
        })
    }

    /// Read one definition. `NOT_FOUND` maps to `Ok(None)` so the executor can
    /// acknowledge a stale `scenario.fire` without treating it as an outage.
    pub async fn get_scenario(&self, id: &str) -> Result<Option<Scenario>, ScenarioError> {
        let response = self
            .inner
            .get_scenario(GetScenarioRequest {
                id: id.to_owned(),
                ..Default::default()
            })
            .await
            .map_err(|error| self.map_error(&error));
        match response {
            Ok(response) => Ok(response.into_owned().scenario.into_option()),
            Err(error) if error.is_not_found() => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn map_error(&self, error: &ConnectError) -> ScenarioError {
        ScenarioError::Rpc {
            code: error.code,
            message: sanitize(
                error.message.as_deref().unwrap_or_default(),
                self.token.expose(),
            ),
        }
    }
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

    use super::{ScenarioClient, ScenarioError};

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
        if path.ends_with("/GetScenario") {
            return (
                [(header::CONTENT_TYPE, "application/json")],
                Json(json!({"scenario": {"id": "sc_1", "label": "evening", "enabled": true}})),
            )
                .into_response();
        }
        StatusCode::NOT_FOUND.into_response()
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

    fn client(base: &str) -> ScenarioClient {
        ScenarioClient::new(base, Token::new("sched-token"), Duration::from_secs(1))
            .expect("client")
    }

    #[tokio::test]
    async fn get_scenario_posts_connect_json_with_bearer() {
        let (base, calls) = serve().await;
        let scenario = client(&base)
            .get_scenario("sc_1")
            .await
            .expect("get")
            .expect("found");
        assert_eq!(scenario.id, "sc_1");
        assert_eq!(scenario.label, "evening");
        assert!(scenario.enabled);
        let calls = calls.lock().expect("lock");
        assert_eq!(calls.paths, vec!["/home.v1.ScenarioService/GetScenario"]);
        assert_eq!(calls.auth, vec!["Bearer sched-token"]);
        assert_eq!(calls.bodies[0]["id"], "sc_1");
    }

    #[tokio::test]
    async fn not_found_is_none_and_other_errors_carry_the_code() {
        async fn missing() -> StatusCode {
            StatusCode::NOT_FOUND
        }
        let app = Router::new().fallback(post(missing));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        // A plain HTTP 404 is not a Connect NOT_FOUND envelope, so the client
        // reports an RPC error instead of inventing an empty definition.
        let error = client(&format!("http://{address}"))
            .get_scenario("sc_1")
            .await
            .expect_err("not a connect envelope");
        assert!(matches!(error, ScenarioError::Rpc { .. }));
        assert!(!error.to_string().contains("sched-token"));
    }

    #[tokio::test]
    async fn base_url_may_carry_either_service_suffix() {
        let (base, _calls) = serve().await;
        for suffix in ["/home.v1.ScenarioService", "/home.v1.SchedulerService"] {
            let client = ScenarioClient::new(
                &format!("{base}{suffix}/"),
                Token::new("sched-token"),
                Duration::from_secs(1),
            )
            .expect("client");
            client.get_scenario("sc_1").await.expect("get");
        }
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
        let client = client(&format!("http://{address}"));
        let error = client.get_scenario("sc_1").await.expect_err("401");
        assert!(matches!(error, ScenarioError::Rpc { .. }));
        assert!(error.is_permanent());
        assert!(!error.to_string().contains("sched-token"));
    }

    #[test]
    fn invalid_url_is_rejected() {
        let error = ScenarioClient::new(
            "not a url",
            Token::new("sched-token"),
            Duration::from_secs(1),
        )
        .expect_err("invalid");
        assert!(matches!(error, ScenarioError::InvalidUrl));
    }

    #[test]
    fn debug_never_prints_the_token() {
        let client = client("http://127.0.0.1:1");
        assert!(!format!("{client:?}").contains("sched-token"));
    }
}
