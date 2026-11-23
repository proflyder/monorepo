use std::time::Duration;

use home_secret::Token;
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Device {
    #[serde(default, rename = "deviceId")]
    pub device_id: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub platform: String,
    #[serde(default, rename = "deviceName")]
    pub device_name: String,
}

#[derive(Debug, Deserialize)]
struct DevicesResponse {
    #[serde(default)]
    devices: Vec<Device>,
}

#[derive(Clone)]
pub struct Registry {
    http: reqwest::Client,
    base_url: String,
    token: Token,
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Registry")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

impl Registry {
    pub fn new(base_url: &str, token: Token, timeout: Duration) -> Result<Self, String> {
        let http = home_tls::public_https_client()
            .map_err(|error| format!("registry TLS: {error}"))?
            .timeout(timeout)
            .build()
            .map_err(|error| format!("registry HTTP client: {error}"))?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_owned(),
            token,
        })
    }

    pub async fn list(&self) -> Result<Vec<Device>, String> {
        self.list_for(None).await
    }

    pub async fn list_for(&self, module: Option<&str>) -> Result<Vec<Device>, String> {
        let mut request = self
            .http
            .get(format!("{}/v1/push/devices", self.base_url))
            .bearer_auth(self.token.expose());
        if let Some(module) = module {
            request = request.query(&[("module", module)]);
        }
        let response = request
            .send()
            .await
            .map_err(|_| "registry request failed".to_owned())?;
        if !response.status().is_success() {
            return Err(format!(
                "registry returned HTTP {}",
                response.status().as_u16()
            ));
        }
        let body = response
            .text()
            .await
            .map_err(|_| "registry response".to_owned())?;
        let parsed: DevicesResponse =
            serde_json::from_str(&body).map_err(|_| "registry returned invalid JSON".to_owned())?;
        Ok(parsed
            .devices
            .into_iter()
            .filter(|device| !device.token.trim().is_empty())
            .collect())
    }

    pub async fn forget(&self, token: &str) -> Result<(), String> {
        let payload = serde_json::to_vec(&serde_json::json!({"token": token}))
            .map_err(|_| "registry request body".to_owned())?;
        let response = self
            .http
            .post(format!("{}/v1/push/devices/remove", self.base_url))
            .bearer_auth(self.token.expose())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(payload)
            .send()
            .await
            .map_err(|_| "registry request failed".to_owned())?;
        if !response.status().is_success() {
            return Err(format!(
                "registry returned HTTP {}",
                response.status().as_u16()
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode, header};
    use axum::response::{IntoResponse, Response};
    use axum::routing::{get, post};
    use axum::{Json, Router};

    #[derive(Clone, Default)]
    struct FakeScheduler {
        removed: Arc<std::sync::Mutex<Vec<String>>>,
    }

    async fn devices(State(fake): State<FakeScheduler>, headers: HeaderMap) -> Response {
        if headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            != Some("Bearer scheduler-token")
        {
            return (StatusCode::UNAUTHORIZED, "nope").into_response();
        }
        let _ = fake;
        Json(serde_json::json!({
            "devices": [
                {"deviceId": "device-a", "token": "token-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                 "platform": "android", "deviceName": "Стас"},
                {"deviceId": "device-b", "token": "", "platform": "android", "deviceName": ""}
            ]
        }))
        .into_response()
    }

    async fn remove(
        State(fake): State<FakeScheduler>,
        Json(body): Json<serde_json::Value>,
    ) -> StatusCode {
        fake.removed
            .lock()
            .expect("lock")
            .push(body["token"].as_str().unwrap_or_default().to_owned());
        StatusCode::OK
    }

    #[tokio::test]
    async fn only_non_empty_tokens_are_returned() {
        let fake = FakeScheduler::default();
        let app = Router::new()
            .route("/v1/push/devices", get(devices))
            .route("/v1/push/devices/remove", post(remove))
            .with_state(fake.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        let registry = Registry::new(
            &format!("http://{address}"),
            Token::new("scheduler-token"),
            Duration::from_secs(5),
        )
        .expect("registry");
        let devices = registry.list().await.expect("list");
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].platform, "android");
        assert_eq!(devices[0].device_id, "device-a");
        assert_eq!(devices[0].device_name, "Стас");
        registry.forget("token-a").await.expect("forget");
        assert_eq!(fake.removed.lock().expect("lock").as_slice(), ["token-a"]);
    }

    #[tokio::test]
    async fn a_rejected_bearer_is_an_error_not_an_empty_list() {
        let app = Router::new()
            .route("/v1/push/devices", get(devices))
            .with_state(FakeScheduler::default());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        let registry = Registry::new(
            &format!("http://{address}"),
            Token::new("wrong"),
            Duration::from_secs(5),
        )
        .expect("registry");
        assert!(registry.list().await.is_err());
    }
}
