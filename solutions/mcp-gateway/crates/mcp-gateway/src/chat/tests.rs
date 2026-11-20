use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt as _;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tower::ServiceExt as _;

use super::hermes::{self, HermesApi, SseParser, TurnEvent};
use super::model::{ErrorKey, SendRequest, Status};
use super::store::{Store, Update};
use super::{Chat, Event, Options};
use crate::tools::BoxFuture;

const NOW: i64 = 1_795_129_665_000;

#[derive(Default)]
struct ScriptedHermes {
    scripts: Mutex<Vec<Vec<TurnEvent>>>,
    seen: Mutex<Vec<String>>,
}

impl ScriptedHermes {
    fn with(scripts: Vec<Vec<TurnEvent>>) -> Arc<Self> {
        Arc::new(Self {
            scripts: Mutex::new(scripts.into_iter().rev().collect()),
            seen: Mutex::new(Vec::new()),
        })
    }
}

impl HermesApi for ScriptedHermes {
    fn run<'a>(&'a self, message: String, sink: mpsc::Sender<TurnEvent>) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            self.seen.lock().expect("seen").push(message);
            let script = self
                .scripts
                .lock()
                .expect("scripts")
                .pop()
                .unwrap_or_default();
            for event in script {
                let _sent = sink.send(event).await;
            }
        })
    }
}

fn sequential_ids() -> super::NewId {
    let counter = Arc::new(AtomicUsize::new(0));
    Arc::new(move || format!("m_{:04}", counter.fetch_add(1, Ordering::SeqCst) + 1))
}

fn chat_with(store: Arc<Store>, hermes: Arc<ScriptedHermes>) -> Arc<Chat> {
    Chat::start(Options {
        store,
        hermes,
        now: Arc::new(|| NOW),
        new_id: sequential_ids(),
    })
    .expect("chat")
}

fn request(client_id: &str, text: &str, reply_to: Option<&str>) -> SendRequest {
    SendRequest {
        client_id: client_id.to_owned(),
        text: text.to_owned(),
        reply_to: reply_to.map(str::to_owned),
    }
}

async fn settled(chat: &Chat, id: &str) -> super::model::Message {
    for _ in 0..200 {
        if let Some(message) = chat.store.get(id).expect("get")
            && matches!(message.status, Status::Done | Status::Failed)
        {
            return message;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("message {id} never settled");
}

fn golden(name: &str, body: &Value) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/chat/testdata/contract")
        .join(format!("{name}.json"));
    let rendered = format!("{}\n", serde_json::to_string_pretty(body).expect("json"));
    if std::env::var_os("UPDATE_CHAT_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().expect("dir")).expect("mkdir");
        std::fs::write(&path, &rendered).expect("write golden");
    }
    let expected = match name {
        "send" => include_str!("testdata/contract/send.json"),
        "page" => include_str!("testdata/contract/page.json"),
        "events" => include_str!("testdata/contract/events.json"),
        _ => include_str!("testdata/contract/error.json"),
    };
    assert_eq!(rendered, expected, "{name}");
}

async fn call(app: &Router, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(body.map_or_else(Body::empty, |value| Body::from(value.to_string())))
        .expect("request");
    let response = app.clone().oneshot(request).await.expect("response");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn a_turn_streams_and_matches_the_app_contract() {
    let hermes = ScriptedHermes::with(vec![
        vec![
            TurnEvent::Delta("Свет ".to_owned()),
            TurnEvent::Delta("включён.".to_owned()),
            TurnEvent::Completed("Свет **включён**.".to_owned()),
        ],
        vec![TurnEvent::Completed("Да, в гостиной.".to_owned())],
    ]);
    let chat = chat_with(
        Arc::new(Store::in_memory().expect("store")),
        Arc::clone(&hermes),
    );
    let mut events = chat.subscribe();
    let app: Router = super::router(Arc::clone(&chat));

    let (status, sent) = call(
        &app,
        "POST",
        "/v1/chat/messages",
        Some(json!({"clientId": "c-1", "text": " Включи свет "})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    golden("send", &sent);
    let first = settled(&chat, "m_0002").await;
    assert_eq!(first.text, "Свет **включён**.");

    let (_, again) = call(
        &app,
        "POST",
        "/v1/chat/messages",
        Some(json!({"clientId": "c-1", "text": "Включи свет"})),
    )
    .await;
    assert_eq!(again["message"]["id"], "m_0001");

    let (status, second) = call(
        &app,
        "POST",
        "/v1/chat/messages",
        Some(json!({"clientId": "c-2", "text": "Где?", "replyTo": "m_0002"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    settled(&chat, second["reply"]["id"].as_str().expect("reply id")).await;
    assert_eq!(
        hermes.seen.lock().expect("seen").clone(),
        vec![
            "Включи свет".to_owned(),
            "[Ответ на сообщение (assistant): «Свет **включён**.»]\nГде?".to_owned()
        ]
    );

    let (_, page) = call(&app, "GET", "/v1/chat/messages?limit=3", None).await;
    golden("page", &page);
    let (_, older) = call(&app, "GET", "/v1/chat/messages?before=2&limit=3", None).await;
    assert_eq!(older["messages"].as_array().expect("messages").len(), 1);
    assert_eq!(older["hasMore"], false);

    let mut recorded = Vec::new();
    while let Ok(event) = events.try_recv() {
        recorded.push(match event {
            Event::Message(message) => json!({"event": "message", "data": message}),
            Event::Delta(delta) => json!({"event": "delta", "data": delta}),
        });
    }
    golden(
        "events",
        &Value::Array(recorded.into_iter().take(6).collect()),
    );
}

#[tokio::test]
async fn invalid_requests_answer_with_resource_keys() {
    let chat = chat_with(
        Arc::new(Store::in_memory().expect("store")),
        ScriptedHermes::with(vec![]),
    );
    let app: Router = super::router(chat);
    let (status, body) = call(
        &app,
        "POST",
        "/v1/chat/messages",
        Some(json!({"clientId": "c-1", "text": "   "})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    golden("error", &body);
    let (status, body) = call(
        &app,
        "POST",
        "/v1/chat/messages",
        Some(json!({"clientId": "c 1", "text": "hi"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "chat_client_id_invalid");
    let (status, body) = call(
        &app,
        "POST",
        "/v1/chat/messages",
        Some(json!({"clientId": "c-2", "text": "hi", "replyTo": "m_9999"})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "chat_reply_not_found");
    let long = "я".repeat(super::model::MAX_TEXT_CHARS + 1);
    let (status, body) = call(
        &app,
        "POST",
        "/v1/chat/messages",
        Some(json!({"clientId": "c-3", "text": long})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "chat_text_too_long");
}

#[tokio::test]
async fn a_failed_reply_keeps_partial_text_and_can_be_retried() {
    let hermes = ScriptedHermes::with(vec![
        vec![
            TurnEvent::Delta("Начал".to_owned()),
            TurnEvent::Failed(ErrorKey::ChatUpstreamFailed),
        ],
        vec![TurnEvent::Completed("Готово.".to_owned())],
    ]);
    let chat = chat_with(Arc::new(Store::in_memory().expect("store")), hermes);
    let app: Router = super::router(Arc::clone(&chat));
    chat.send(&request("c-1", "Проверь", None)).expect("send");
    let failed = settled(&chat, "m_0002").await;
    assert_eq!(failed.status, Status::Failed);
    assert_eq!(failed.text, "Начал");
    assert_eq!(failed.error, Some(ErrorKey::ChatUpstreamFailed));

    let (status, body) = call(&app, "POST", "/v1/chat/messages/m_0001/retry", None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "chat_not_retryable");
    let (status, body) = call(&app, "POST", "/v1/chat/messages/m_0002/retry", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "pending");
    let done = settled(&chat, "m_0002").await;
    assert_eq!(done.text, "Готово.");
    assert_eq!(done.error, None);
}

#[tokio::test]
async fn a_restart_fails_interrupted_turns_and_resumes_pending_ones() {
    let store = Arc::new(Store::in_memory().expect("store"));
    let ids = sequential_ids();
    for client in ["c-1", "c-2"] {
        store
            .insert_turn(&super::store::NewTurn {
                client_id: client,
                text: "hi",
                reply_to: None,
                user_id: ids(),
                reply_id: ids(),
                now: NOW,
            })
            .expect("insert");
    }
    store
        .update(
            "m_0002",
            &Update {
                text: Some("пол"),
                status: Status::Streaming,
                error: None,
                now: NOW,
            },
        )
        .expect("update");
    let hermes = ScriptedHermes::with(vec![vec![TurnEvent::Completed("ok".to_owned())]]);
    let chat = Chat::start(Options {
        store: Arc::clone(&store),
        hermes,
        now: Arc::new(|| NOW),
        new_id: Arc::new(|| "m_x".to_owned()),
    })
    .expect("chat");
    let interrupted = store.get("m_0002").expect("get").expect("message");
    assert_eq!(interrupted.status, Status::Failed);
    assert_eq!(interrupted.error, Some(ErrorKey::ChatInterrupted));
    assert_eq!(interrupted.text, "пол");
    assert_eq!(settled(&chat, "m_0004").await.text, "ok");
}

#[tokio::test]
async fn the_event_stream_replays_changes_after_the_cursor() {
    let hermes = ScriptedHermes::with(vec![vec![TurnEvent::Completed("ok".to_owned())]]);
    let chat = chat_with(Arc::new(Store::in_memory().expect("store")), hermes);
    chat.send(&request("c-1", "hi", None)).expect("send");
    settled(&chat, "m_0002").await;
    let all = chat.snapshot(0).expect("snapshot");
    let cursor = all.first().expect("first").version;
    let later = chat.snapshot(cursor).expect("later");
    assert_eq!(later.len(), 1);
    assert_eq!(later[0].id, "m_0002");
    assert_eq!(later[0].status, Status::Done);

    let app: Router = super::router(Arc::clone(&chat));
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/chat/events?after={cursor}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["content-type"].to_str().expect("type"),
        "text/event-stream"
    );
    let mut body = response.into_body();
    let frame = body.frame().await.expect("frame").expect("data");
    let text = String::from_utf8(frame.into_data().expect("bytes").to_vec()).expect("utf8");
    assert!(text.starts_with("event: message\n"), "{text}");
    assert!(text.contains("\"id\":\"m_0002\""), "{text}");
}

#[test]
fn the_sse_parser_handles_split_frames() {
    let mut parser = SseParser::default();
    assert!(
        parser
            .push("event: assistant.delta\ndata: {\"del")
            .is_empty()
    );
    let events = parser.push("ta\":\"x\"}\n\n: keepalive\n\nevent: done\r\ndata: {}\r\n\r\n");
    assert_eq!(
        events,
        vec![
            ("assistant.delta".to_owned(), "{\"delta\":\"x\"}".to_owned()),
            ("done".to_owned(), "{}".to_owned())
        ]
    );
}

#[tokio::test]
async fn the_http_client_creates_the_session_and_reads_the_stream() {
    use axum::routing::post;
    let calls = Arc::new(Mutex::new(Vec::<String>::new()));
    let create_calls = Arc::clone(&calls);
    let stream_calls = Arc::clone(&calls);
    let app = Router::new()
        .route(
            "/api/sessions",
            post(move |body: String| {
                let calls = Arc::clone(&create_calls);
                async move {
                    calls.lock().expect("calls").push(body);
                    (StatusCode::CONFLICT, "{}")
                }
            }),
        )
        .route(
            "/api/sessions/{id}/chat/stream",
            post(move |body: String| {
                let calls = Arc::clone(&stream_calls);
                async move {
                    calls.lock().expect("calls").push(body);
                    (
                        [("content-type", "text/event-stream")],
                        "event: run.started\ndata: {}\n\nevent: assistant.delta\ndata: {\"delta\":\"Пр\"}\n\nevent: assistant.delta\ndata: {\"delta\":\"ивет\"}\n\nevent: assistant.completed\ndata: {\"content\":\"Привет\"}\n\nevent: done\ndata: {}\n\n",
                    )
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let address = listener.local_addr().expect("address");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
    let client = hermes::Client::new(
        &format!("http://{address}"),
        "relay".to_owned(),
        "home-app",
        "prompt".to_owned(),
    )
    .expect("client");
    let (sink, mut events) = mpsc::channel(16);
    client.run("hi".to_owned(), sink).await;
    let mut received = Vec::new();
    while let Some(event) = events.recv().await {
        received.push(event);
    }
    assert_eq!(
        received,
        vec![
            TurnEvent::Delta("Пр".to_owned()),
            TurnEvent::Delta("ивет".to_owned()),
            TurnEvent::Completed("Привет".to_owned())
        ]
    );
    let calls = calls.lock().expect("calls").clone();
    let create: Value = serde_json::from_str(&calls[0]).expect("create");
    assert_eq!(create["id"], "home-app");
    let turn: Value = serde_json::from_str(&calls[1]).expect("turn");
    assert_eq!(turn, json!({"message": "hi", "system_message": "prompt"}));
}
