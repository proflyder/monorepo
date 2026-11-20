use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, Response, StatusCode, header};
use axum::routing::post;
use tokio::sync::mpsc;

use crate::relay::Upstream;
use crate::services::TokenProvider;

pub const MAX_RELAY_BODY: usize = 12 << 20;

#[derive(Clone)]
pub struct ChatRelay {
    upstream: Upstream,
    session: Arc<str>,
    caller_token: TokenProvider,
    upstream_key: TokenProvider,
}

impl ChatRelay {
    #[must_use]
    pub fn new(
        upstream: Upstream,
        session: &str,
        caller_token: TokenProvider,
        upstream_key: TokenProvider,
    ) -> Self {
        Self {
            upstream,
            session: Arc::from(session),
            caller_token,
            upstream_key,
        }
    }

    pub fn router(self) -> Router {
        Router::new()
            .route("/api/sessions", post(create_session))
            .route("/api/sessions/{id}/chat/stream", post(chat_stream))
            .fallback(not_found)
            .with_state(Arc::new(self))
    }

    fn authorized(&self, headers: &HeaderMap) -> bool {
        let expected = (self.caller_token)();
        let presented = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().strip_prefix("Bearer "))
            .map(str::trim)
            .unwrap_or_default();
        !expected.is_empty() && constant_time_eq(presented.as_bytes(), expected.as_bytes())
    }

    fn upstream_authorization(&self) -> Option<HeaderValue> {
        let key = (self.upstream_key)();
        (!key.is_empty())
            .then(|| HeaderValue::from_str(&format!("Bearer {key}")).ok())
            .flatten()
    }
}

async fn create_session(
    State(relay): State<Arc<ChatRelay>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response<Body> {
    if !relay.authorized(&headers) {
        return status(StatusCode::UNAUTHORIZED);
    }
    if body.len() > MAX_RELAY_BODY || requested_session(&body).as_deref() != Some(&relay.session) {
        return status(StatusCode::FORBIDDEN);
    }
    match relay
        .upstream
        .buffered(
            reqwest::Method::POST,
            "/api/sessions",
            Some(body.to_vec()),
            relay.upstream_authorization(),
            None,
            None,
        )
        .await
    {
        Ok(fetch) => Response::builder()
            .status(fetch.status.as_u16())
            .header(header::CONTENT_TYPE, fetch.content_type)
            .body(Body::from(fetch.body))
            .unwrap_or_else(|_| status(StatusCode::BAD_GATEWAY)),
        Err(error) => {
            tracing::warn!(%error, "chat relay: session create failed");
            status(StatusCode::BAD_GATEWAY)
        }
    }
}

async fn chat_stream(
    State(relay): State<Arc<ChatRelay>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response<Body> {
    if !relay.authorized(&headers) {
        return status(StatusCode::UNAUTHORIZED);
    }
    if id != *relay.session || body.len() > MAX_RELAY_BODY {
        return status(StatusCode::FORBIDDEN);
    }
    let path = format!("/api/sessions/{id}/chat/stream");
    let response = match relay
        .upstream
        .open_stream(&path, body.to_vec(), relay.upstream_authorization())
        .await
    {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!(%error, "chat relay: stream open failed");
            return status(StatusCode::BAD_GATEWAY);
        }
    };
    let code = response.status().as_u16();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/json")
        .to_owned();
    let (sender, receiver) = mpsc::channel(64);
    tokio::spawn(pump(response, sender));
    Response::builder()
        .status(code)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "no-cache")
        .body(Body::from_stream(Pumped { receiver }))
        .unwrap_or_else(|_| status(StatusCode::BAD_GATEWAY))
}

async fn pump(mut response: reqwest::Response, sender: mpsc::Sender<Bytes>) {
    while let Ok(Some(chunk)) = response.chunk().await {
        if sender.send(chunk).await.is_err() {
            return;
        }
    }
}

struct Pumped {
    receiver: mpsc::Receiver<Bytes>,
}

impl futures_core::Stream for Pumped {
    type Item = Result<Bytes, Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.receiver.poll_recv(cx).map(|item| item.map(Ok))
    }
}

async fn not_found() -> Response<Body> {
    status(StatusCode::NOT_FOUND)
}

fn requested_session(body: &[u8]) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    value
        .get("id")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

fn status(code: StatusCode) -> Response<Body> {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = code;
    response
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::time::Duration;

    use axum::Router;
    use axum::body::Bytes;
    use axum::http::HeaderMap;
    use axum::routing::post;

    use super::ChatRelay;
    use crate::relay::Upstream;

    #[derive(Clone, Default)]
    struct Seen(Arc<Mutex<Vec<String>>>);

    async fn serve(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        format!("http://{address}")
    }

    fn record(seen: &Seen, headers: &HeaderMap, path: &str) {
        let auth = headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_owned();
        seen.0.lock().expect("lock").push(format!("{path} {auth}"));
    }

    async fn hermes(seen: Seen) -> String {
        let create = {
            let seen = seen.clone();
            move |headers: HeaderMap, _body: Bytes| {
                let seen = seen.clone();
                async move {
                    record(&seen, &headers, "/api/sessions");
                    (axum::http::StatusCode::CREATED, "{\"ok\":true}")
                }
            }
        };
        let stream = move |headers: HeaderMap, _body: Bytes| {
            let seen = seen.clone();
            async move {
                record(&seen, &headers, "/stream");
                (
                    [("content-type", "text/event-stream")],
                    "event: assistant.delta\ndata: {\"delta\":\"hi\"}\n\nevent: done\ndata: {}\n\n",
                )
            }
        };
        serve(
            Router::new()
                .route("/api/sessions", post(create))
                .route("/api/sessions/{id}/chat/stream", post(stream)),
        )
        .await
    }

    async fn relay_for(upstream: &str) -> String {
        let upstream = Upstream::new(upstream, Duration::from_secs(5)).expect("client");
        let relay = ChatRelay::new(
            upstream,
            "home-app",
            Arc::new(|| "caller".to_owned()),
            Arc::new(|| "hermes-key".to_owned()),
        );
        serve(relay.router()).await
    }

    #[tokio::test]
    async fn only_the_configured_session_is_relayed_with_the_hermes_key() {
        let seen = Seen::default();
        let relay = relay_for(&hermes(seen.clone()).await).await;
        let client = reqwest::Client::new();
        let created = client
            .post(format!("{relay}/api/sessions"))
            .bearer_auth("caller")
            .body("{\"id\":\"home-app\"}")
            .send()
            .await
            .expect("create");
        assert_eq!(created.status(), 201);
        let other = client
            .post(format!("{relay}/api/sessions"))
            .bearer_auth("caller")
            .body("{\"id\":\"telegram\"}")
            .send()
            .await
            .expect("create other");
        assert_eq!(other.status(), 403);
        let streamed = client
            .post(format!("{relay}/api/sessions/home-app/chat/stream"))
            .bearer_auth("caller")
            .body("{}")
            .send()
            .await
            .expect("stream");
        assert_eq!(streamed.status(), 200);
        assert!(streamed.text().await.expect("body").contains("\"hi\""));
        let foreign = client
            .post(format!("{relay}/api/sessions/telegram/chat/stream"))
            .bearer_auth("caller")
            .body("{}")
            .send()
            .await
            .expect("foreign");
        assert_eq!(foreign.status(), 403);
        assert_eq!(
            seen.0.lock().expect("lock").clone(),
            vec![
                "/api/sessions Bearer hermes-key".to_owned(),
                "/stream Bearer hermes-key".to_owned()
            ]
        );
    }

    #[tokio::test]
    async fn a_wrong_token_and_other_paths_are_refused() {
        let relay = relay_for(&hermes(Seen::default()).await).await;
        let client = reqwest::Client::new();
        let wrong = client
            .post(format!("{relay}/api/sessions/home-app/chat/stream"))
            .bearer_auth("nope")
            .body("{}")
            .send()
            .await
            .expect("wrong");
        assert_eq!(wrong.status(), 401);
        let jobs = client
            .get(format!("{relay}/api/jobs"))
            .bearer_auth("caller")
            .send()
            .await
            .expect("jobs");
        assert_eq!(jobs.status(), 404);
    }
}
