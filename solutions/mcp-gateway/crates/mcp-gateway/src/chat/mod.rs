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
use axum::routing::{get, patch, post};
use futures::stream::{self, Stream, StreamExt as _};
use home_app_auth::{Module, Principal};
use serde::{Deserialize, Serialize};
use tokio::sync::{Notify as Signal, broadcast, mpsc};

use base64::Engine as _;
use hermes::{HermesApi, Turn, TurnEvent};
use media::{Attachment, AttachmentKind, MediaStore};
use model::{
    AUTO_TITLE_CHARS, CreateSession, DEFAULT_PAGE, Delta, ErrorBody, ErrorKey, Kind,
    MAX_ATTACHMENTS, MAX_CLIENT_ID_CHARS, MAX_PAGE, MAX_TEXT_CHARS, MAX_TITLE_CHARS, Message, Page,
    Role, SendRequest, SendResponse, Session, SessionList, SessionPatch, Status, Tool,
};
use serde_json::{Value, json};
use store::{NewTurn, Scope, Store, StoreError, Update};
use stt::SpeechToText;

use crate::push::PushApi;

pub const PERSIST_EVERY: Duration = Duration::from_secs(2);
pub const KEEPALIVE: Duration = Duration::from_secs(15);
pub const SNAPSHOT_LIMIT: usize = 500;
pub const QUOTE_FOR_MODEL_CHARS: usize = 1500;
pub const PUSH_BODY_CHARS: usize = 300;
pub const DEFAULT_ASSISTANT_TITLE: &str = "Рикардо";
pub const LEGACY_SESSION_TITLE: &str = "Главный чат";
pub const APP_SESSION_PREFIX: &str = "app_";
const EVENTS_CAPACITY: usize = 512;

pub type Now = Arc<dyn Fn() -> i64 + Send + Sync>;
pub type NewId = Arc<dyn Fn() -> String + Send + Sync>;

#[derive(Debug, Clone)]
pub enum Event {
    Message(Box<Message>),
    Delta(Delta),
    Tool(Tool),
    Session(Box<Session>),
    SessionRemoved(String),
}

impl Event {
    fn session_id(&self) -> &str {
        match self {
            Self::Message(message) => &message.session_id,
            Self::Delta(delta) => &delta.session_id,
            Self::Tool(tool) => &tool.session_id,
            Self::Session(session) => &session.id,
            Self::SessionRemoved(id) => id,
        }
    }
}

pub struct Options {
    pub default_session: String,
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
    default_session: String,
    running: Mutex<HashMap<String, Arc<Signal>>>,
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
        options.store.adopt_legacy(
            &options.default_session,
            LEGACY_SESSION_TITLE,
            (options.now)(),
        )?;
        let chat = Arc::new(Self {
            default_session: options.default_session,
            running: Mutex::new(HashMap::new()),
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
        let session_id = self.session_or_default(request.session_id.as_deref());
        let turn = NewTurn {
            session_id,
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

    fn session_or_default<'a>(&'a self, requested: Option<&'a str>) -> &'a str {
        requested
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .unwrap_or(&self.default_session)
    }

    pub fn sessions(&self, archived: bool) -> Result<SessionList, ErrorKey> {
        let sessions = self.store.sessions(archived).map_err(storage_key)?;
        Ok(SessionList { sessions })
    }

    pub fn create_session(&self, request: &CreateSession) -> Result<Session, ErrorKey> {
        let title = request.title.as_deref().map(valid_title).transpose()?;
        let id = format!(
            "{APP_SESSION_PREFIX}{}",
            (self.new_id)().trim_start_matches("m_")
        );
        let session = self
            .store
            .create_session(&id, title, (self.now)())
            .map_err(storage_key)?;
        self.publish(Event::Session(Box::new(session.clone())));
        Ok(session)
    }

    pub fn patch_session(&self, id: &str, patch: &SessionPatch) -> Result<Session, ErrorKey> {
        let title = patch.title.as_deref().map(valid_title).transpose()?;
        let normalized = SessionPatch {
            title: title.map(str::to_owned),
            ..patch.clone()
        };
        let session = self
            .store
            .patch_session(id, &normalized, (self.now)())
            .map_err(storage_key)?;
        if let Some(title) = title {
            self.mirror_title(id, title);
        }
        self.publish(Event::Session(Box::new(session.clone())));
        Ok(session)
    }

    pub fn delete_session(&self, id: &str) -> Result<(), ErrorKey> {
        self.stop_session(id);
        let attachments = self.store.delete_session(id).map_err(storage_key)?;
        if let Some(media) = &self.media {
            attachments
                .iter()
                .for_each(|attachment| media.remove(&attachment.id));
        }
        self.publish(Event::SessionRemoved(id.to_owned()));
        let hermes = Arc::clone(&self.hermes);
        let session = id.to_owned();
        tokio::spawn(async move {
            if let Err(key) = hermes.delete(&session).await {
                tracing::warn!(error = key.as_str(), "chat: hermes session not deleted");
            }
        });
        Ok(())
    }

    fn mirror_title(&self, id: &str, title: &str) {
        let hermes = Arc::clone(&self.hermes);
        let session = id.to_owned();
        let fields = json!({"title": title});
        tokio::spawn(async move {
            if let Err(key) = hermes.patch(&session, fields).await {
                tracing::warn!(error = key.as_str(), "chat: hermes title not mirrored");
            }
        });
    }

    fn stop_session(&self, id: &str) {
        let running: Vec<Arc<Signal>> = self
            .running
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|(reply, _)| {
                self.store
                    .get(reply)
                    .ok()
                    .flatten()
                    .is_some_and(|message| message.session_id == id)
            })
            .map(|(_, signal)| Arc::clone(signal))
            .collect();
        running.iter().for_each(|signal| signal.notify_one());
    }

    pub fn stop(&self, id: &str) -> Result<Message, ErrorKey> {
        let message = self
            .store
            .get(id)
            .map_err(storage_key)?
            .ok_or(ErrorKey::ChatNotFound)?;
        if message.role != Role::Assistant {
            return Err(ErrorKey::ChatNotStoppable);
        }
        match message.status {
            Status::Pending => Ok(self.stopped(id, None)?),
            Status::Streaming => {
                self.signal_of(id)
                    .ok_or(ErrorKey::ChatNotStoppable)?
                    .notify_one();
                Ok(message)
            }
            Status::Done | Status::Failed => Err(ErrorKey::ChatNotStoppable),
        }
    }

    fn signal_of(&self, id: &str) -> Option<Arc<Signal>> {
        self.running
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .cloned()
    }

    fn stopped(&self, id: &str, text: Option<&str>) -> Result<Message, ErrorKey> {
        let update = Update {
            text,
            status: Status::Failed,
            error: Some(ErrorKey::ChatStopped),
            now: (self.now)(),
        };
        let message = self.store.update(id, &update).map_err(storage_key)?;
        self.publish(Event::Message(Box::new(message.clone())));
        Ok(message)
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
        match self.store.upsert_alert(
            &self.default_session,
            &(self.new_id)(),
            card,
            &text,
            (self.now)(),
        ) {
            Ok((message, created)) => {
                self.publish(Event::Message(Box::new(message.clone())));
                if created && card.status == alerts::AlertStatus::Firing {
                    self.push_alert(card, &message);
                }
                true
            }
            Err(error) => {
                tracing::warn!(%error, "chat: alert not stored");
                false
            }
        }
    }

    fn push_alert(&self, card: &alerts::AlertCard, message: &Message) {
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
        self.push_message(
            Module::Alerts,
            format!("{icon} {}", card.name),
            &body,
            message,
        );
    }

    fn push_message(&self, module: Module, title: String, body: &str, message: &Message) {
        let Some(push) = self.push.clone() else {
            return;
        };
        let body = store::truncate(body.trim(), PUSH_BODY_CHARS);
        if body.is_empty() {
            return;
        }
        let link = format!(
            "home://chat/session/{}/message/{}",
            message.session_id, message.id
        );
        tokio::spawn(async move {
            if let Err(error) = push.notify_module(module, &title, &body, Some(&link)).await {
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
        if self.store.request_of(id).map_err(storage_key)?.is_none() {
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
        self.page_for(None, before, limit, true)
    }

    pub fn page_for(
        &self,
        session: Option<&str>,
        before: Option<i64>,
        limit: Option<usize>,
        include_alerts: bool,
    ) -> Result<Page, ErrorKey> {
        let limit = limit.unwrap_or(DEFAULT_PAGE).clamp(1, MAX_PAGE);
        let session = self.session_or_default(session);
        if self.store.session(session).map_err(storage_key)?.is_none() {
            return Err(ErrorKey::ChatSessionNotFound);
        }
        let cursor = self.store.cursor().map_err(storage_key)?;
        let scope = Scope {
            session: Some(session),
            include_alerts,
        };
        let (messages, has_more) = self
            .store
            .page_before(&scope, before, limit)
            .map_err(storage_key)?;
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
        self.snapshot_for(
            &Scope {
                session: None,
                include_alerts: true,
            },
            after,
        )
    }

    pub fn snapshot_for(&self, scope: &Scope<'_>, after: i64) -> Result<Vec<Message>, ErrorKey> {
        let changed = self
            .store
            .changed_after(scope, after, SNAPSHOT_LIMIT)
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
        let Ok(Some(reply)) = self.store.get(reply_id) else {
            return;
        };
        if reply.status != Status::Pending {
            return;
        }
        let Ok(Some(request)) = self.store.request_of(reply_id) else {
            self.update(reply_id, None, Status::Failed, Some(ErrorKey::ChatNotFound));
            return;
        };
        let signal = Arc::new(Signal::new());
        self.running
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(reply_id.to_owned(), Arc::clone(&signal));
        let outcome = tokio::select! {
            outcome = self.stream_turn(&request, reply_id) => Some(outcome),
            () = signal.notified() => None,
        };
        self.running
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(reply_id);
        let partial = self.live_text(reply_id);
        self.set_live(reply_id, None);
        match outcome {
            Some((text, terminal)) => self.finish_turn(&request, reply_id, &text, terminal),
            None => {
                if let Err(key) = self.stopped(reply_id, partial.as_deref()) {
                    tracing::warn!(error = key.as_str(), "chat: stop not stored");
                }
            }
        }
    }

    async fn stream_turn(&self, request: &Message, reply_id: &str) -> (String, TurnEvent) {
        self.set_live(reply_id, Some(String::new()));
        self.update(reply_id, Some(""), Status::Streaming, None);
        let message = match self.content_for(request).await {
            Ok(message) => message,
            Err(key) => return (String::new(), TurnEvent::Failed(key)),
        };
        let title = self
            .store
            .session(&request.session_id)
            .ok()
            .flatten()
            .and_then(|session| session.title);
        let (sink, mut events) = mpsc::channel::<TurnEvent>(64);
        let turn = Turn {
            session: &request.session_id,
            title: title.as_deref(),
            message,
        };
        let run = self.hermes.run(turn, sink);
        let consume = async {
            let mut text = String::new();
            let mut persisted = Instant::now();
            let mut terminal = TurnEvent::Failed(ErrorKey::ChatUpstreamFailed);
            while let Some(event) = events.recv().await {
                match event {
                    TurnEvent::Delta(delta) => {
                        self.on_delta(request, reply_id, &mut text, &delta, &mut persisted);
                    }
                    TurnEvent::Tool { name, done } => self.publish(Event::Tool(Tool {
                        id: reply_id.to_owned(),
                        session_id: request.session_id.clone(),
                        name,
                        done,
                    })),
                    other => terminal = other,
                }
            }
            (text, terminal)
        };
        let ((), outcome) = tokio::join!(run, consume);
        outcome
    }

    fn finish_turn(&self, request: &Message, reply_id: &str, text: &str, terminal: TurnEvent) {
        match terminal {
            TurnEvent::Completed(final_text) => {
                self.update(reply_id, Some(&final_text), Status::Done, None);
                if let Ok(Some(reply)) = self.store.get(reply_id) {
                    self.push_message(
                        Module::Chat,
                        self.notify.assistant_title.clone(),
                        &final_text,
                        &reply,
                    );
                }
                self.ensure_title(&request.session_id);
            }
            TurnEvent::Failed(key) => self.update(reply_id, Some(text), Status::Failed, Some(key)),
            TurnEvent::Delta(_) | TurnEvent::Tool { .. } => {}
        }
    }

    fn ensure_title(&self, session_id: &str) {
        let titled = self
            .store
            .session(session_id)
            .ok()
            .flatten()
            .is_none_or(|session| session.title.is_some());
        if titled {
            return;
        }
        let Some(fallback) = self
            .store
            .first_user_text(session_id)
            .ok()
            .flatten()
            .map(|text| auto_title(&text))
            .filter(|title| !title.is_empty())
        else {
            return;
        };
        let hermes = Arc::clone(&self.hermes);
        let store = Arc::clone(&self.store);
        let events = self.events.clone();
        let session = session_id.to_owned();
        tokio::spawn(async move {
            let title = hermes.title(&session).await.unwrap_or(fallback);
            if store.title_if_missing(&session, &title).unwrap_or(false)
                && let Ok(Some(updated)) = store.session(&session)
            {
                let _receivers = events.send(Event::Session(Box::new(updated)));
            }
        });
    }

    fn on_delta(
        &self,
        request: &Message,
        reply_id: &str,
        text: &mut String,
        delta: &str,
        persisted: &mut Instant,
    ) {
        let offset = utf16_len(text);
        text.push_str(delta);
        self.set_live(reply_id, Some(text.clone()));
        self.publish(Event::Delta(Delta {
            id: reply_id.to_owned(),
            session_id: request.session_id.clone(),
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

fn valid_title(raw: &str) -> Result<&str, ErrorKey> {
    let title = raw.trim();
    let valid = !title.is_empty() && title.chars().count() <= MAX_TITLE_CHARS;
    valid.then_some(title).ok_or(ErrorKey::ChatTitleInvalid)
}

fn auto_title(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|line| !line.is_empty());
    store::truncate(line.unwrap_or_default(), AUTO_TITLE_CHARS)
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
        StoreError::SessionNotFound => ErrorKey::ChatSessionNotFound,
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
        .route("/v1/chat/messages/{id}/stop", post(stop))
        .route("/v1/chat/sessions", get(list_sessions).post(create_session))
        .route(
            "/v1/chat/sessions/{id}",
            patch(patch_session).delete(delete_session),
        )
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
    session: Option<String>,
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
    session: Option<String>,
}

async fn list(
    State(chat): State<Arc<Chat>>,
    Query(query): Query<ListQuery>,
    principal: Option<Extension<Principal>>,
) -> Response<Body> {
    let alerts = alerts_allowed(principal.as_ref());
    respond(chat.page_for(query.session.as_deref(), query.before, query.limit, alerts))
}

#[derive(Debug, Deserialize)]
struct SessionsQuery {
    #[serde(default)]
    archived: bool,
}

async fn list_sessions(
    State(chat): State<Arc<Chat>>,
    Query(query): Query<SessionsQuery>,
) -> Response<Body> {
    respond(chat.sessions(query.archived))
}

async fn create_session(State(chat): State<Arc<Chat>>, body: axum::body::Bytes) -> Response<Body> {
    let request = if body.is_empty() {
        Ok(CreateSession::default())
    } else {
        serde_json::from_slice::<CreateSession>(&body)
    };
    request.map_or_else(
        |_| error_response(ErrorKey::ChatTitleInvalid),
        |request| respond(chat.create_session(&request)),
    )
}

async fn patch_session(
    State(chat): State<Arc<Chat>>,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Response<Body> {
    serde_json::from_slice::<SessionPatch>(&body).map_or_else(
        |_| error_response(ErrorKey::ChatTitleInvalid),
        |patch| respond(chat.patch_session(&id, &patch)),
    )
}

async fn delete_session(State(chat): State<Arc<Chat>>, Path(id): Path<String>) -> Response<Body> {
    match chat.delete_session(&id) {
        Ok(()) => json(StatusCode::OK, &json!({"deleted": id})),
        Err(key) => error_response(key),
    }
}

async fn stop(State(chat): State<Arc<Chat>>, Path(id): Path<String>) -> Response<Body> {
    respond(chat.stop(&id))
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
    let filter = Filter {
        alerts: alerts_allowed(principal.as_ref()),
        session: query.session.filter(|id| !id.trim().is_empty()),
    };
    let receiver = chat.subscribe();
    let scope = Scope {
        session: filter.session.as_deref(),
        include_alerts: filter.alerts,
    };
    let snapshot = match chat.snapshot_for(&scope, query.after.unwrap_or(0)) {
        Ok(snapshot) => snapshot,
        Err(key) => return error_response(key),
    };
    let initial = stream::iter(
        snapshot
            .into_iter()
            .map(|message| Ok(message_event(&message))),
    );
    Sse::new(initial.chain(live_events(receiver, filter)))
        .keep_alive(KeepAlive::new().interval(KEEPALIVE))
        .into_response()
}

fn alerts_allowed(principal: Option<&Extension<Principal>>) -> bool {
    principal
        .as_ref()
        .is_some_and(|principal| principal.0.allows(Module::Alerts))
}

#[derive(Debug, Clone)]
struct Filter {
    alerts: bool,
    session: Option<String>,
}

impl Filter {
    fn admits(&self, event: &Event) -> bool {
        self.session
            .as_deref()
            .is_none_or(|session| session == event.session_id())
    }
}

fn live_events(
    receiver: broadcast::Receiver<Event>,
    filter: Filter,
) -> impl Stream<Item = Result<SseEvent, Infallible>> {
    stream::unfold((receiver, filter), |(mut receiver, filter)| async move {
        loop {
            let event = receiver.recv().await.ok()?;
            if let Some(event) = filter
                .admits(&event)
                .then(|| sse_event(&event, filter.alerts))
                .flatten()
            {
                return Some((Ok(event), (receiver, filter)));
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
        Event::Tool(tool) => Some(json_event("tool", tool)),
        Event::Session(session) => Some(json_event("session", session)),
        Event::SessionRemoved(id) => Some(json_event("session_removed", &json!({"id": id}))),
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
        ErrorKey::ChatNotFound
        | ErrorKey::ChatReplyNotFound
        | ErrorKey::ChatAttachmentNotFound
        | ErrorKey::ChatSessionNotFound => StatusCode::NOT_FOUND,
        ErrorKey::ChatAttachmentTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
        ErrorKey::ChatNotRetryable | ErrorKey::ChatNotStoppable => StatusCode::CONFLICT,
        ErrorKey::ChatStorageFailed
        | ErrorKey::ChatUpstreamUnavailable
        | ErrorKey::ChatUpstreamFailed
        | ErrorKey::ChatInterrupted
        | ErrorKey::ChatStopped
        | ErrorKey::ChatSttFailed => StatusCode::SERVICE_UNAVAILABLE,
        ErrorKey::ChatAttachmentUnsupported
        | ErrorKey::ChatTooManyAttachments
        | ErrorKey::ChatVoiceFormat
        | ErrorKey::ChatTextEmpty
        | ErrorKey::ChatTextTooLong
        | ErrorKey::ChatClientIdInvalid
        | ErrorKey::ChatTitleInvalid => StatusCode::BAD_REQUEST,
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
