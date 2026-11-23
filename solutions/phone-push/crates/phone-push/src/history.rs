use std::time::Duration;

use home_secret::Token;
use serde::Serialize;

pub const HISTORY_PATH: &str = "/v1/push/history";
pub const NOTIFY_KIND: &str = "notify";
pub const FIND_KIND: &str = "find";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub device_id: String,
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub title: String,
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deep_link: Option<String>,
}

#[derive(Clone)]
pub struct HistoryClient {
    http: reqwest::Client,
    url: String,
    token: Token,
}

impl std::fmt::Debug for HistoryClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HistoryClient")
            .field("url", &self.url)
            .finish_non_exhaustive()
    }
}

impl HistoryClient {
    pub fn new(base_url: &str, token: Token, timeout: Duration) -> Result<Self, String> {
        let http = home_tls::public_https_client()
            .map_err(|error| format!("history TLS: {error}"))?
            .timeout(timeout)
            .build()
            .map_err(|error| format!("history HTTP client: {error}"))?;
        Ok(Self {
            http,
            url: format!("{}{HISTORY_PATH}", base_url.trim_end_matches('/')),
            token,
        })
    }

    pub async fn record(&self, entry: &HistoryEntry) -> Result<(), String> {
        let payload = serde_json::to_vec(entry).map_err(|_| "history request body".to_owned())?;
        let response = self
            .http
            .post(&self.url)
            .bearer_auth(self.token.expose())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(payload)
            .send()
            .await
            .map_err(|_| "history request failed".to_owned())?;
        if !response.status().is_success() {
            return Err(format!(
                "history returned HTTP {}",
                response.status().as_u16()
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode, header};
    use axum::response::{IntoResponse, Response};
    use axum::routing::post;
    use axum::{Json, Router};

    use super::*;

    type Received = Arc<Mutex<Vec<(String, String)>>>;

    async fn record(
        State(received): State<Received>,
        headers: HeaderMap,
        body: String,
    ) -> Response {
        let authorization = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        received.lock().expect("lock").push((authorization, body));
        Json(serde_json::json!({"entry": {"id": "n-1"}})).into_response()
    }

    #[tokio::test]
    async fn the_entry_is_posted_with_the_shared_token() {
        let received = Arc::new(Mutex::new(Vec::new()));
        let app = Router::new()
            .route(HISTORY_PATH, post(record))
            .with_state(Arc::clone(&received));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        let client = HistoryClient::new(
            &format!("http://{address}"),
            Token::new("scheduler-token"),
            Duration::from_secs(5),
        )
        .expect("client");
        client
            .record(&HistoryEntry {
                device_id: "device-a".to_owned(),
                id: "n-1".to_owned(),
                kind: NOTIFY_KIND.to_owned(),
                title: "Дом".to_owned(),
                body: "Готово".to_owned(),
                deep_link: Some("home://alarms".to_owned()),
            })
            .await
            .expect("record");
        let received = received.lock().expect("lock").clone();
        assert_eq!(received[0].0, "Bearer scheduler-token");
        let body: serde_json::Value = serde_json::from_str(&received[0].1).expect("json");
        assert_eq!(body["deviceId"], "device-a");
        assert_eq!(body["type"], NOTIFY_KIND);
        assert_eq!(body["deepLink"], "home://alarms");
    }

    #[tokio::test]
    async fn a_rejected_record_is_an_error() {
        let app = Router::new().route(
            HISTORY_PATH,
            post(|| async { StatusCode::UNAUTHORIZED.into_response() }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        let client = HistoryClient::new(
            &format!("http://{address}"),
            Token::new("wrong"),
            Duration::from_secs(5),
        )
        .expect("client");
        assert!(
            client
                .record(&HistoryEntry {
                    device_id: "device-a".to_owned(),
                    id: "n-1".to_owned(),
                    kind: FIND_KIND.to_owned(),
                    title: String::new(),
                    body: "нашёл".to_owned(),
                    deep_link: None,
                })
                .await
                .is_err()
        );
    }
}
