//! End-to-end sender test against a fake OAuth + FCM pair.
//!
//! The RSA key in `testdata/service-account.pem` is a throwaway generated for
//! these tests only (see the Gitleaks allowlist); it belongs to no Google
//! project and signs nothing real.

use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::post;
use phone_push::fcm::{Error, Message, Sender, ServiceAccount};
use ring::signature::{RSA_PKCS1_2048_8192_SHA256, UnparsedPublicKey};

const PRIVATE_KEY_PEM: &str = include_str!("../testdata/service-account.pem");
const PROJECT: &str = "home-test-project";

#[derive(Debug, Clone, Default)]
struct FakeGoogle {
    token_requests: Arc<Mutex<Vec<String>>>,
    messages: Arc<Mutex<Vec<(String, serde_json::Value)>>>,
    fcm_status: Arc<Mutex<u16>>,
}

fn service_account_json() -> String {
    let escaped = PRIVATE_KEY_PEM.replace('\n', "\\n");
    format!(
        r#"{{"type":"service_account","project_id":"{PROJECT}","client_email":"push@example.test","private_key":"{escaped}"}}"#
    )
}

async fn token(State(fake): State<FakeGoogle>, body: String) -> impl IntoResponse {
    fake.token_requests.lock().expect("lock").push(body);
    (
        StatusCode::OK,
        r#"{"access_token":"fake-access-token","expires_in":3600,"token_type":"Bearer"}"#,
    )
}

async fn send(
    State(fake): State<FakeGoogle>,
    Path(project): Path<String>,
    headers: HeaderMap,
    body: String,
) -> impl IntoResponse {
    assert_eq!(project, PROJECT);
    assert_eq!(
        headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
        Some("Bearer fake-access-token")
    );
    let parsed: serde_json::Value = serde_json::from_str(&body).expect("json body");
    fake.messages.lock().expect("lock").push((
        headers["authorization"]
            .to_str()
            .unwrap_or_default()
            .to_owned(),
        parsed,
    ));
    let status = *fake.fcm_status.lock().expect("lock");
    let payload = if status == 200 {
        r#"{"name":"projects/home-test-project/messages/1"}"#.to_owned()
    } else {
        r#"{"error":{"status":"NOT_FOUND","details":[{"errorCode":"UNREGISTERED"}]}}"#.to_owned()
    };
    (StatusCode::from_u16(status).expect("status"), payload)
}

struct Fixture {
    sender: Sender,
    fake: FakeGoogle,
}

fn fixture(fcm_status: u16) -> Fixture {
    let fake = FakeGoogle {
        fcm_status: Arc::new(Mutex::new(fcm_status)),
        ..FakeGoogle::default()
    };
    let app = Router::new()
        .route("/token", post(token))
        .route("/v1/projects/{project}/messages:send", post(send))
        .with_state(fake.clone());
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    listener.set_nonblocking(true).expect("nonblocking");
    let address = listener.local_addr().expect("addr");
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::from_std(listener).expect("listener");
            let _ = axum::serve(listener, app).await;
        });
    });
    let sender = Sender::new(
        &service_account_json(),
        &format!("http://{address}/token"),
        &format!("http://{address}"),
        Duration::from_secs(5),
    )
    .expect("sender");
    assert_eq!(sender.project_id(), PROJECT);
    Fixture { sender, fake }
}

fn block_on<T>(future: impl Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(future)
}

#[test]
fn a_find_message_is_sent_through_oauth_and_reuses_the_cached_token() {
    let Fixture { sender, fake } = fixture(200);
    let message = Message::Find {
        reason: "где телефон".to_owned(),
    };
    for _ in 0..2 {
        block_on(sender.send("device-token", &message, 1_700_000_000)).expect("send");
    }
    let token_requests = fake.token_requests.lock().expect("lock").clone();
    assert_eq!(token_requests.len(), 1, "the access token is cached");
    assert!(
        token_requests[0]
            .contains("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer"),
        "{}",
        token_requests[0]
    );
    let messages = fake.messages.lock().expect("lock").clone();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].0, "Bearer fake-access-token");
    assert_eq!(messages[0].1["message"]["android"]["priority"], "high");
    assert!(messages[0].1["message"].get("notification").is_none());
    assert_eq!(messages[0].1["message"]["data"]["type"], "phone_find");
}

#[test]
fn a_notification_carries_the_channel_and_the_deep_link() {
    let Fixture { sender, fake } = fixture(200);
    let message = Message::Notification {
        title: "Дом".to_owned(),
        body: "Дверь открыта".to_owned(),
        deep_link: Some("home://alarms".to_owned()),
    };
    block_on(sender.send("device-token", &message, 1_700_000_000)).expect("send");
    let messages = fake.messages.lock().expect("lock").clone();
    assert_eq!(
        messages[0].1["message"]["android"]["notification"]["channel_id"],
        "home_notifications"
    );
    assert_eq!(
        messages[0].1["message"]["data"]["deepLink"],
        "home://alarms"
    );
}

#[test]
fn an_unregistered_token_is_reported_so_the_caller_can_forget_it() {
    let Fixture { sender, .. } = fixture(404);
    let message = Message::Find {
        reason: String::new(),
    };
    let error =
        block_on(sender.send("device-token", &message, 1_700_000_000)).expect_err("dead token");
    assert!(matches!(error, Error::Unregistered), "{error}");
}

#[test]
fn the_signed_assertion_verifies_with_the_service_account_key() {
    let account = ServiceAccount::parse(&service_account_json()).expect("account");
    let assertion = account
        .assertion("https://oauth2.googleapis.com/token", 1_700_000_000)
        .expect("jwt");
    let parts: Vec<&str> = assertion.split('.').collect();
    assert_eq!(parts.len(), 3);
    let public_key = account.public_key_der().expect("public key");
    let signature =
        base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, parts[2])
            .expect("signature");
    let verifier = UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, public_key.as_slice());
    verifier
        .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature)
        .expect("signature verifies");
}
