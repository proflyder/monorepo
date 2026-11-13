//! Client for the phone push service (`solutions/phone-push`).
//!
//! The gateway never talks to Google: it only asks the push service to ring the
//! owner's phone or to show a notification. Device tokens and the FCM service
//! account stay in that service, so the MCP surface cannot enumerate handsets
//! and Hermes never sees a Google credential.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::tools::BoxFuture;

/// Delivery counts returned by the push service. Tokens are never included.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delivery {
    /// Devices the message reached.
    #[serde(default)]
    pub sent: usize,
    /// Devices that failed for a transient reason.
    #[serde(default)]
    pub failed: usize,
    /// Devices that were registered for the send.
    #[serde(default)]
    pub devices: usize,
}

/// The push endpoints the tools need.
pub trait PushApi: Send + Sync {
    /// Whether the backend is configured.
    fn configured(&self) -> bool {
        true
    }

    /// Data-only high-priority find-phone message.
    fn find<'a>(&'a self, reason: &'a str) -> BoxFuture<'a, Result<Delivery, String>>;

    /// Normal notification with a title, a body and an optional deep link.
    fn notify<'a>(
        &'a self,
        title: &'a str,
        body: &'a str,
        deep_link: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Delivery, String>>;
}

/// The real HTTP client.
#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base_url: String,
    token: String,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("push::Client")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

impl Client {
    /// Builds a client for the push service root URL.
    pub fn new(base_url: &str, token: String, timeout: Duration) -> Result<Self, String> {
        let http = home_tls::public_https_client()
            .map_err(|error| format!("push TLS: {error}"))?
            .timeout(timeout)
            .build()
            .map_err(|error| format!("push HTTP client: {error}"))?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_owned(),
            token,
        })
    }

    async fn post(&self, path: &str, body: serde_json::Value) -> Result<Delivery, String> {
        let payload = serde_json::to_vec(&body).map_err(|_| "push request body".to_owned())?;
        let response = self
            .http
            .post(format!("{}{path}", self.base_url))
            .bearer_auth(self.token.as_str())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(payload)
            .send()
            .await
            .map_err(|_| "push request failed".to_owned())?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!("push service returned HTTP {}", status.as_u16()));
        }
        let text = response
            .text()
            .await
            .map_err(|_| "push response".to_owned())?;
        serde_json::from_str(&text).map_err(|_| "push service returned invalid JSON".to_owned())
    }
}

impl PushApi for Client {
    fn find<'a>(&'a self, reason: &'a str) -> BoxFuture<'a, Result<Delivery, String>> {
        Box::pin(async move { self.post("/v1/push/find", json!({"reason": reason})).await })
    }

    fn notify<'a>(
        &'a self,
        title: &'a str,
        body: &'a str,
        deep_link: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Delivery, String>> {
        Box::pin(async move {
            self.post(
                "/v1/push/notify",
                json!({"title": title, "body": body, "deepLink": deep_link.unwrap_or_default()}),
            )
            .await
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode, header};
    use axum::response::IntoResponse;
    use axum::routing::post;
    use axum::{Json, Router};

    use super::*;

    #[derive(Clone, Default)]
    struct Fake {
        bodies: Arc<std::sync::Mutex<Vec<(String, serde_json::Value)>>>,
    }

    async fn find(
        State(fake): State<Fake>,
        headers: HeaderMap,
        Json(body): Json<serde_json::Value>,
    ) -> impl IntoResponse {
        if headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            != Some("Bearer push-token")
        {
            return (StatusCode::UNAUTHORIZED, "{}").into_response();
        }
        fake.bodies
            .lock()
            .expect("lock")
            .push(("find".to_owned(), body));
        (StatusCode::OK, r#"{"sent":1,"failed":0,"devices":1}"#).into_response()
    }

    async fn notify(
        State(fake): State<Fake>,
        Json(body): Json<serde_json::Value>,
    ) -> impl IntoResponse {
        fake.bodies
            .lock()
            .expect("lock")
            .push(("notify".to_owned(), body));
        (StatusCode::OK, r#"{"sent":1,"failed":1,"devices":2}"#).into_response()
    }

    #[tokio::test]
    async fn find_and_notify_reach_the_expected_paths() {
        let fake = Fake::default();
        let app = Router::new()
            .route("/v1/push/find", post(find))
            .route("/v1/push/notify", post(notify))
            .with_state(fake.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        let client = Client::new(
            &format!("http://{address}"),
            "push-token".to_owned(),
            Duration::from_secs(5),
        )
        .expect("client");

        let delivery = client.find("где телефон").await.expect("find");
        assert_eq!(delivery.sent, 1);
        let delivery = client
            .notify("Дом", "Дверь открыта", Some("home://alarms"))
            .await
            .expect("notify");
        assert_eq!(delivery.failed, 1);
        assert_eq!(delivery.devices, 2);

        let bodies = fake.bodies.lock().expect("lock").clone();
        assert_eq!(bodies[0].0, "find");
        assert_eq!(bodies[0].1["reason"], "где телефон");
        assert_eq!(bodies[1].1["deepLink"], "home://alarms");
    }

    #[tokio::test]
    async fn a_rejected_token_is_an_error() {
        let app = Router::new()
            .route("/v1/push/find", post(find))
            .with_state(Fake::default());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        let client = Client::new(
            &format!("http://{address}"),
            "wrong".to_owned(),
            Duration::from_secs(5),
        )
        .expect("client");
        assert!(client.find("").await.is_err());
    }
}
