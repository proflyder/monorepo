pub mod alerts;
pub mod hermes;
pub mod media;
pub mod model;
pub mod store;
pub mod stt;

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::Extension;
use axum::Router;
use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::HeaderMap;
use axum::http::{Response, StatusCode, header};
use axum::response::IntoResponse;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::routing::{get, post};
use futures::stream::{self, Stream, StreamExt as _};
use home_app_auth::{Module, Principal};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};

use base64::Engine as _;
use hermes::{HermesApi, TurnEvent};
use media::{Attachment, AttachmentKind, MediaStore};
use model::{
    DEFAULT_PAGE, Delta, ErrorBody, ErrorKey, Kind, MAX_ATTACHMENTS, MAX_CLIENT_ID_CHARS, MAX_PAGE,
    MAX_TEXT_CHARS, Message, Page, Role, SendRequest, SendResponse, Status,
};
use serde_json::{Value, json};
use store::{NewTurn, Store, StoreError, Update};
use stt::SpeechToText;

use crate::push::PushApi;

pub const PERSIST_EVERY: Duration = Duration::from_secs(2);
pub const KEEPALIVE: Duration = Duration::from_secs(15);
pub const SNAPSHOT_LIMIT: usize = 500;
pub const QUOTE_FOR_MODEL_CHARS: usize = 1500;
pub const PUSH_BODY_CHARS: usize = 300;
pub const DEFAULT_ASSISTANT_TITLE: &str = "Рикардо";
const EVENTS_CAPACITY: usize = 512;

pub type Now = Arc<dyn Fn() -> i64 + Send + Sync>;
pub type NewId = Arc<dyn Fn() -> String + Send + Sync>;

#[derive(Debug, Clone)]
pub enum Event {
    Message(Box<Message>),
    Delta(Delta),
}

pub struct Options {
    pub store: Arc<Store>,
    pub hermes: Arc<dyn HermesApi>,
    pub media: Option<Arc<MediaStore>>,
    pub stt: Option<Arc<dyn SpeechToText>>,
    pub push: Option<Arc<dyn PushApi>>,
    pub notify: Notify,
    pub now: Now,
    pub new_id: NewId,
}

#[derive(Debug, Clone)]
pub struct Notify {
    pub assistant_title: String,
    pub zone: jiff::tz::TimeZone,
}

impl Default for Notify {
    fn default() -> Self {
        Self {
            assistant_title: DEFAULT_ASSISTANT_TITLE.to_owned(),
            zone: jiff::tz::TimeZone::UTC,
        }
    }
}

pub struct Chat {
    push: Option<Arc<dyn PushApi>>,
    notify: Notify,
    store: Arc<Store>,
    hermes: Arc<dyn HermesApi>,
    media: Option<Arc<MediaStore>>,
    stt: Option<Arc<dyn SpeechToText>>,
    now: Now,
    new_id: NewId,
    events: broadcast::Sender<Event>,
    queue: mpsc::UnboundedSender<String>,
    live: Mutex<HashMap<String, String>>,
}

#[must_use]
pub fn system_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

#[must_use]
pub fn random_id() -> String {
    format!("m_{}", uuid::Uuid::now_v7().simple())
}

#[must_use]
pub fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

impl Chat {
    pub fn start(options: Options) -> Result<Arc<Self>, StoreError> {
        let (events, _) = broadcast::channel(EVENTS_CAPACITY);
        let (queue, mut receiver) = mpsc::unbounded_channel::<String>();
        let chat = Arc::new(Self {
            store: options.store,
            hermes: options.hermes,
            media: options.media,
            stt: options.stt,
            push: options.push,
            notify: options.notify,
            now: options.now,
            new_id: options.new_id,
            events,
            queue,
            live: Mutex::new(HashMap::new()),
        });
        let (pending, failed) = chat.store.recover((chat.now)())?;
        failed
            .into_iter()
            .for_each(|message| chat.publish(Event::Message(Box::new(message))));
        pending.into_iter().for_each(|id| chat.enqueue(id));
        let worker = Arc::clone(&chat);
        tokio::spawn(async move {
            while let Some(reply_id) = receiver.recv().await {
                worker.run_turn(&reply_id).await;
            }
        });
        Ok(chat)
    }

    fn enqueue(&self, reply_id: String) {
        let _queued = self.queue.send(reply_id);
    }

    fn publish(&self, event: Event) {
        let _receivers = self.events.send(event);
    }

    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    pub fn send(&self, request: &SendRequest) -> Result<SendResponse, ErrorKey> {
        let text = request.text.trim();
        let client_id = request.client_id.trim();
        validate_client_id(client_id)?;
        if text.chars().count() > MAX_TEXT_CHARS {
            return Err(ErrorKey::ChatTextTooLong);
        }
        let attachments = self.resolve(&request.attachments)?;
        if text.is_empty() && attachments.is_empty() {
            return Err(ErrorKey::ChatTextEmpty);
        }
        let turn = NewTurn {
            client_id,
            text,
            kind: kind_of(&attachments),
            attachments: &attachments,
            reply_to: request.reply_to.as_deref().filter(|id| !id.is_empty()),
            user_id: (self.new_id)(),
            reply_id: (self.new_id)(),
            now: (self.now)(),
        };
        let (message, reply, created) = self.store.insert_turn(&turn).map_err(storage_key)?;
        if created {
            self.publish(Event::Message(Box::new(message.clone())));
            self.publish(Event::Message(Box::new(reply.clone())));
            self.enqueue(reply.id.clone());
        }
        Ok(SendResponse { message, reply })
    }

    pub fn ingest(&self, webhook: &alerts::Webhook) -> usize {
        webhook
            .alerts
            .iter()
            .take(alerts::MAX_ALERTS)
            .filter_map(alerts::WebhookAlert::card)
            .filter(|card| self.ingest_card(card))
            .count()
    }

    fn ingest_card(&self, card: &alerts::AlertCard) -> bool {
        let text = card.text(&self.notify.zone);
        match self
            .store
            .upsert_alert(&(self.new_id)(), card, &text, (self.now)())
        {
            Ok((message, created)) => {
                self.publish(Event::Message(Box::new(message.clone())));
                if created && card.status == alerts::AlertStatus::Firing {
                    self.push_alert(card, &message.id);
                }
                true
            }
            Err(error) => {
                tracing::warn!(%error, "chat: alert not stored");
                false
            }
        }
    }

    fn push_alert(&self, card: &alerts::AlertCard, id: &str) {
        let icon = if card.severity == "critical" {
            "🔴"
        } else {
            "🟠"
        };
        let body = if card.summary.is_empty() {
            card.name.clone()
        } else {
            card.summary.clone()
        };
        self.push_message(format!("{icon} {}", card.name), &body, id);
    }

    fn push_message(&self, title: String, body: &str, id: &str) {
        let Some(push) = self.push.clone() else {
            return;
        };
        let body = store::truncate(body.trim(), PUSH_BODY_CHARS);
        if body.is_empty() {
            return;
        }
        let link = format!("home://chat/message/{id}");
        tokio::spawn(async move {
            if let Err(error) = push.notify(&title, &body, Some(&link), None).await {
                tracing::warn!(%error, "chat: push failed");
            }
        });
    }

    fn resolve(&self, ids: &[String]) -> Result<Vec<Attachment>, ErrorKey> {
        if ids.len() > MAX_ATTACHMENTS {
            return Err(ErrorKey::ChatTooManyAttachments);
        }
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let media = self
            .media
            .as_ref()
            .ok_or(ErrorKey::ChatAttachmentUnsupported)?;
        let attachments = ids
            .iter()
            .map(|id| media.get(id))
            .collect::<Result<Vec<_>, _>>()?;
        let voice = attachments.iter().any(|a| a.kind == AttachmentKind::Voice);
        if voice && attachments.len() > 1 {
            return Err(ErrorKey::ChatTooManyAttachments);
        }
        Ok(attachments)
    }

    pub fn upload(&self, mime: &str, name: &str, bytes: &[u8]) -> Result<Attachment, ErrorKey> {
        let media = self
            .media
            .as_ref()
            .ok_or(ErrorKey::ChatAttachmentUnsupported)?;
        let id = format!("f_{}", (self.new_id)().trim_start_matches("m_"));
        media.save(&id, mime, name, bytes)
    }

    pub fn download(&self, id: &str) -> Result<(Attachment, Vec<u8>), ErrorKey> {
        let media = self
            .media
            .as_ref()
            .ok_or(ErrorKey::ChatAttachmentNotFound)?;
        Ok((media.get(id)?, media.bytes(id)?))
    }

    pub fn retry(&self, id: &str) -> Result<Message, ErrorKey> {
        let message = self
            .store
            .get(id)
            .map_err(storage_key)?
            .ok_or(ErrorKey::ChatNotFound)?;
        if message.role != Role::Assistant || message.status != Status::Failed {
            return Err(ErrorKey::ChatNotRetryable);
        }
        let reset = self
            .store
            .update(
                id,
                &Update {
                    text: Some(""),
                    status: Status::Pending,
                    error: None,
                    now: (self.now)(),
                },
            )
            .map_err(storage_key)?;
        self.publish(Event::Message(Box::new(reset.clone())));
        self.enqueue(id.to_owned());
        Ok(reset)
    }

    pub fn page(&self, before: Option<i64>, limit: Option<usize>) -> Result<Page, ErrorKey> {
        let limit = limit.unwrap_or(DEFAULT_PAGE).clamp(1, MAX_PAGE);
        let cursor = self.store.cursor().map_err(storage_key)?;
        let (messages, has_more) = self.store.page_before(before, limit).map_err(storage_key)?;
        Ok(Page {
            messages: messages
                .into_iter()
                .map(|message| self.with_live(message))
                .collect(),
            has_more,
            cursor,
        })
    }

    pub fn snapshot(&self, after: i64) -> Result<Vec<Message>, ErrorKey> {
        let changed = self
            .store
            .changed_after(after, SNAPSHOT_LIMIT)
            .map_err(storage_key)?;
        Ok(changed
            .into_iter()
            .map(|message| self.with_live(message))
            .collect())
    }

    fn with_live(&self, mut message: Message) -> Message {
        if message.status == Status::Streaming
            && let Some(text) = self.live_text(&message.id)
        {
            message.text = text;
        }
        message
    }

    fn live_text(&self, id: &str) -> Option<String> {
        self.live
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .cloned()
    }

    fn set_live(&self, id: &str, text: Option<String>) {
        let mut live = self.live.lock().unwrap_or_else(PoisonError::into_inner);
        match text {
            Some(text) => live.insert(id.to_owned(), text),
            None => live.remove(id),
        };
    }

    fn update(&self, id: &str, text: Option<&str>, status: Status, error: Option<ErrorKey>) {
        let update = Update {
            text,
            status,
            error,
            now: (self.now)(),
        };
        match self.store.update(id, &update) {
            Ok(message) => self.publish(Event::Message(Box::new(self.with_live(message)))),
            Err(error) => tracing::warn!(%error, "chat: update failed"),
        }
    }

    fn quote_prefix(&self, request: &Message) -> String {
        let quoted = request
            .reply_to
            .as_ref()
            .and_then(|quote| self.store.get(&quote.id).ok().flatten());
        quoted.map_or_else(String::new, |quoted| {
            let text = quoted.transcript.as_deref().unwrap_or(&quoted.text);
            format!(
                "[Ответ на сообщение ({}): «{}»]\n",
                quoted.role.as_str(),
                store::truncate(text, QUOTE_FOR_MODEL_CHARS)
            )
        })
    }

    async fn content_for(&self, request: &Message) -> Result<Value, ErrorKey> {
        let mut text = self.quote_prefix(request);
        let mut images = Vec::new();
        for attachment in &request.attachments {
            match attachment.kind {
                AttachmentKind::Voice => {
                    text.push_str(&self.voice_text(request, attachment).await?);
                }
                AttachmentKind::Image => images.push(self.image_part(attachment)?),
                AttachmentKind::File => text.push_str(&self.file_text(attachment)?),
            }
        }
        if !request.text.is_empty() {
            text.push_str(&request.text);
        }
        if images.is_empty() {
            return Ok(Value::String(text));
        }
        let mut parts = vec![json!({"type": "text", "text": text})];
        parts.extend(images);
        Ok(Value::Array(parts))
    }

    async fn voice_text(
        &self,
        request: &Message,
        attachment: &Attachment,
    ) -> Result<String, ErrorKey> {
        let transcript = match request.transcript.as_deref() {
            Some(known) => known.to_owned(),
            None => self.transcribe(request, attachment).await?,
        };
        Ok(format!("[Голосовое сообщение]: {transcript}\n"))
    }

    async fn transcribe(
        &self,
        request: &Message,
        attachment: &Attachment,
    ) -> Result<String, ErrorKey> {
        let stt = self.stt.as_ref().ok_or(ErrorKey::ChatSttFailed)?;
        let media = self
            .media
            .as_ref()
            .ok_or(ErrorKey::ChatAttachmentNotFound)?;
        let pcm = media::parse_wav(&media.bytes(&attachment.id)?)?;
        let transcript = stt.transcribe(&pcm).await.map_err(|error| {
            tracing::warn!(%error, "chat: speech-to-text failed");
            ErrorKey::ChatSttFailed
        })?;
        if transcript.is_empty() {
            return Err(ErrorKey::ChatSttFailed);
        }
        match self
            .store
            .set_transcript(&request.id, &transcript, (self.now)())
        {
            Ok(message) => self.publish(Event::Message(Box::new(message))),
            Err(error) => tracing::warn!(%error, "chat: transcript not saved"),
        }
        Ok(transcript)
    }

    fn image_part(&self, attachment: &Attachment) -> Result<Value, ErrorKey> {
        let media = self
            .media
            .as_ref()
            .ok_or(ErrorKey::ChatAttachmentNotFound)?;
        let encoded =
            base64::engine::general_purpose::STANDARD.encode(media.bytes(&attachment.id)?);
        Ok(json!({
            "type": "image_url",
            "image_url": {"url": format!("data:{};base64,{encoded}", attachment.mime)}
        }))
    }

    fn file_text(&self, attachment: &Attachment) -> Result<String, ErrorKey> {
        let media = self
            .media
            .as_ref()
            .ok_or(ErrorKey::ChatAttachmentNotFound)?;
        let bytes = media.bytes(&attachment.id)?;
        let content = String::from_utf8_lossy(&bytes);
        Ok(format!("[Файл {}]\n```\n{content}\n```\n", attachment.name))
    }

    async fn run_turn(&self, reply_id: &str) {
        let Ok(Some(request)) = self.store.request_of(reply_id) else {
            self.update(reply_id, None, Status::Failed, Some(ErrorKey::ChatNotFound));
            return;
        };
        self.set_live(reply_id, Some(String::new()));
        self.update(reply_id, Some(""), Status::Streaming, None);
        let message = match self.content_for(&request).await {
            Ok(message) => message,
            Err(key) => {
                self.set_live(reply_id, None);
                self.update(reply_id, Some(""), Status::Failed, Some(key));
                return;
            }
        };
        let (sink, mut events) = mpsc::channel::<TurnEvent>(64);
        let run = self.hermes.run(message, sink);
        let consume = async {
            let mut text = String::new();
            let mut persisted = Instant::now();
            let mut terminal = TurnEvent::Failed(ErrorKey::ChatUpstreamFailed);
            while let Some(event) = events.recv().await {
                match event {
                    TurnEvent::Delta(delta) => {
                        self.on_delta(reply_id, &mut text, &delta, &mut persisted);
                    }
                    other => terminal = other,
                }
            }
            (text, terminal)
        };
        let ((), (text, terminal)) = tokio::join!(run, consume);
        self.set_live(reply_id, None);
        match terminal {
            TurnEvent::Completed(final_text) => {
                self.update(reply_id, Some(&final_text), Status::Done, None);
                self.push_message(self.notify.assistant_title.clone(), &final_text, reply_id);
            }
            TurnEvent::Failed(key) => self.update(reply_id, Some(&text), Status::Failed, Some(key)),
            TurnEvent::Delta(_) => {}
        }
    }

    fn on_delta(&self, reply_id: &str, text: &mut String, delta: &str, persisted: &mut Instant) {
        let offset = utf16_len(text);
        text.push_str(delta);
        self.set_live(reply_id, Some(text.clone()));
        self.publish(Event::Delta(Delta {
            id: reply_id.to_owned(),
            offset,
            text: delta.to_owned(),
        }));
        if persisted.elapsed() >= PERSIST_EVERY {
            *persisted = Instant::now();
            let update = Update {
                text: Some(text),
                status: Status::Streaming,
                error: None,
                now: (self.now)(),
            };
            if let Err(error) = self.store.update(reply_id, &update) {
                tracing::warn!(%error, "chat: streaming checkpoint failed");
            }
        }
    }
}

fn kind_of(attachments: &[Attachment]) -> Kind {
    let has = |kind: AttachmentKind| attachments.iter().any(|a| a.kind == kind);
    match () {
        () if has(AttachmentKind::Voice) => Kind::Voice,
        () if has(AttachmentKind::Image) => Kind::Image,
        () if has(AttachmentKind::File) => Kind::File,
        () => Kind::Text,
    }
}

fn validate_client_id(client_id: &str) -> Result<(), ErrorKey> {
    let valid = !client_id.is_empty()
        && client_id.chars().count() <= MAX_CLIENT_ID_CHARS
        && client_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    valid.then_some(()).ok_or(ErrorKey::ChatClientIdInvalid)
}

fn storage_key(error: StoreError) -> ErrorKey {
    match error {
        StoreError::ReplyNotFound => ErrorKey::ChatReplyNotFound,
        StoreError::NotFound => ErrorKey::ChatNotFound,
        StoreError::Sqlite(error) => {
            tracing::warn!(%error, "chat: storage failed");
            ErrorKey::ChatStorageFailed
        }
    }
}

pub fn router<S: Clone + Send + Sync + 'static>(chat: Arc<Chat>) -> Router<S> {
    Router::new()
        .route("/v1/chat/messages", get(list).post(send))
        .route("/v1/chat/messages/{id}/retry", post(retry))
        .route("/v1/chat/events", get(events))
        .route(
            "/v1/chat/attachments",
            post(upload).layer(DefaultBodyLimit::max(media::MAX_UPLOAD_BYTES)),
        )
        .route("/v1/chat/attachments/{id}", get(download))
        .with_state(chat)
}

pub fn alerts_router<S: Clone + Send + Sync + 'static>(chat: Arc<Chat>) -> Router<S> {
    Router::new()
        .route("/v1/chat/alerts", post(alert_webhook))
        .with_state(chat)
}

async fn alert_webhook(State(chat): State<Arc<Chat>>, body: axum::body::Bytes) -> Response<Body> {
    serde_json::from_slice::<alerts::Webhook>(&body).map_or_else(
        |_| {
            json(
                StatusCode::BAD_REQUEST,
                &json!({"code": "chat_alert_invalid"}),
            )
        },
        |webhook| json(StatusCode::OK, &json!({"accepted": chat.ingest(&webhook)})),
    )
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    before: Option<i64>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct UploadQuery {
    #[serde(default)]
    name: String,
}

async fn upload(
    State(chat): State<Arc<Chat>>,
    Query(query): Query<UploadQuery>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response<Body> {
    let mime = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    respond(chat.upload(mime, &query.name, &body))
}

async fn download(State(chat): State<Arc<Chat>>, Path(id): Path<String>) -> Response<Body> {
    match chat.download(&id) {
        Ok((attachment, bytes)) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, attachment.mime)
            .header(
                header::CACHE_CONTROL,
                "private, max-age=31536000, immutable",
            )
            .body(Body::from(bytes))
            .unwrap_or_default(),
        Err(key) => error_response(key),
    }
}

#[derive(Debug, Deserialize)]
struct EventsQuery {
    after: Option<i64>,
}

async fn list(
    State(chat): State<Arc<Chat>>,
    Query(query): Query<ListQuery>,
    principal: Option<Extension<Principal>>,
) -> Response<Body> {
    let alerts = alerts_allowed(principal.as_ref());
    respond(chat.page(query.before, query.limit).map(|mut page| {
        if !alerts {
            page.messages.retain(|message| message.kind != Kind::Alert);
        }
        page
    }))
}

async fn send(State(chat): State<Arc<Chat>>, body: axum::body::Bytes) -> Response<Body> {
    serde_json::from_slice::<SendRequest>(&body).map_or_else(
        |_| error_response(ErrorKey::ChatTextEmpty),
        |request| respond(chat.send(&request)),
    )
}

async fn retry(State(chat): State<Arc<Chat>>, Path(id): Path<String>) -> Response<Body> {
    respond(chat.retry(&id))
}

async fn events(
    State(chat): State<Arc<Chat>>,
    Query(query): Query<EventsQuery>,
    principal: Option<Extension<Principal>>,
) -> Response<Body> {
    let alerts = alerts_allowed(principal.as_ref());
    let receiver = chat.subscribe();
    let snapshot = match chat.snapshot(query.after.unwrap_or(0)) {
        Ok(snapshot) => snapshot,
        Err(key) => return error_response(key),
    };
    let initial = stream::iter(
        snapshot
            .into_iter()
            .filter(move |message| alerts || message.kind != Kind::Alert)
            .map(|message| Ok(message_event(&message))),
    );
    Sse::new(initial.chain(live_events(receiver, alerts)))
        .keep_alive(KeepAlive::new().interval(KEEPALIVE))
        .into_response()
}

fn alerts_allowed(principal: Option<&Extension<Principal>>) -> bool {
    principal.is_none_or(|principal| principal.0.allows(Module::Alerts))
}

fn live_events(
    receiver: broadcast::Receiver<Event>,
    alerts: bool,
) -> impl Stream<Item = Result<SseEvent, Infallible>> {
    stream::unfold((receiver, alerts), |(mut receiver, alerts)| async move {
        loop {
            let event = receiver.recv().await.ok()?;
            if let Some(event) = sse_event(&event, alerts) {
                return Some((Ok(event), (receiver, alerts)));
            }
        }
    })
}

fn sse_event(event: &Event, alerts: bool) -> Option<SseEvent> {
    match event {
        Event::Message(message) if alerts || message.kind != Kind::Alert => {
            Some(message_event(message))
        }
        Event::Message(_) => None,
        Event::Delta(delta) => Some(json_event("delta", delta)),
    }
}

fn message_event(message: &Message) -> SseEvent {
    json_event("message", message).id(message.version.to_string())
}

fn json_event<T: Serialize>(name: &str, value: &T) -> SseEvent {
    SseEvent::default()
        .event(name)
        .data(serde_json::to_string(value).unwrap_or_default())
}

fn respond<T: Serialize>(result: Result<T, ErrorKey>) -> Response<Body> {
    match result {
        Ok(value) => json(StatusCode::OK, &value),
        Err(key) => error_response(key),
    }
}

fn error_response(key: ErrorKey) -> Response<Body> {
    let status = match key {
        ErrorKey::ChatNotFound | ErrorKey::ChatReplyNotFound | ErrorKey::ChatAttachmentNotFound => {
            StatusCode::NOT_FOUND
        }
        ErrorKey::ChatAttachmentTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
        ErrorKey::ChatAttachmentUnsupported
        | ErrorKey::ChatTooManyAttachments
        | ErrorKey::ChatVoiceFormat => StatusCode::BAD_REQUEST,
        ErrorKey::ChatNotRetryable => StatusCode::CONFLICT,
        ErrorKey::ChatStorageFailed
        | ErrorKey::ChatUpstreamUnavailable
        | ErrorKey::ChatUpstreamFailed
        | ErrorKey::ChatInterrupted
        | ErrorKey::ChatSttFailed => StatusCode::SERVICE_UNAVAILABLE,
        ErrorKey::ChatTextEmpty | ErrorKey::ChatTextTooLong | ErrorKey::ChatClientIdInvalid => {
            StatusCode::BAD_REQUEST
        }
    };
    json(status, &ErrorBody { code: key })
}

fn json<T: Serialize>(status: StatusCode, value: &T) -> Response<Body> {
    let body = serde_json::to_vec(value).unwrap_or_default();
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(body))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
