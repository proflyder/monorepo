//! Minimal read-only Prometheus HTTP API client: one instant query.
//!
//! Wire-compatible with `solutions/mcp-gateway/internal/prometheus/client.go`.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{Error, build_http, decode, normalize_base_url};

const DEFAULT_LIMIT: usize = 20;
const MAX_BODY_BYTES: usize = 1 << 20;

/// One series returned by an instant query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    /// Series labels (`metric` in the Prometheus wire format).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,
    /// Parsed numeric value. An unparsable value stays `0`, like the Go client.
    pub value: f64,
    /// The raw string Prometheus returned next to the timestamp.
    #[serde(default)]
    pub raw: String,
}

/// A Prometheus-compatible API client.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::Client,
    base_url: String,
}

impl Client {
    /// Build a client. `base_url` is the API root, for example
    /// `http://prometheus.example:9090`.
    pub fn new(base_url: impl Into<String>, timeout: Duration) -> Result<Self, Error> {
        Ok(Self {
            http: build_http("prometheus query", timeout)?,
            base_url: normalize_base_url(&base_url.into()),
        })
    }

    /// Run an instant query and return at most `limit` samples.
    ///
    /// An empty query is rejected, a non-positive limit becomes `20`.
    pub async fn query(&self, promql: &str, limit: usize) -> Result<Vec<Sample>, Error> {
        const OP: &str = "prometheus query";
        if promql.trim().is_empty() {
            return Err(Error::Invalid {
                operation: OP,
                detail: "promql is required",
            });
        }
        let limit = if limit == 0 { DEFAULT_LIMIT } else { limit };
        let response = self
            .http
            .get(format!("{}/api/v1/query", self.base_url))
            .query(&[("query", promql)])
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
        let decoded: ApiResponse = decode(OP, &body, MAX_BODY_BYTES)?;
        if decoded.status != "success" {
            return Err(Error::Api {
                operation: OP,
                detail: format!("{} {}", decoded.error_type, decoded.error)
                    .trim()
                    .to_owned(),
            });
        }
        let mut samples = Vec::with_capacity(decoded.data.result.len());
        for item in decoded.data.result {
            if samples.len() >= limit {
                break;
            }
            let raw = item
                .value
                .get(1)
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let value = raw.parse::<f64>().unwrap_or(0.0);
            samples.push(Sample {
                labels: item.metric,
                value,
                raw,
            });
        }
        Ok(samples)
    }
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    #[serde(default)]
    status: String,
    #[serde(default, rename = "errorType")]
    error_type: String,
    #[serde(default)]
    error: String,
    #[serde(default)]
    data: ApiData,
}

#[derive(Debug, Default, Deserialize)]
struct ApiData {
    #[serde(default)]
    result: Vec<RawSample>,
}

#[derive(Debug, Deserialize)]
struct RawSample {
    #[serde(default)]
    metric: BTreeMap<String, String>,
    #[serde(default)]
    value: Vec<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::Router;
    use axum::extract::State;
    use axum::http::StatusCode;
    use axum::routing::get;

    use super::*;

    async fn serve(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn instant_query_keeps_labels_order_and_limit() {
        let seen = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let app = Router::new()
            .route(
                "/api/v1/query",
                get(
                    |State(seen): State<Arc<std::sync::Mutex<Vec<String>>>>,
                     axum::extract::Query(params): axum::extract::Query<
                        std::collections::HashMap<String, String>,
                    >| async move {
                        seen.lock().expect("lock").push(params["query"].clone());
                        axum::Json(serde_json::json!({
                            "status": "success",
                            "data": {
                                "resultType": "vector",
                                "result": [
                                    {"metric": {"job": "scheduler"}, "value": [1730000000i64, "0"]},
                                    {"metric": {"job": "hermes"}, "value": [1730000000i64, "1"]},
                                    {"metric": {"job": "extra"}, "value": [1730000000i64, "2"]}
                                ]
                            }
                        }))
                    },
                ),
            )
            .with_state(seen.clone());
        let base = serve(app).await;
        let client = Client::new(format!("{base}/"), Duration::from_secs(5)).expect("client");
        let samples = client
            .query(r#"up{job=~"scheduler|hermes"}"#, 2)
            .await
            .expect("query");
        assert_eq!(
            seen.lock().expect("lock").as_slice(),
            [r#"up{job=~"scheduler|hermes"}"#]
        );
        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0].value, 0.0);
        assert_eq!(samples[1].value, 1.0);
        assert_eq!(samples[0].labels["job"], "scheduler");
        assert_eq!(samples[0].raw, "0");
    }

    #[tokio::test]
    async fn an_unparsable_value_is_zero() {
        let app = Router::new().route(
            "/api/v1/query",
            get(|| async {
                axum::Json(serde_json::json!({
                    "status": "success",
                    "data": {"result": [{"metric": {}, "value": [1i64, "NaN-ish"]}]}
                }))
            }),
        );
        let base = serve(app).await;
        let client = Client::new(base, Duration::from_secs(5)).expect("client");
        let samples = client.query("up", 10).await.expect("query");
        assert_eq!(samples[0].value, 0.0);
        assert_eq!(samples[0].raw, "NaN-ish");
    }

    #[tokio::test]
    async fn errors_are_classified() {
        let client = Client::new("http://example", Duration::from_secs(1)).expect("client");
        let error = client.query("  ", 1).await.expect_err("empty query");
        assert_eq!(error.kind(), crate::ErrorKind::Permanent);

        let app = Router::new().route(
            "/api/v1/query",
            get(|| async { StatusCode::INTERNAL_SERVER_ERROR }),
        );
        let base = serve(app).await;
        let error = Client::new(base, Duration::from_secs(1))
            .expect("client")
            .query("up", 1)
            .await
            .expect_err("HTTP 500");
        assert_eq!(error.kind(), crate::ErrorKind::Transient);

        let app = Router::new().route(
            "/api/v1/query",
            get(|| async {
                axum::Json(serde_json::json!({
                    "status": "error",
                    "errorType": "bad_data",
                    "error": "parse error"
                }))
            }),
        );
        let base = serve(app).await;
        let error = Client::new(base, Duration::from_secs(1))
            .expect("client")
            .query("up", 1)
            .await
            .expect_err("API error");
        assert_eq!(error.kind(), crate::ErrorKind::Permanent);
    }
}
