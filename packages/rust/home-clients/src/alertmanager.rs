//! Minimal read-only Alertmanager v2 API client.
//!
//! Wire-compatible with `solutions/mcp-gateway/internal/alertmanager/client.go`.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::{Error, build_http, decode, normalize_base_url};

const DEFAULT_LIMIT: usize = 20;
const MAX_BODY_BYTES: usize = 1 << 20;

/// The subset of an Alertmanager alert the gateway reports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Alert {
    /// Alertmanager's stable identifier for the alert.
    #[serde(default)]
    pub fingerprint: String,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    #[serde(default)]
    pub annotations: BTreeMap<String, String>,
    #[serde(
        default,
        rename = "startsAt",
        deserialize_with = "deserialize_time",
        serialize_with = "serialize_time"
    )]
    pub starts_at: Option<OffsetDateTime>,
    #[serde(
        default,
        rename = "endsAt",
        deserialize_with = "deserialize_time",
        serialize_with = "serialize_time"
    )]
    pub ends_at: Option<OffsetDateTime>,
    #[serde(default)]
    pub status: AlertStatus,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AlertStatus {
    #[serde(default)]
    pub state: String,
}

impl Alert {
    /// One-line human description, with the same fallback order as Go:
    /// `summary`, then `description`, then `alertname`, then `alert`.
    #[must_use]
    pub fn summary(&self) -> String {
        if let Some(text) = self.annotations.get("summary").filter(|s| !s.is_empty()) {
            return text.clone();
        }
        if let Some(text) = self
            .annotations
            .get("description")
            .filter(|s| !s.is_empty())
        {
            return text.clone();
        }
        if let Some(name) = self.labels.get("alertname").filter(|s| !s.is_empty()) {
            return name.clone();
        }
        "alert".to_owned()
    }

    /// The alert severity label or an empty string.
    #[must_use]
    pub fn severity(&self) -> &str {
        self.labels.get("severity").map_or("", String::as_str)
    }
}

/// An Alertmanager client.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::Client,
    base_url: String,
}

impl Client {
    /// Build a client. `base_url` is the Alertmanager root, without `/api/v2`.
    pub fn new(base_url: impl Into<String>, timeout: Duration) -> Result<Self, Error> {
        Ok(Self {
            http: build_http("alertmanager query", timeout)?,
            base_url: normalize_base_url(&base_url.into()),
        })
    }

    /// Active, unsilenced, uninhibited alerts, capped at `limit`.
    ///
    /// A non-positive limit becomes `20`.
    pub async fn active(&self, limit: usize) -> Result<Vec<Alert>, Error> {
        const OP: &str = "alertmanager query";
        let limit = if limit == 0 { DEFAULT_LIMIT } else { limit };
        let response = self
            .http
            .get(format!("{}/api/v2/alerts", self.base_url))
            .query(&[
                ("active", "true"),
                ("silenced", "false"),
                ("inhibited", "false"),
            ])
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|source| Error::Transport {
                operation: OP,
                source,
            })?;
        let status = response.status().as_u16();
        let body = response.bytes().await.map_err(|source| Error::Transport {
            operation: OP,
            source,
        })?;
        if status != 200 {
            return Err(Error::Status {
                operation: OP,
                status,
            });
        }
        let mut alerts: Vec<Alert> = decode(OP, &body, MAX_BODY_BYTES)?;
        alerts.truncate(limit);
        Ok(alerts)
    }
}

fn deserialize_time<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<OffsetDateTime>, D::Error> {
    let raw = Option::<String>::deserialize(deserializer)?;
    match raw.as_deref().map(str::trim) {
        None | Some("") => Ok(None),
        Some(text) => OffsetDateTime::parse(text, &Rfc3339)
            .map(Some)
            .map_err(serde::de::Error::custom),
    }
}

fn serialize_time<S: Serializer>(
    value: &Option<OffsetDateTime>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        Some(time) => {
            serializer.serialize_str(&time.format(&Rfc3339).map_err(serde::ser::Error::custom)?)
        }
        None => serializer.serialize_none(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use axum::Router;
    use axum::extract::{Query, State};
    use axum::http::StatusCode;
    use axum::routing::get;

    use super::*;

    type Seen = Arc<std::sync::Mutex<Vec<HashMap<String, String>>>>;

    async fn serve(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn active_alerts_carry_filters_and_summary() {
        let seen: Seen = Arc::default();
        let app = Router::new()
            .route(
                "/api/v2/alerts",
                get(
                    |State(seen): State<Seen>,
                     Query(params): Query<HashMap<String, String>>| async move {
                        seen.lock().expect("lock").push(params);
                        axum::Json(serde_json::json!([{
                            "fingerprint": "a1b2c3d4e5f6",
                            "labels": {"alertname": "RicardoEndpointDown", "severity": "warning"},
                            "annotations": {"summary": "Гермес недоступен"},
                            "startsAt": "2026-11-01T09:00:00Z",
                            "status": {"state": "active"}
                        }]))
                    },
                ),
            )
            .with_state(seen.clone());
        let base = serve(app).await;
        let alerts = Client::new(format!("{base}/"), Duration::from_secs(5))
            .expect("client")
            .active(10)
            .await
            .expect("active");
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].summary(), "Гермес недоступен");
        assert_eq!(alerts[0].severity(), "warning");
        assert_eq!(alerts[0].fingerprint, "a1b2c3d4e5f6");
        assert_eq!(alerts[0].status.state, "active");
        assert_eq!(
            alerts[0].starts_at,
            Some(OffsetDateTime::parse("2026-11-01T09:00:00Z", &Rfc3339).expect("time"))
        );
        let seen = seen.lock().expect("lock");
        assert_eq!(seen[0].get("active").map(String::as_str), Some("true"));
        assert_eq!(seen[0].get("silenced").map(String::as_str), Some("false"));
        assert_eq!(seen[0].get("inhibited").map(String::as_str), Some("false"));
    }

    #[tokio::test]
    async fn summary_falls_back_and_limit_is_enforced() {
        let app = Router::new().route(
            "/api/v2/alerts",
            get(|| async {
                axum::Json(serde_json::json!([
                    {"labels": {"alertname": "OnlyName"}},
                    {"annotations": {"description": "fallback"}, "labels": {}},
                    {"labels": {}}
                ]))
            }),
        );
        let base = serve(app).await;
        let client = Client::new(base, Duration::from_secs(5)).expect("client");
        let alerts = client.active(2).await.expect("active");
        assert_eq!(alerts.len(), 2);
        assert_eq!(alerts[0].summary(), "OnlyName");
        assert_eq!(alerts[1].summary(), "fallback");
        assert_eq!(alerts[0].starts_at, None);
    }

    #[tokio::test]
    async fn an_http_error_is_transient() {
        let app = Router::new().route("/api/v2/alerts", get(|| async { StatusCode::BAD_GATEWAY }));
        let base = serve(app).await;
        let error = Client::new(base, Duration::from_secs(1))
            .expect("client")
            .active(5)
            .await
            .expect_err("HTTP 502");
        assert_eq!(error.kind(), crate::ErrorKind::Transient);
    }
}
