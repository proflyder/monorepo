use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use home_secret::Secret;
use serde_json::json;

use crate::domain::SyncChange;

pub const ORIGIN_HEADER: &str = "home-device-id";
pub const SYNC_PATH: &str = "/v1/push/sync";
const TIMEOUT: Duration = Duration::from_secs(10);
const MAX_ORIGIN_CHARS: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Announcement {
    pub change: SyncChange,
    pub origin: Option<String>,
}

pub trait SyncSink: Send + Sync + std::fmt::Debug {
    fn publish(&self, announcement: Announcement);
}

#[derive(Debug, Default)]
pub struct NoSync;

impl SyncSink for NoSync {
    fn publish(&self, _announcement: Announcement) {}
}

#[derive(Debug, Default)]
pub struct MemorySync {
    published: Mutex<Vec<Announcement>>,
}

impl MemorySync {
    #[must_use]
    pub fn taken(&self) -> Vec<Announcement> {
        std::mem::take(
            &mut *self
                .published
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        )
    }
}

impl SyncSink for MemorySync {
    fn publish(&self, announcement: Announcement) {
        self.published
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(announcement);
    }
}

pub struct PushSync {
    client: reqwest::Client,
    url: String,
    token: Secret,
}

impl std::fmt::Debug for PushSync {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PushSync")
            .field("url", &self.url)
            .finish_non_exhaustive()
    }
}

impl PushSync {
    pub fn new(base_url: &str, token: Secret) -> Result<Self, String> {
        let _already_installed = rustls::crypto::ring::default_provider().install_default();
        let client = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .build()
            .map_err(|error| format!("push client: {error}"))?;
        Ok(Self {
            client,
            url: format!("{}{SYNC_PATH}", base_url.trim_end_matches('/')),
            token,
        })
    }

    pub async fn send(&self, announcement: &Announcement) -> Result<(), String> {
        let response = self
            .client
            .post(&self.url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", self.token.expose()),
            )
            .body(body(announcement).to_string())
            .send()
            .await
            .map_err(|error| format!("sync push request failed: {error}"))?;
        if response.status().is_success() {
            return Ok(());
        }
        Err(format!(
            "sync push returned HTTP {}",
            response.status().as_u16()
        ))
    }
}

impl SyncSink for Arc<PushSync> {
    fn publish(&self, announcement: Announcement) {
        let sync = Self::clone(self);
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        runtime.spawn(async move {
            if let Err(error) = sync.send(&announcement).await {
                tracing::warn!(%error, entity = announcement.change.entity.name(), "sync push failed");
            }
        });
    }
}

#[must_use]
pub fn body(announcement: &Announcement) -> serde_json::Value {
    let mut body = json!({
        "entity": announcement.change.entity.name(),
        "op": announcement.change.op.name(),
        "id": announcement.change.id,
    });
    if let Some(origin) = &announcement.origin {
        body["originDeviceId"] = json!(origin);
    }
    body
}

#[must_use]
pub fn origin(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty() && value.chars().count() <= MAX_ORIGIN_CHARS)
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::extract::State;
    use axum::http::HeaderMap;
    use axum::routing::post;
    use tokio::sync::mpsc;

    use super::*;
    use crate::domain::SyncEntity;

    #[test]
    fn the_body_names_the_change_and_its_origin_only() {
        let announcement = Announcement {
            change: SyncChange::delete(SyncEntity::Timers, "t-1"),
            origin: Some("phone-a".to_owned()),
        };
        assert_eq!(
            body(&announcement),
            json!({"entity": "timers", "op": "delete", "id": "t-1", "originDeviceId": "phone-a"})
        );
    }

    #[test]
    fn a_blank_or_oversized_origin_is_ignored() {
        assert_eq!(origin(Some("  ")), None);
        assert_eq!(origin(Some(&"x".repeat(200))), None);
        assert_eq!(origin(Some(" phone ")), Some("phone".to_owned()));
    }

    #[tokio::test]
    async fn the_push_service_receives_the_change_with_the_shared_token() {
        let (sent, mut received) = mpsc::unbounded_channel::<(String, String)>();
        let app = Router::new()
            .route(SYNC_PATH, post(record))
            .with_state(sent);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move { axum::serve(listener, app).await });
        let sync =
            PushSync::new(&format!("http://{address}/"), Secret::new("shared")).expect("client");
        let announcement = Announcement {
            change: SyncChange::upsert(SyncEntity::Alarms, "a-1"),
            origin: None,
        };
        sync.send(&announcement).await.expect("send");
        let (authorization, payload) = received.recv().await.expect("request");
        assert_eq!(authorization, "Bearer shared");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&payload).expect("json"),
            json!({"entity": "alarms", "op": "upsert", "id": "a-1"})
        );
    }

    async fn record(
        State(sent): State<mpsc::UnboundedSender<(String, String)>>,
        headers: HeaderMap,
        payload: String,
    ) -> &'static str {
        let authorization = headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let _delivered = sent.send((authorization, payload));
        "{}"
    }
}
