pub mod hermes;
pub mod model;
pub mod store;

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{Response, StatusCode, header};
use axum::response::IntoResponse;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::routing::{get, post};
use futures::stream::{self, Stream, StreamExt as _};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};

use hermes::{HermesApi, TurnEvent};
use model::{
    DEFAULT_PAGE, Delta, ErrorBody, ErrorKey, MAX_CLIENT_ID_CHARS, MAX_PAGE, MAX_TEXT_CHARS,
    Message, Page, Role, SendRequest, SendResponse, Status,
};
use store::{NewTurn, Store, StoreError, Update};

pub const PERSIST_EVERY: Duration = Duration::from_secs(2);
pub const KEEPALIVE: Duration = Duration::from_secs(15);
pub const SNAPSHOT_LIMIT: usize = 500;
pub const QUOTE_FOR_MODEL_CHARS: usize = 1500;
const EVENTS_CAPACITY: usize = 512;

pub type Now = Arc<dyn Fn() -> i64 + Send + Sync>;
pub type NewId = Arc<dyn Fn() -> String + Send + Sync>;

#[derive(Debug, Clone)]
pub enum Event {
    Message(Message),
    Delta(Delta),
}

pub struct Options {
    pub store: Arc<Store>,
    pub hermes: Arc<dyn HermesApi>,
    pub now: Now,
    pub new_id: NewId,
}

pub struct Chat {
    store: Arc<Store>,
    hermes: Arc<dyn HermesApi>,
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
            now: options.now,
            new_id: options.new_id,
            events,
            queue,
            live: Mutex::new(HashMap::new()),
        });
        let (pending, failed) = chat.store.recover((chat.now)())?;
        failed
            .into_iter()
            .for_each(|message| chat.publish(Event::Message(message)));
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
        match text.chars().count() {
            0 => return Err(ErrorKey::ChatTextEmpty),
            count if count > MAX_TEXT_CHARS => return Err(ErrorKey::ChatTextTooLong),
            _ => {}
        }
        let turn = NewTurn {
            client_id,
            text,
            reply_to: request.reply_to.as_deref().filter(|id| !id.is_empty()),
            user_id: (self.new_id)(),
            reply_id: (self.new_id)(),
            now: (self.now)(),
        };
        let (message, reply, created) = self.store.insert_turn(&turn).map_err(storage_key)?;
        if created {
            self.publish(Event::Message(message.clone()));
            self.publish(Event::Message(reply.clone()));
            self.enqueue(reply.id.clone());
        }
        Ok(SendResponse { message, reply })
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
        self.publish(Event::Message(reset.clone()));
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
            Ok(message) => self.publish(Event::Message(self.with_live(message))),
            Err(error) => tracing::warn!(%error, "chat: update failed"),
        }
    }

    fn prompt_for(&self, request: &Message) -> String {
        let quoted = request
            .reply_to
            .as_ref()
            .and_then(|quote| self.store.get(&quote.id).ok().flatten());
        match quoted {
            Some(quoted) => format!(
                "[Ответ на сообщение ({}): «{}»]\n{}",
                quoted.role.as_str(),
                store::truncate(&quoted.text, QUOTE_FOR_MODEL_CHARS),
                request.text
            ),
            None => request.text.clone(),
        }
    }

    async fn run_turn(&self, reply_id: &str) {
        let Ok(Some(request)) = self.store.request_of(reply_id) else {
            self.update(reply_id, None, Status::Failed, Some(ErrorKey::ChatNotFound));
            return;
        };
        let message = self.prompt_for(&request);
        self.set_live(reply_id, Some(String::new()));
        self.update(reply_id, Some(""), Status::Streaming, None);
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
        .with_state(chat)
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    before: Option<i64>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct EventsQuery {
    after: Option<i64>,
}

async fn list(State(chat): State<Arc<Chat>>, Query(query): Query<ListQuery>) -> Response<Body> {
    respond(chat.page(query.before, query.limit))
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

async fn events(State(chat): State<Arc<Chat>>, Query(query): Query<EventsQuery>) -> Response<Body> {
    let receiver = chat.subscribe();
    let snapshot = match chat.snapshot(query.after.unwrap_or(0)) {
        Ok(snapshot) => snapshot,
        Err(key) => return error_response(key),
    };
    let initial = stream::iter(
        snapshot
            .into_iter()
            .map(|message| Ok(message_event(&message))),
    );
    Sse::new(initial.chain(live_events(receiver)))
        .keep_alive(KeepAlive::new().interval(KEEPALIVE))
        .into_response()
}

fn live_events(
    receiver: broadcast::Receiver<Event>,
) -> impl Stream<Item = Result<SseEvent, Infallible>> {
    stream::unfold(receiver, |mut receiver| async move {
        let event = receiver.recv().await.ok()?;
        Some((Ok(sse_event(&event)), receiver))
    })
}

fn sse_event(event: &Event) -> SseEvent {
    match event {
        Event::Message(message) => message_event(message),
        Event::Delta(delta) => json_event("delta", delta),
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
        ErrorKey::ChatNotFound | ErrorKey::ChatReplyNotFound => StatusCode::NOT_FOUND,
        ErrorKey::ChatNotRetryable => StatusCode::CONFLICT,
        ErrorKey::ChatStorageFailed
        | ErrorKey::ChatUpstreamUnavailable
        | ErrorKey::ChatUpstreamFailed
        | ErrorKey::ChatInterrupted => StatusCode::SERVICE_UNAVAILABLE,
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
