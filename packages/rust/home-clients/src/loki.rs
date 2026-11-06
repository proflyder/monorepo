//! Minimal read-only Loki query client.
//!
//! Wire-compatible with `solutions/mcp-gateway/internal/loki/client.go`: one
//! `/loki/api/v1/query_range` call with `direction=backward`, newest lines first.

use std::collections::BTreeMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::{Error, build_http, decode, normalize_base_url};

const DEFAULT_SINCE: Duration = Duration::from_secs(15 * 60);
const DEFAULT_LIMIT: usize = 50;
const MAX_BODY_BYTES: usize = 2 << 20;

/// One log line with its stream labels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Line {
    #[serde(
        deserialize_with = "deserialize_timestamp",
        serialize_with = "serialize_timestamp"
    )]
    pub timestamp: OffsetDateTime,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,
    pub text: String,
}

/// A Loki client.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::Client,
    base_url: String,
}

impl Client {
    /// Build a client. `base_url` is the Loki root, for example
    /// `http://loki.example:3100`.
    pub fn new(base_url: impl Into<String>, timeout: Duration) -> Result<Self, Error> {
        Ok(Self {
            http: build_http("loki query", timeout)?,
            base_url: normalize_base_url(&base_url.into()),
        })
    }

    /// Range query for the last `since`, newest lines first, capped at `limit`.
    ///
    /// An empty selector is rejected; a zero `since` becomes 15 minutes and a
    /// zero `limit` becomes `50`.
    pub async fn query(
        &self,
        selector: &str,
        since: Duration,
        limit: usize,
    ) -> Result<Vec<Line>, Error> {
        const OP: &str = "loki query";
        if selector.trim().is_empty() {
            return Err(Error::Invalid {
                operation: OP,
                detail: "loki selector is required",
            });
        }
        let since = if since.is_zero() {
            DEFAULT_SINCE
        } else {
            since
        };
        let limit = if limit == 0 { DEFAULT_LIMIT } else { limit };
        let end = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let start = end.saturating_sub(since.as_nanos());
        let response = self
            .http
            .get(format!("{}/loki/api/v1/query_range", self.base_url))
            .query(&[
                ("query", selector.to_owned()),
                ("start", start.to_string()),
                ("end", end.to_string()),
                ("limit", limit.to_string()),
                ("direction", "backward".to_owned()),
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
        let decoded: QueryResponse = decode(OP, &body, MAX_BODY_BYTES)?;
        if decoded.status != "success" {
            return Err(Error::Api {
                operation: OP,
                detail: decoded.error,
            });
        }
        let mut lines = Vec::new();
        for stream in decoded.data.result {
            for value in stream.values {
                if value.len() != 2 {
                    continue;
                }
                let Ok(nanos) = value[0].parse::<i128>() else {
                    continue;
                };
                let Ok(timestamp) = OffsetDateTime::from_unix_timestamp_nanos(nanos) else {
                    continue;
                };
                lines.push(Line {
                    timestamp,
                    labels: stream.stream.clone(),
                    text: value[1].clone(),
                });
            }
        }
        lines.sort_by_key(|line| std::cmp::Reverse(line.timestamp));
        lines.truncate(limit);
        Ok(lines)
    }
}

fn deserialize_timestamp<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<OffsetDateTime, D::Error> {
    let raw = String::deserialize(deserializer)?;
    OffsetDateTime::parse(raw.trim(), &time::format_description::well_known::Rfc3339)
        .map_err(serde::de::Error::custom)
}

fn serialize_timestamp<S: serde::Serializer>(
    value: &OffsetDateTime,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(
        &value
            .format(&time::format_description::well_known::Rfc3339)
            .map_err(serde::ser::Error::custom)?,
    )
}

#[derive(Debug, Deserialize)]
struct QueryResponse {
    #[serde(default)]
    status: String,
    #[serde(default)]
    error: String,
    #[serde(default)]
    data: QueryData,
}

#[derive(Debug, Default, Deserialize)]
struct QueryData {
    #[serde(default)]
    result: Vec<Stream>,
}

#[derive(Debug, Deserialize)]
struct Stream {
    #[serde(default)]
    stream: BTreeMap<String, String>,
    #[serde(default)]
    values: Vec<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use axum::Router;
    use axum::extract::{Query, State};
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
    async fn query_is_backward_and_newest_first() {
        let seen: Seen = Arc::default();
        let app =
            Router::new()
                .route(
                    "/loki/api/v1/query_range",
                    get(
                        |State(seen): State<Seen>,
                         Query(params): Query<HashMap<String, String>>| async move {
                            seen.lock().expect("lock").push(params);
                            axum::Json(serde_json::json!({
                                "status": "success",
                                "data": {
                                    "resultType": "streams",
                                    "result": [{
                                        "stream": {"namespace": "home"},
                                        "values": [
                                            ["1730000000000000000", "first"],
                                            ["1730000001000000000", "second"]
                                        ]
                                    }]
                                }
                            }))
                        },
                    ),
                )
                .with_state(seen.clone());
        let base = serve(app).await;
        let lines = Client::new(base, Duration::from_secs(5))
            .expect("client")
            .query(r#"{namespace="home"}"#, Duration::from_secs(900), 50)
            .await
            .expect("query");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "second");
        assert_eq!(lines[1].text, "first");
        assert_eq!(lines[0].labels["namespace"], "home");
        let seen = seen.lock().expect("lock");
        assert_eq!(
            seen[0].get("query").map(String::as_str),
            Some(r#"{namespace="home"}"#)
        );
        assert_eq!(
            seen[0].get("direction").map(String::as_str),
            Some("backward")
        );
        assert_eq!(seen[0].get("limit").map(String::as_str), Some("50"));
    }

    #[tokio::test]
    async fn invalid_values_are_skipped_and_limit_applies() {
        let app = Router::new().route(
            "/loki/api/v1/query_range",
            get(|| async {
                axum::Json(serde_json::json!({
                    "status": "success",
                    "data": {"result": [{
                        "stream": {},
                        "values": [
                            ["not-a-number", "bad"],
                            ["1", "short"],
                            ["1730000000000000000", "a"],
                            ["1730000002000000000", "b"]
                        ]
                    }]}
                }))
            }),
        );
        let base = serve(app).await;
        let lines = Client::new(base, Duration::from_secs(5))
            .expect("client")
            .query(r#"{a="b"}"#, Duration::from_secs(60), 1)
            .await
            .expect("query");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "b");
    }

    #[tokio::test]
    async fn errors_are_classified() {
        let client = Client::new("http://example", Duration::from_secs(1)).expect("client");
        let error = client
            .query("", Duration::from_secs(60), 10)
            .await
            .expect_err("empty selector");
        assert_eq!(error.kind(), crate::ErrorKind::Permanent);

        let app = Router::new().route(
            "/loki/api/v1/query_range",
            get(|| async {
                axum::Json(serde_json::json!({"status": "error", "error": "parse error"}))
            }),
        );
        let base = serve(app).await;
        let error = Client::new(base, Duration::from_secs(1))
            .expect("client")
            .query(r#"{a="b"}"#, Duration::from_secs(60), 10)
            .await
            .expect_err("API error");
        assert_eq!(error.kind(), crate::ErrorKind::Permanent);
    }
}
