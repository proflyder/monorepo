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

use super::hermes::{self, HermesApi, SseParser, Turn, TurnEvent};
use super::media::{self, MediaStore, Pcm};
use super::model::{ErrorKey, SendRequest, Status};
use super::store::{Store, Update};
use super::stt::{self, SpeechToText};
use super::{Chat, Event, Options};
use crate::tools::BoxFuture;

const NOW: i64 = 1_795_129_665_000;
const DEFAULT_SESSION: &str = "home-app";

#[derive(Default)]
struct ScriptedHermes {
    scripts: Mutex<Vec<Vec<TurnEvent>>>,
    seen: Mutex<Vec<Value>>,
    sessions: Mutex<Vec<String>>,
    calls: Mutex<Vec<String>>,
    title: Mutex<Option<String>>,
}

impl ScriptedHermes {
    fn with(scripts: Vec<Vec<TurnEvent>>) -> Arc<Self> {
        Arc::new(Self {
            scripts: Mutex::new(scripts.into_iter().rev().collect()),
            ..Self::default()
        })
    }
}

impl HermesApi for ScriptedHermes {
    fn run<'a>(&'a self, turn: Turn<'a>, sink: mpsc::Sender<TurnEvent>) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            self.sessions
                .lock()
                .expect("sessions")
                .push(turn.session.to_owned());
            self.seen.lock().expect("seen").push(turn.message);
            let script = self
                .scripts
                .lock()
                .expect("scripts")
                .pop()
                .unwrap_or_default();
            for event in script {
                if event == TurnEvent::Delta(HANG.to_owned()) {
                    std::future::pending::<()>().await;
                }
                let _sent = sink.send(event).await;
            }
        })
    }

    fn title<'a>(&'a self, _session: &'a str) -> BoxFuture<'a, Option<String>> {
        Box::pin(async move { self.title.lock().expect("title").clone() })
    }

    fn patch<'a>(&'a self, session: &'a str, fields: Value) -> BoxFuture<'a, Result<(), ErrorKey>> {
        Box::pin(async move {
            self.calls
                .lock()
                .expect("calls")
                .push(format!("patch {session} {fields}"));
            Ok(())
        })
    }

    fn delete<'a>(&'a self, session: &'a str) -> BoxFuture<'a, Result<(), ErrorKey>> {
        Box::pin(async move {
            self.calls
                .lock()
                .expect("calls")
                .push(format!("delete {session}"));
            Ok(())
        })
    }
}

const HANG: &str = "\u{0}hang";

fn sequential_ids() -> super::NewId {
    let counter = Arc::new(AtomicUsize::new(0));
    Arc::new(move || format!("m_{:04}", counter.fetch_add(1, Ordering::SeqCst) + 1))
}

struct FixedStt(&'static str);

impl SpeechToText for FixedStt {
    fn transcribe<'a>(&'a self, pcm: &'a Pcm) -> BoxFuture<'a, Result<String, String>> {
        let text = if pcm.samples.is_empty() {
            String::new()
        } else {
            self.0.to_owned()
        };
        Box::pin(async move { Ok(text) })
    }
}

fn media_dir() -> (tempfile::TempDir, Arc<MediaStore>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let media = Arc::new(MediaStore::open(dir.path()).expect("media"));
    (dir, media)
}

fn chat_with(store: Arc<Store>, hermes: Arc<ScriptedHermes>) -> Arc<Chat> {
    chat_with_media(store, hermes, None)
}

fn chat_with_media(
    store: Arc<Store>,
    hermes: Arc<ScriptedHermes>,
    media: Option<Arc<MediaStore>>,
) -> Arc<Chat> {
    Chat::start(Options {
        default_session: DEFAULT_SESSION.to_owned(),
        store,
        hermes,
        media,
        stt: Some(Arc::new(FixedStt("включи свет в гостиной"))),
        push: None,
        notify: super::Notify::default(),
        now: Arc::new(|| NOW),
        new_id: sequential_ids(),
    })
    .expect("chat")
}

fn request(client_id: &str, text: &str, reply_to: Option<&str>) -> SendRequest {
    SendRequest {
        client_id: client_id.to_owned(),
        session_id: None,
        text: text.to_owned(),
        reply_to: reply_to.map(str::to_owned),
        attachments: Vec::new(),
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
        "voice" => include_str!("testdata/contract/voice.json"),
        "attachment" => include_str!("testdata/contract/attachment.json"),
        "alert" => include_str!("testdata/contract/alert.json"),
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
            json!("Включи свет"),
            json!("[Ответ на сообщение (assistant): «Свет **включён**.»]\nГде?")
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
            other => json!({"event": "other", "data": other.session_id()}),
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
    store
        .adopt_legacy(DEFAULT_SESSION, "legacy", NOW)
        .expect("legacy");
    let ids = sequential_ids();
    for client in ["c-1", "c-2"] {
        store
            .insert_turn(&super::store::NewTurn {
                session_id: DEFAULT_SESSION,
                client_id: client,
                text: "hi",
                kind: super::model::Kind::Text,
                attachments: &[],
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
        default_session: DEFAULT_SESSION.to_owned(),
        store: Arc::clone(&store),
        hermes,
        media: None,
        stt: None,
        push: None,
        notify: super::Notify::default(),
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
        "prompt".to_owned(),
    )
    .expect("client");
    let (sink, mut events) = mpsc::channel(16);
    let turn = Turn {
        session: "home-app",
        title: Some("Главный чат"),
        message: json!("hi"),
    };
    client.run(turn, sink).await;
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

async fn upload(app: &Router, mime: &str, name: &str, bytes: Vec<u8>) -> (StatusCode, Value) {
    let request = Request::builder()
        .method("POST")
        .uri(format!("/v1/chat/attachments?name={name}"))
        .header("content-type", mime)
        .body(Body::from(bytes))
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

fn second_of_speech() -> Vec<u8> {
    media::wav(
        &vec![1_u8; usize::try_from(media::VOICE_RATE).expect("rate") * 2],
        media::VOICE_RATE,
    )
}

#[tokio::test]
async fn a_voice_message_is_transcribed_by_stt_and_sent_as_text() {
    let hermes = ScriptedHermes::with(vec![vec![TurnEvent::Completed("Включил.".to_owned())]]);
    let (_dir, media) = media_dir();
    let chat = chat_with_media(
        Arc::new(Store::in_memory().expect("store")),
        Arc::clone(&hermes),
        Some(media),
    );
    let app: Router = super::router(Arc::clone(&chat));
    let (status, attachment) = upload(&app, "audio/wav", "voice.wav", second_of_speech()).await;
    assert_eq!(status, StatusCode::OK);
    golden("attachment", &attachment);
    let (status, sent) = call(
        &app,
        "POST",
        "/v1/chat/messages",
        Some(json!({"clientId": "v-1", "attachments": [attachment["id"]]})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{sent}");
    settled(&chat, sent["reply"]["id"].as_str().expect("reply")).await;
    assert_eq!(
        hermes.seen.lock().expect("seen").clone(),
        vec![json!("[Голосовое сообщение]: включи свет в гостиной\n")]
    );
    let voice = chat
        .store
        .get(sent["message"]["id"].as_str().expect("id"))
        .expect("get")
        .expect("message");
    golden("voice", &serde_json::to_value(&voice).expect("json"));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/chat/attachments/{}",
                    attachment["id"].as_str().expect("id")
                ))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.headers()["content-type"], "audio/wav");
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    assert_eq!(bytes.to_vec(), second_of_speech());
}

#[tokio::test]
async fn an_image_goes_to_hermes_as_a_vision_part_with_its_caption() {
    let hermes = ScriptedHermes::with(vec![vec![TurnEvent::Completed("Это плата.".to_owned())]]);
    let (_dir, media) = media_dir();
    let chat = chat_with_media(
        Arc::new(Store::in_memory().expect("store")),
        Arc::clone(&hermes),
        Some(media),
    );
    let app: Router = super::router(Arc::clone(&chat));
    let jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3];
    let (_, attachment) = upload(&app, "image/jpeg", "photo.jpg", jpeg).await;
    let (_, sent) = call(
        &app,
        "POST",
        "/v1/chat/messages",
        Some(json!({"clientId": "i-1", "text": "Что это?", "attachments": [attachment["id"]]})),
    )
    .await;
    assert_eq!(sent["message"]["kind"], "image");
    settled(&chat, sent["reply"]["id"].as_str().expect("reply")).await;
    let seen = hermes.seen.lock().expect("seen").clone();
    assert_eq!(seen[0][0], json!({"type": "text", "text": "Что это?"}));
    assert_eq!(
        seen[0][1]["image_url"]["url"],
        "data:image/jpeg;base64,/9j/4AECAw=="
    );
}

#[tokio::test]
async fn uploads_are_validated() {
    let (_dir, media) = media_dir();
    let chat = chat_with_media(
        Arc::new(Store::in_memory().expect("store")),
        ScriptedHermes::with(vec![]),
        Some(media),
    );
    let app: Router = super::router(chat);
    let (status, body) = upload(&app, "application/zip", "a.zip", vec![1, 2, 3]).await;
    assert_eq!(
        (status, body["code"].clone()),
        (
            StatusCode::BAD_REQUEST,
            json!("chat_attachment_unsupported")
        )
    );
    let stereo = {
        let mut wav = media::wav(&[0, 0, 0, 0], media::VOICE_RATE);
        wav[22] = 2;
        wav
    };
    let (_, body) = upload(&app, "audio/wav", "v.wav", stereo).await;
    assert_eq!(body["code"], "chat_voice_format");
    let (_, body) = upload(&app, "text/plain", "notes.txt", "лог".as_bytes().to_vec()).await;
    assert_eq!(body["kind"], "file");
    let (status, body) = call(
        &app,
        "POST",
        "/v1/chat/messages",
        Some(json!({"clientId": "x-1", "attachments": ["f_missing"]})),
    )
    .await;
    assert_eq!(
        (status, body["code"].clone()),
        (StatusCode::NOT_FOUND, json!("chat_attachment_not_found"))
    );
}

#[tokio::test]
async fn the_wyoming_client_streams_audio_and_reads_the_transcript() {
    use tokio::io::BufReader;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let address = listener.local_addr().expect("address").to_string();
    let server = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.expect("accept");
        let (read, mut write) = socket.into_split();
        let mut reader = BufReader::new(read);
        let mut kinds = Vec::new();
        loop {
            let (kind, _) = stt::read_event(&mut reader).await.expect("event");
            kinds.push(kind.clone());
            if kind == "audio-stop" {
                break;
            }
        }
        stt::write_event(&mut write, "transcript", &json!({"text": " привет "}), &[])
            .await
            .expect("reply");
        kinds
    });
    let client = stt::Wyoming::new(
        vec!["127.0.0.1:1".to_owned(), address],
        Duration::from_secs(5),
    );
    let pcm = Pcm {
        samples: vec![0; stt::CHUNK_BYTES * 2 + 10],
        rate: media::VOICE_RATE,
    };
    assert_eq!(client.transcribe(&pcm).await.expect("transcript"), "привет");
    assert_eq!(
        server.await.expect("server"),
        vec![
            "transcribe",
            "audio-start",
            "audio-chunk",
            "audio-chunk",
            "audio-chunk",
            "audio-stop"
        ]
    );
}

type SentPush = (String, String, String, Option<String>);

#[derive(Default)]
struct RecordingPush {
    sent: Mutex<Vec<SentPush>>,
}

impl crate::push::PushApi for RecordingPush {
    fn devices<'a>(&'a self) -> BoxFuture<'a, Result<Vec<crate::push::Device>, String>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn find<'a>(
        &'a self,
        _reason: &'a str,
        _device: Option<&'a str>,
    ) -> BoxFuture<'a, Result<crate::push::Delivery, String>> {
        Box::pin(async { Ok(crate::push::Delivery::default()) })
    }

    fn notify<'a>(
        &'a self,
        _title: &'a str,
        _body: &'a str,
        _deep_link: Option<&'a str>,
        _device: Option<&'a str>,
    ) -> BoxFuture<'a, Result<crate::push::Delivery, String>> {
        Box::pin(async { Err("chat pushes must name a module".to_owned()) })
    }

    fn notify_module<'a>(
        &'a self,
        module: home_app_auth::Module,
        title: &'a str,
        body: &'a str,
        deep_link: Option<&'a str>,
    ) -> BoxFuture<'a, Result<crate::push::Delivery, String>> {
        self.sent.lock().expect("sent").push((
            module.id().to_owned(),
            title.to_owned(),
            body.to_owned(),
            deep_link.map(str::to_owned),
        ));
        Box::pin(async { Ok(crate::push::Delivery::default()) })
    }
}

fn alerting_chat(hermes: Arc<ScriptedHermes>, push: Arc<RecordingPush>) -> Arc<Chat> {
    Chat::start(Options {
        default_session: DEFAULT_SESSION.to_owned(),
        store: Arc::new(Store::in_memory().expect("store")),
        hermes,
        media: None,
        stt: None,
        push: Some(push),
        notify: super::Notify {
            zone: jiff::tz::TimeZone::get("Asia/Almaty").expect("zone"),
            ..super::Notify::default()
        },
        now: Arc::new(|| NOW),
        new_id: sequential_ids(),
    })
    .expect("chat")
}

fn webhook(status: &str) -> Value {
    json!({
        "status": status,
        "alerts": [{
            "status": status,
            "labels": {"alertname": "KubePodCrashLooping", "severity": "critical", "namespace": "home-assistant", "pod": "scheduler-7d9"},
            "annotations": {"summary": "scheduler перезапускается", "runbook_url": "https://runbooks.example.com/crash"},
            "startsAt": "2026-11-20T08:00:00Z",
            "endsAt": if status == "resolved" { "2026-11-20T08:30:00Z" } else { "0001-01-01T00:00:00Z" },
            "fingerprint": "a1b2c3d4e5f60718"
        }]
    })
}

async fn settle_tasks() {
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn alerts_become_cards_that_resolve_in_place_and_push_once() {
    let push = Arc::new(RecordingPush::default());
    let hermes = ScriptedHermes::with(vec![vec![TurnEvent::Completed(
        "Разбираюсь: OOMKilled.".to_owned(),
    )]]);
    let chat = alerting_chat(Arc::clone(&hermes), Arc::clone(&push));
    let alerts: Router = super::alerts_router(Arc::clone(&chat));
    let (status, body) = call(&alerts, "POST", "/v1/chat/alerts", Some(webhook("firing"))).await;
    assert_eq!(
        (status, body["accepted"].clone()),
        (StatusCode::OK, json!(1))
    );
    let (_, again) = call(&alerts, "POST", "/v1/chat/alerts", Some(webhook("firing"))).await;
    assert_eq!(again["accepted"], 1);
    let (_, _) = call(
        &alerts,
        "POST",
        "/v1/chat/alerts",
        Some(webhook("resolved")),
    )
    .await;
    let card = chat.store.get("m_0001").expect("get").expect("alert");
    golden("alert", &serde_json::to_value(&card).expect("json"));
    assert_eq!(chat.page(None, None).expect("page").messages.len(), 1);
    assert_eq!(
        chat.page_for(None, None, Some(1), false)
            .expect("page without alerts")
            .messages
            .len(),
        0
    );
    assert_eq!(
        chat.page_for(None, None, Some(1), true)
            .expect("page with alerts")
            .messages
            .len(),
        1
    );

    let app: Router = super::router(Arc::clone(&chat));
    let (_, sent) = call(
        &app,
        "POST",
        "/v1/chat/messages",
        Some(json!({"clientId": "r-1", "text": "Что случилось?", "replyTo": "m_0001"})),
    )
    .await;
    settled(&chat, sent["reply"]["id"].as_str().expect("reply")).await;
    let prompt = hermes.seen.lock().expect("seen")[0]
        .as_str()
        .expect("text")
        .to_owned();
    assert!(prompt.contains("🔎 a1b2c3d4e5f6"), "{prompt}");
    assert!(prompt.ends_with("Что случилось?"), "{prompt}");

    settle_tasks().await;
    let pushes = push.sent.lock().expect("sent").clone();
    assert_eq!(
        pushes,
        vec![
            (
                "alerts".to_owned(),
                "🔴 KubePodCrashLooping".to_owned(),
                "scheduler перезапускается".to_owned(),
                Some("home://chat/session/home-app/message/m_0001".to_owned())
            ),
            (
                "chat".to_owned(),
                "Рикардо".to_owned(),
                "Разбираюсь: OOMKilled.".to_owned(),
                Some(format!(
                    "home://chat/session/home-app/message/{}",
                    sent["reply"]["id"].as_str().expect("id")
                ))
            ),
        ]
    );
}

#[tokio::test]
async fn an_invalid_webhook_is_rejected() {
    let chat = alerting_chat(
        ScriptedHermes::with(vec![]),
        Arc::new(RecordingPush::default()),
    );
    let alerts: Router = super::alerts_router(chat);
    let (status, body) = call(
        &alerts,
        "POST",
        "/v1/chat/alerts",
        Some(json!({"alerts": "nope"})),
    )
    .await;
    assert_eq!(
        (status, body["code"].clone()),
        (StatusCode::BAD_REQUEST, json!("chat_alert_invalid"))
    );
}

#[tokio::test]
async fn sessions_are_listed_newest_first_with_previews_and_route_turns() {
    let hermes = ScriptedHermes::with(vec![vec![TurnEvent::Completed("Готово.".to_owned())]]);
    let clock = Arc::new(std::sync::atomic::AtomicI64::new(NOW));
    let tick = Arc::clone(&clock);
    let chat = Chat::start(Options {
        default_session: DEFAULT_SESSION.to_owned(),
        store: Arc::new(Store::in_memory().expect("store")),
        hermes: Arc::clone(&hermes) as Arc<dyn HermesApi>,
        media: None,
        stt: None,
        push: None,
        notify: super::Notify::default(),
        now: Arc::new(move || tick.fetch_add(1000, Ordering::SeqCst)),
        new_id: sequential_ids(),
    })
    .expect("chat");
    let app: Router = super::router(Arc::clone(&chat));

    let (status, created) = call(
        &app,
        "POST",
        "/v1/chat/sessions",
        Some(json!({"title": " Шопинг "})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(created["title"], "Шопинг");
    let id = created["id"].as_str().expect("id").to_owned();
    assert!(id.starts_with(super::APP_SESSION_PREFIX), "{id}");

    let (status, sent) = call(
        &app,
        "POST",
        "/v1/chat/messages",
        Some(json!({"clientId": "c-1", "sessionId": id, "text": "Купи молоко"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(sent["message"]["sessionId"], id.as_str());
    settled(&chat, sent["reply"]["id"].as_str().expect("reply")).await;
    assert_eq!(
        hermes.sessions.lock().expect("sessions").clone(),
        vec![id.clone()]
    );

    let (_, list) = call(&app, "GET", "/v1/chat/sessions", None).await;
    let sessions = list["sessions"].as_array().expect("sessions");
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions[0]["id"], id.as_str());
    assert_eq!(sessions[0]["preview"]["text"], "Готово.");
    assert_eq!(sessions[0]["preview"]["role"], "assistant");
    assert_eq!(sessions[1]["id"], DEFAULT_SESSION);
    assert_eq!(sessions[1]["title"], super::LEGACY_SESSION_TITLE);

    let (_, own) = call(
        &app,
        "GET",
        &format!("/v1/chat/messages?session={id}"),
        None,
    )
    .await;
    assert_eq!(own["messages"].as_array().expect("messages").len(), 2);
    let (_, legacy) = call(&app, "GET", "/v1/chat/messages", None).await;
    assert_eq!(legacy["messages"].as_array().expect("messages").len(), 0);
    let (status, missing) = call(&app, "GET", "/v1/chat/messages?session=s_missing", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(missing["code"], "chat_session_not_found");
    let (status, _) = call(
        &app,
        "POST",
        "/v1/chat/messages",
        Some(json!({"clientId": "c-2", "sessionId": "s_missing", "text": "?"})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn sessions_can_be_renamed_pinned_archived_and_deleted() {
    let hermes = ScriptedHermes::with(Vec::new());
    let chat = chat_with(
        Arc::new(Store::in_memory().expect("store")),
        Arc::clone(&hermes),
    );
    let app: Router = super::router(Arc::clone(&chat));
    let (_, created) = call(&app, "POST", "/v1/chat/sessions", None).await;
    let id = created["id"].as_str().expect("id").to_owned();
    assert!(created.get("title").is_none());
    let path = format!("/v1/chat/sessions/{id}");

    let (status, renamed) = call(&app, "PATCH", &path, Some(json!({"title": "Отпуск"}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(renamed["title"], "Отпуск");
    let (status, invalid) = call(&app, "PATCH", &path, Some(json!({"title": "  "}))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(invalid["code"], "chat_title_invalid");

    let (_, pinned) = call(&app, "PATCH", &path, Some(json!({"pinned": true}))).await;
    assert_eq!(pinned["pinned"], true);
    let (_, list) = call(&app, "GET", "/v1/chat/sessions", None).await;
    assert_eq!(list["sessions"][0]["id"], id.as_str());

    call(&app, "PATCH", &path, Some(json!({"archived": true}))).await;
    let (_, active) = call(&app, "GET", "/v1/chat/sessions", None).await;
    assert!(
        active["sessions"]
            .as_array()
            .expect("sessions")
            .iter()
            .all(|s| s["id"] != id.as_str())
    );
    let (_, archived) = call(&app, "GET", "/v1/chat/sessions?archived=true", None).await;
    assert_eq!(archived["sessions"][0]["id"], id.as_str());

    let (status, _) = call(&app, "DELETE", &path, None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = call(&app, "DELETE", &path, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    settle_tasks().await;
    assert_eq!(
        hermes.calls.lock().expect("calls").clone(),
        vec![
            format!("patch {id} {{\"title\":\"Отпуск\"}}"),
            format!("delete {id}")
        ]
    );
}

#[tokio::test]
async fn a_streaming_reply_can_be_stopped_and_retried() {
    let hermes = ScriptedHermes::with(vec![
        vec![
            TurnEvent::Delta("Начинаю".to_owned()),
            TurnEvent::Delta(HANG.to_owned()),
        ],
        vec![TurnEvent::Completed("Заново.".to_owned())],
    ]);
    let chat = chat_with(Arc::new(Store::in_memory().expect("store")), hermes);
    let app: Router = super::router(Arc::clone(&chat));
    let (_, sent) = call(
        &app,
        "POST",
        "/v1/chat/messages",
        Some(json!({"clientId": "c-1", "text": "Расскажи длинно"})),
    )
    .await;
    let reply = sent["reply"]["id"].as_str().expect("reply").to_owned();
    for _ in 0..200 {
        if chat.live_text(&reply).as_deref() == Some("Начинаю") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let (status, _) = call(
        &app,
        "POST",
        &format!("/v1/chat/messages/{reply}/stop"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let stopped = settled(&chat, &reply).await;
    assert_eq!(stopped.status, Status::Failed);
    assert_eq!(stopped.error, Some(ErrorKey::ChatStopped));
    assert_eq!(stopped.text, "Начинаю");
    let (status, body) = call(
        &app,
        "POST",
        &format!("/v1/chat/messages/{reply}/stop"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "chat_not_stoppable");

    call(
        &app,
        "POST",
        &format!("/v1/chat/messages/{reply}/retry"),
        None,
    )
    .await;
    assert_eq!(settled(&chat, &reply).await.text, "Заново.");
}

#[tokio::test]
async fn tool_activity_and_titles_reach_the_session_stream() {
    let hermes = ScriptedHermes::with(vec![
        vec![
            TurnEvent::Tool {
                name: "ha_call".to_owned(),
                done: false,
            },
            TurnEvent::Tool {
                name: "ha_call".to_owned(),
                done: true,
            },
            TurnEvent::Completed("Включил.".to_owned()),
        ],
        vec![TurnEvent::Completed("Куплю.".to_owned())],
    ]);
    *hermes.title.lock().expect("title") = Some("Свет в гостиной".to_owned());
    let chat = chat_with(
        Arc::new(Store::in_memory().expect("store")),
        Arc::clone(&hermes),
    );
    let created = chat
        .create_session(&super::model::CreateSession::default())
        .expect("session");
    let mut events = chat.subscribe();
    let sent = chat
        .send(&SendRequest {
            session_id: Some(created.id.clone()),
            ..request("c-1", "Включи свет\nв гостиной", None)
        })
        .expect("send");
    settled(&chat, &sent.reply.id).await;
    settle_tasks().await;
    let mut tools = Vec::new();
    let mut titles = Vec::new();
    while let Ok(event) = events.try_recv() {
        match event {
            Event::Tool(tool) => tools.push((tool.session_id, tool.name, tool.done)),
            Event::Session(session) => titles.push(session.title),
            _ => {}
        }
    }
    assert_eq!(
        tools,
        vec![
            (created.id.clone(), "ha_call".to_owned(), false),
            (created.id.clone(), "ha_call".to_owned(), true)
        ]
    );
    assert_eq!(titles, vec![Some("Свет в гостиной".to_owned())]);

    *hermes.title.lock().expect("title") = None;
    let second = chat
        .create_session(&super::model::CreateSession::default())
        .expect("session");
    let reply = chat
        .send(&SendRequest {
            session_id: Some(second.id.clone()),
            ..request(
                "c-2",
                "  \nКупи хлеба и молока, пожалуйста, до вечера сегодня",
                None,
            )
        })
        .expect("send")
        .reply;
    settled(&chat, &reply.id).await;
    settle_tasks().await;
    let titled = chat
        .store
        .session(&second.id)
        .expect("store")
        .expect("session");
    assert_eq!(
        titled.title.as_deref(),
        Some("Купи хлеба и молока, пожалуйста, до вече…")
    );
}

#[tokio::test]
async fn the_event_stream_can_follow_one_session() {
    let filter = super::Filter {
        alerts: true,
        session: Some("s_1".to_owned()),
    };
    let delta = |session: &str| {
        Event::Delta(super::model::Delta {
            id: "m_1".to_owned(),
            session_id: session.to_owned(),
            offset: 0,
            text: "x".to_owned(),
        })
    };
    assert!(filter.admits(&delta("s_1")));
    assert!(!filter.admits(&delta("s_2")));
    assert!(
        super::Filter {
            alerts: true,
            session: None
        }
        .admits(&delta("s_2"))
    );
}

#[tokio::test]
async fn a_legacy_transcript_becomes_the_default_session() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("chat.db");
    {
        let legacy = rusqlite::Connection::open(&path).expect("open");
        legacy
            .execute_batch(
                "CREATE TABLE chat_messages (seq INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT NOT NULL UNIQUE, \
                 version INTEGER NOT NULL, client_id TEXT UNIQUE, role TEXT NOT NULL, kind TEXT NOT NULL, \
                 text TEXT NOT NULL, reply_to TEXT, request_id TEXT, status TEXT NOT NULL, error TEXT, \
                 created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL); \
                 INSERT INTO chat_messages (id, version, role, kind, text, status, created_at, updated_at) \
                 VALUES ('m_old', 1, 'user', 'text', 'Старый вопрос', 'done', 1, 1);",
            )
            .expect("legacy schema");
    }
    let chat = chat_with(
        Arc::new(Store::open(&path).expect("store")),
        ScriptedHermes::with(Vec::new()),
    );
    let page = chat.page(None, None).expect("page");
    assert_eq!(page.messages.len(), 1);
    assert_eq!(page.messages[0].session_id, DEFAULT_SESSION);
    let sessions = chat.sessions(false).expect("sessions").sessions;
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, DEFAULT_SESSION);
    assert_eq!(
        sessions[0].preview.as_ref().map(|p| p.text.as_str()),
        Some("Старый вопрос")
    );
}
