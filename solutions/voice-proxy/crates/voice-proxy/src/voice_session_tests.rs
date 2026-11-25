use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Bytes;
use axum::http::{HeaderMap, Uri};
use axum::routing::post;

use crate::api::{Options, Proxy};
use crate::chat_relay::SessionPolicy;
use crate::domain::clock::SystemClock;
use crate::relay::Upstream;

#[derive(Clone, Default)]
struct Seen(Arc<Mutex<Vec<(String, HeaderMap, String)>>>);

async fn hermes(seen: Seen) -> String {
    let handler = move |uri: Uri, headers: HeaderMap, body: Bytes| {
        let seen = seen.clone();
        async move {
            seen.0.lock().expect("lock").push((
                uri.to_string(),
                headers,
                String::from_utf8_lossy(&body).into_owned(),
            ));
            (
                [("content-type", "application/json")],
                "{\"id\":\"c\",\"object\":\"chat.completion\",\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}",
            )
        }
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let address = listener.local_addr().expect("address");
    let app = Router::new().fallback(post(handler));
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
    format!("http://{address}")
}

fn proxy(upstream: &str) -> Router {
    let upstream = Upstream::new(upstream, std::time::Duration::from_secs(5)).expect("upstream");
    Arc::new(Proxy::new(Options {
        upstream,
        reasoning_effort: "low".to_owned(),
        max_body: 1 << 20,
        cancel_phrases: Vec::new(),
        router: None,
        clock: Arc::new(SystemClock),
    }))
    .router()
}

#[tokio::test]
async fn the_speaker_keeps_talking_to_the_single_completions_session() {
    let seen = Seen::default();
    let upstream = hermes(seen.clone()).await;
    let app = proxy(&upstream);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let address = listener.local_addr().expect("address");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
    let client = reqwest::Client::new();
    for text in ["включи свет", "а теперь выключи"] {
        let response = client
            .post(format!("http://{address}/v1/chat/completions"))
            .header("content-type", "application/json")
            .body(format!(
                "{{\"messages\":[{{\"role\":\"user\",\"content\":\"{text}\"}}]}}"
            ))
            .send()
            .await
            .expect("send");
        assert!(response.status().is_success());
    }
    let calls = seen.0.lock().expect("lock").clone();
    assert_eq!(calls.len(), 2);
    for (path, headers, body) in &calls {
        assert_eq!(path, "/v1/chat/completions");
        assert!(
            headers
                .keys()
                .all(|name| !name.as_str().contains("session")),
            "{headers:?}"
        );
        let value: serde_json::Value = serde_json::from_str(body).expect("json");
        let fields = value.as_object().expect("object");
        assert!(fields.keys().all(|key| !key.contains("session")), "{body}");
    }
}

#[test]
fn the_app_relay_refuses_every_session_but_the_app_ones() {
    let policy = SessionPolicy::new("home-app", "app_");
    for foreign in [
        "api_1795129665_0a1b2c3d",
        "voice",
        "station",
        "telegram",
        "",
        "app_",
    ] {
        assert!(!policy.allows(foreign), "{foreign}");
    }
}
