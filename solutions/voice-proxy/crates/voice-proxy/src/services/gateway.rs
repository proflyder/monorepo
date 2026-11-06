//! The mcp-gateway voice client: ask where an utterance belongs, long-poll a
//! routed turn's result and the dispatcher updates.
//!
//! The request bodies, paths and timeouts are the wire contract:
//!
//! * utterance: 5 s (`HERMES_VOICE_GATEWAY_TIMEOUT`);
//! * result/updates: the gateway timeout plus the long-poll wait, because the
//!   server only answers when the wait expires;
//! * a non-object JSON response is treated as an empty object.

use std::time::Duration;

use serde_json::Value;
use voice_contract::{
    ResultRequest, ResultResponse, UpdatesRequest, UpdatesResponse, UtteranceRequest,
    UtteranceResponse,
};

use super::TokenProvider;
use crate::domain::profile::Profile;

/// Paths of the voice API.
pub const UTTERANCE_PATH: &str = "/v1/voice/utterance";
pub const RESULT_PATH: &str = "/v1/voice/result";
pub const UPDATES_PATH: &str = "/v1/voice/updates";

/// One gateway call failure. It never contains the bearer or a response body.
#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    #[error("gateway URL is not configured")]
    NotConfigured,
    #[error("gateway request failed: {0}")]
    Request(#[source] reqwest::Error),
    #[error("gateway returned HTTP {0}")]
    Status(u16),
    #[error("gateway response is not valid JSON")]
    Decode,
}

/// Calls the mcp-gateway voice routes.
pub struct Gateway {
    base_url: String,
    timeout: Duration,
    token: TokenProvider,
    client: reqwest::Client,
}

impl std::fmt::Debug for Gateway {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Gateway")
            .field("base_url", &self.base_url)
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

impl Gateway {
    /// Builds a client. See [`crate::relay::Upstream::new`] for the ring
    /// provider requirement.
    ///
    /// # Errors
    ///
    /// Returns a request error when the HTTP client cannot be built.
    pub fn new(
        base_url: &str,
        token: TokenProvider,
        timeout: Duration,
    ) -> Result<Self, GatewayError> {
        let _already_installed = rustls::crypto::ring::default_provider().install_default();
        let timeout = if timeout.is_zero() {
            Duration::from_secs(5)
        } else {
            timeout
        };
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(GatewayError::Request)?;
        Ok(Self {
            base_url: base_url.trim().trim_end_matches('/').to_owned(),
            timeout,
            token,
            client,
        })
    }

    /// Asks the gateway where one phrase belongs.
    ///
    /// # Errors
    ///
    /// Returns a transport, status or decode failure.
    pub async fn utterance(
        &self,
        text: &str,
        profile: Profile,
    ) -> Result<UtteranceResponse, GatewayError> {
        let body = UtteranceRequest {
            text: text.to_owned(),
            profile: profile.as_str().to_owned(),
        };
        let value = self.post(UTTERANCE_PATH, &body, self.timeout).await?;
        Ok(value
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or(UtteranceResponse {
                route: voice_contract::Route::Unknown(String::new()),
                ok: None,
                thread_id: None,
                after_seq: None,
                speech: None,
            }))
    }

    /// Long-polls the answer of a routed turn.
    ///
    /// # Errors
    ///
    /// Returns a transport, status or decode failure.
    pub async fn result(
        &self,
        thread_id: &str,
        after_seq: i64,
        wait_seconds: u64,
    ) -> Result<ResultResponse, GatewayError> {
        let wait = wait_seconds.max(1);
        let body = ResultRequest {
            thread_id: thread_id.to_owned(),
            after_seq,
            wait_seconds: i32::try_from(wait).unwrap_or(i32::MAX),
        };
        let value = self
            .post(RESULT_PATH, &body, self.timeout + Duration::from_secs(wait))
            .await?;
        Ok(value
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or(ResultResponse {
                status: String::new(),
                speech: None,
                text: None,
                completed_seq: None,
            }))
    }

    /// Long-polls the dispatcher updates. `None` bootstraps without a cursor.
    ///
    /// # Errors
    ///
    /// Returns a transport, status or decode failure.
    pub async fn updates(
        &self,
        after_seq: Option<i64>,
        wait_seconds: u64,
    ) -> Result<UpdatesResponse, GatewayError> {
        let wait = wait_seconds.max(1);
        let body = UpdatesRequest {
            after_seq,
            wait_seconds: Some(i32::try_from(wait).unwrap_or(i32::MAX)),
        };
        let value = self
            .post(
                UPDATES_PATH,
                &body,
                self.timeout + Duration::from_secs(wait),
            )
            .await?;
        Ok(value
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or(UpdatesResponse {
                cursor: None,
                items: Vec::new(),
                status: None,
                speech: None,
            }))
    }

    async fn post<B>(
        &self,
        path: &str,
        body: &B,
        timeout: Duration,
    ) -> Result<Option<Value>, GatewayError>
    where
        B: serde::Serialize + Sync,
    {
        if self.base_url.is_empty() {
            return Err(GatewayError::NotConfigured);
        }
        let payload = serde_json::to_vec(body).map_err(|_| GatewayError::Decode)?;
        let response = self
            .client
            .post(format!("{}{path}", self.base_url))
            .timeout(timeout)
            .bearer_auth((self.token)())
            .header("content-type", "application/json")
            .header("accept", "application/json")
            .body(payload)
            .send()
            .await
            .map_err(GatewayError::Request)?;
        let status = response.status();
        if !status.is_success() {
            return Err(GatewayError::Status(status.as_u16()));
        }
        let bytes = response.bytes().await.map_err(GatewayError::Request)?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| GatewayError::Decode)?;
        Ok(value.is_object().then_some(value))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use axum::extract::State;
    use axum::http::StatusCode;
    use axum::routing::post;
    use axum::{Json, Router};
    use serde_json::{Value, json};
    use voice_contract::Route;

    use super::{Gateway, GatewayError};
    use crate::domain::profile::Profile;

    type Calls = Arc<std::sync::Mutex<Vec<(String, String, Value)>>>;

    async fn serve(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        format!("http://{address}")
    }

    fn client(base: &str) -> Gateway {
        Gateway::new(
            base,
            Arc::new(|| "gateway-token".to_owned()),
            Duration::from_secs(2),
        )
        .expect("client")
    }

    fn voice_app(calls: Calls) -> Router {
        Router::new()
            .route("/v1/voice/utterance", post(utterance))
            .route("/v1/voice/result", post(result))
            .route("/v1/voice/updates", post(updates))
            .with_state(calls)
    }

    async fn utterance(
        State(calls): State<Calls>,
        headers: axum::http::HeaderMap,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        let auth = headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_owned();
        calls
            .lock()
            .expect("lock")
            .push(("/v1/voice/utterance".to_owned(), auth, body));
        Json(
            json!({"route": "bb", "ok": true, "threadId": "thr_1", "afterSeq": 7, "speech": "Передал."}),
        )
    }

    async fn result(Json(body): Json<Value>) -> Json<Value> {
        assert_eq!(body["threadId"], "thr_1");
        assert_eq!(body["afterSeq"], 7);
        Json(json!({"status": "done", "speech": "Готово.", "completedSeq": 9}))
    }

    async fn updates(Json(body): Json<Value>) -> Json<Value> {
        assert_eq!(body["waitSeconds"], 3);
        Json(json!({"cursor": 12, "items": [{"seq": 11, "spoken": "Готово."}]}))
    }

    #[tokio::test]
    async fn utterance_posts_text_profile_and_bearer() {
        let calls: Calls = Arc::new(std::sync::Mutex::new(Vec::new()));
        let base = serve(voice_app(calls.clone())).await;
        let response = client(&base)
            .utterance("проверь", Profile::Work)
            .await
            .expect("utterance");
        assert_eq!(response.route, Route::Bb);
        assert_eq!(response.thread_id.as_deref(), Some("thr_1"));
        let calls = calls.lock().expect("lock");
        assert_eq!(calls[0].1, "Bearer gateway-token");
        assert_eq!(calls[0].2, json!({"text": "проверь", "profile": "work"}));
    }

    #[tokio::test]
    async fn result_and_updates_decode_the_typed_contract() {
        let calls: Calls = Arc::new(std::sync::Mutex::new(Vec::new()));
        let base = serve(voice_app(calls)).await;
        let gateway = client(&base);
        let result = gateway.result("thr_1", 7, 1).await.expect("result");
        assert_eq!(result.completed_seq, Some(9));
        let updates = gateway.updates(None, 3).await.expect("updates");
        assert_eq!(updates.cursor, Some(12));
        assert_eq!(updates.items[0].spoken, "Готово.");
    }

    #[tokio::test]
    async fn a_non_object_response_is_an_empty_answer() {
        let app = Router::new().route(
            "/v1/voice/utterance",
            post(|| async { Json(json!(["not", "an", "object"])) }),
        );
        let base = serve(app).await;
        let response = client(&base)
            .utterance("x", Profile::Work)
            .await
            .expect("utterance");
        assert_eq!(response.route, Route::Unknown(String::new()));
    }

    #[tokio::test]
    async fn a_non_2xx_is_an_error() {
        let app = Router::new().route(
            "/v1/voice/utterance",
            post(|| async { StatusCode::SERVICE_UNAVAILABLE }),
        );
        let base = serve(app).await;
        let error = client(&base)
            .utterance("x", Profile::Work)
            .await
            .expect_err("503");
        assert!(matches!(error, GatewayError::Status(503)));
    }
}
