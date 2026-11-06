//! The HTTP surface: GET passthrough and the `POST /v1/chat/completions`
//! pipeline.
//!
//! The pipeline order is part of the contract: cancel guard first (before any
//! profile read or routing call), then the optional voice router, then the
//! `reasoning_effort` injection, then either the streaming relay or the
//! buffered sanitising relay.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};
use std::time::Instant;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::header::{
    ACCEPT, AUTHORIZATION, CACHE_CONTROL, CONNECTION, CONTENT_LENGTH, CONTENT_TYPE,
};
use axum::http::{HeaderMap, HeaderValue, Method, Request, StatusCode};
use axum::response::Response;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::domain::cancel::CancelGuard;
use crate::domain::clock::Clock;
use crate::domain::sanitize::{StreamingSanitizer, sanitize_chat_payload};
use crate::openai::{self, ChatChunk, ChatRequest};
use crate::relay::sse;
use crate::relay::{MAX_BODY, Upstream};
use crate::services::VoiceRouter;

/// The id of a proxy-generated local reply.
pub const DEFAULT_COMPLETION_ID: &str = "chatcmpl-voice-local";
/// The id of the local reply that answers a cancel phrase.
///
/// Wire value: rename together with `testdata/contract/cancel_short_circuit.json`.
pub const CANCEL_COMPLETION_ID: &str = "chatcmpl-ricardo-cancel";
/// Cancelled speech: never spoken and never sent upstream. HA's llama_cpp
/// adapter drops an empty assistant message and then raises "Unable to get
/// response"; a whitespace-only body keeps it valid while `tts_input.strip()`
/// still skips TTS.
pub const CANCEL_SILENCE: &str = " ";
/// Message of the OpenAI-shaped upstream error.
pub const UPSTREAM_UNAVAILABLE: &str = "Hermes upstream is unavailable";

/// Options of the proxy handler.
pub struct Options {
    pub upstream: Upstream,
    pub reasoning_effort: String,
    pub max_body: usize,
    pub cancel_phrases: Vec<String>,
    pub router: Option<Arc<VoiceRouter>>,
    pub clock: Arc<dyn Clock>,
}

/// The proxy HTTP handler.
pub struct Proxy {
    upstream: Upstream,
    reasoning_effort: String,
    max_body: usize,
    cancel: CancelGuard,
    router: Option<Arc<VoiceRouter>>,
    clock: Arc<dyn Clock>,
    cancel_count: AtomicU64,
}

impl Proxy {
    /// Builds the handler with the documented defaults.
    #[must_use]
    pub fn new(options: Options) -> Self {
        let phrases: Vec<&str> = options.cancel_phrases.iter().map(String::as_str).collect();
        Self {
            upstream: options.upstream,
            reasoning_effort: if options.reasoning_effort.is_empty() {
                "low".to_owned()
            } else {
                options.reasoning_effort
            },
            max_body: if options.max_body == 0 {
                MAX_BODY
            } else {
                options.max_body
            },
            cancel: CancelGuard::new(&phrases),
            router: options.router,
            clock: options.clock,
            cancel_count: AtomicU64::new(0),
        }
    }

    /// The axum router. Every path and method lands in one fallback, exactly
    /// like the Go `BaseHTTPRequestHandler` subclass.
    pub fn router(self: &Arc<Self>) -> Router {
        Router::new()
            .fallback(dispatch)
            .with_state(Arc::clone(self))
    }

    /// The normalised upstream base URL.
    #[must_use]
    pub fn upstream_base(&self) -> &str {
        self.upstream.base()
    }

    /// How many cancel phrases were answered locally.
    #[must_use]
    pub fn cancel_count(&self) -> u64 {
        self.cancel_count.load(Ordering::Relaxed)
    }
}

async fn dispatch(State(proxy): State<Arc<Proxy>>, request: Request<Body>) -> Response {
    let method = request.method().clone();
    let path_and_query = request
        .uri()
        .path_and_query()
        .map_or_else(|| request.uri().path().to_owned(), ToString::to_string);
    let headers = request.headers().clone();
    match method {
        Method::GET => proxy.handle_get(&path_and_query, &headers).await,
        Method::POST => proxy.handle_post(&path_and_query, request).await,
        other => {
            let body = format!("Unsupported method ({other})\n");
            send(
                StatusCode::NOT_IMPLEMENTED.as_u16(),
                body.into_bytes(),
                "text/plain; charset=utf-8",
            )
        }
    }
}

impl Proxy {
    async fn handle_get(&self, path_and_query: &str, headers: &HeaderMap) -> Response {
        let result = self
            .upstream
            .buffered(
                Method::GET,
                path_and_query,
                None,
                header_value(headers, AUTHORIZATION),
                header_str(headers, CONTENT_TYPE),
                header_str(headers, ACCEPT),
            )
            .await;
        match result {
            Ok(fetch) => send(fetch.status.as_u16(), fetch.body, &fetch.content_type),
            Err(_) => upstream_unavailable(),
        }
    }

    async fn handle_post(&self, path_and_query: &str, request: Request<Body>) -> Response {
        let (parts, body) = request.into_parts();
        let raw = match axum::body::to_bytes(body, self.max_body.saturating_add(1)).await {
            Ok(bytes) => bytes.to_vec(),
            Err(_) => return upstream_unavailable(),
        };
        if raw.len() > self.max_body {
            return upstream_unavailable();
        }
        let mut value: serde_json::Value = if raw.is_empty() {
            serde_json::Value::Object(serde_json::Map::new())
        } else {
            match serde_json::from_slice(&raw) {
                Ok(value @ serde_json::Value::Object(_)) => value,
                _ => return upstream_unavailable(),
            }
        };
        let is_chat = path_and_query.starts_with("/v1/chat/completions");
        let mut request_payload = serde_json::from_value::<ChatRequest>(value.clone()).ok();
        let authorization = header_value(&parts.headers, AUTHORIZATION);

        if is_chat {
            if let Some(parsed) = &request_payload
                && let Some(text) = parsed.last_user_text()
                && self.cancel.matches(text).is_some()
            {
                return self.send_cancel_completion(parsed);
            }
            if let (Some(router), Some(parsed)) = (&self.router, &request_payload)
                && let Some(utterance) = parsed.last_user_text().map(str::to_owned)
            {
                let chars = utterance.chars().count();
                let stream = parsed.is_stream();
                let decision = router.route(&utterance).await;
                tracing::info!(
                    event = "voice_route",
                    outcome = decision.outcome,
                    profile = crate::services::profile_or_unknown(decision.profile),
                    chars,
                    stream,
                    "voice route decided"
                );
                if decision.handled
                    && let Some(speech) = decision.speech
                {
                    return self.send_local_completion(parsed, &speech, DEFAULT_COMPLETION_ID);
                }
            }
            if let Some(parsed) = &request_payload {
                let prepared = parsed.with_reasoning_effort(&self.reasoning_effort);
                value = serde_json::to_value(&prepared).unwrap_or(value);
                request_payload = Some(prepared);
            }
            if request_payload.as_ref().is_some_and(ChatRequest::is_stream) {
                return self
                    .handle_stream(path_and_query, &value, authorization)
                    .await;
            }
        }

        let Ok(body) = serde_json::to_vec(&value) else {
            return upstream_unavailable();
        };
        let result = self
            .upstream
            .buffered(
                Method::POST,
                path_and_query,
                Some(body),
                authorization,
                header_str(&parts.headers, CONTENT_TYPE),
                header_str(&parts.headers, ACCEPT),
            )
            .await;
        let Ok(fetch) = result else {
            return upstream_unavailable();
        };
        let mut body = fetch.body;
        if is_chat
            && fetch.status == StatusCode::OK
            && let Ok(mut object) = serde_json::from_slice::<serde_json::Value>(&body)
            && object.is_object()
        {
            sanitize_chat_payload(&mut object);
            if let Ok(encoded) = serde_json::to_vec(&object) {
                body = encoded;
            }
        }
        send(fetch.status.as_u16(), body, &fetch.content_type)
    }

    fn send_cancel_completion(&self, payload: &ChatRequest) -> Response {
        let count = self.cancel_count.fetch_add(1, Ordering::Relaxed) + 1;
        // Structural evidence only: never the recognised text or payload.
        tracing::info!(
            event = "cancel_guarded",
            count,
            stream = payload.is_stream(),
            upstream = "skipped",
            "cancel phrase answered locally"
        );
        self.send_local_completion(payload, CANCEL_SILENCE, CANCEL_COMPLETION_ID)
    }

    fn send_local_completion(
        &self,
        payload: &ChatRequest,
        speech: &str,
        completion_id: &str,
    ) -> Response {
        let created = self.clock.now().unix_timestamp();
        let model = payload.model_name();
        if payload.is_stream() {
            let mut body = String::new();
            body.push_str(&sse::data_frame(
                &serde_json::to_string(&openai::local_chunk(
                    completion_id,
                    model,
                    created,
                    &serde_json::json!({"role": "assistant", "content": speech}),
                    None,
                ))
                .unwrap_or_default(),
            ));
            body.push_str(&sse::data_frame(
                &serde_json::to_string(&openai::local_chunk(
                    completion_id,
                    model,
                    created,
                    &serde_json::json!({}),
                    Some("stop"),
                ))
                .unwrap_or_default(),
            ));
            body.push_str(&sse::data_frame("[DONE]"));
            return sse_response(body.into_bytes());
        }
        let body = openai::local_completion(completion_id, model, created, speech);
        send(StatusCode::OK.as_u16(), body, "application/json")
    }

    async fn handle_stream(
        &self,
        path_and_query: &str,
        payload: &serde_json::Value,
        authorization: Option<HeaderValue>,
    ) -> Response {
        let Ok(body) = serde_json::to_vec(payload) else {
            return upstream_unavailable();
        };
        let Ok(response) = self
            .upstream
            .open_stream(path_and_query, body, authorization)
            .await
        else {
            return upstream_unavailable();
        };
        let status = response.status();
        if status.as_u16() >= 400 {
            let data = read_limited(response, self.max_body).await;
            let (body, content_type) = openai::normalize_error(
                status.as_u16(),
                data.unwrap_or_default(),
                "application/json",
            );
            return send(status.as_u16(), body, &content_type);
        }
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        if !content_type.starts_with("text/event-stream") {
            return upstream_unavailable();
        }

        let cancel = CancellationToken::new();
        let (sender, receiver) = mpsc::unbounded_channel();
        let stream = RelayBody {
            receiver,
            cancel: cancel.clone(),
        };
        let upstream = self.upstream.clone();
        tokio::spawn(async move {
            relay_stream(response, upstream, sender, cancel).await;
        });
        let mut response = Response::new(Body::from_stream(stream));
        *response.status_mut() = StatusCode::OK;
        let headers = response.headers_mut();
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("text/event-stream; charset=utf-8"),
        );
        headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
        headers.insert(CONNECTION, HeaderValue::from_static("close"));
        response
    }
}

/// Drains the whole relay into the client.
async fn relay_stream(
    mut response: reqwest::Response,
    upstream: Upstream,
    sender: mpsc::UnboundedSender<Bytes>,
    cancel: CancellationToken,
) {
    let started = Instant::now();
    let mut reader = sse::Reader::new();
    let mut state = StreamState::default();
    loop {
        match state.pump(&mut reader, started, &sender) {
            Ok(Pump::Done) => return,
            Err(()) => {
                let _ = send_frame(&sender, &error_frame());
                return;
            }
            Ok(Pump::More) => {}
        }
        let chunk = tokio::select! {
            () = cancel.cancelled() => return,
            result = tokio::time::timeout(upstream.timeout(), response.chunk()) => result,
        };
        match chunk {
            Err(_) | Ok(Err(_)) => {
                let _ = send_frame(&sender, &error_frame());
                return;
            }
            Ok(Ok(Some(bytes))) => {
                if reader.push(&bytes).is_err() {
                    let _ = send_frame(&sender, &error_frame());
                    return;
                }
            }
            Ok(Ok(None)) => {
                if reader.finish().is_err() {
                    let _ = send_frame(&sender, &error_frame());
                    return;
                }
                if let Ok(Pump::Done) = state.pump(&mut reader, started, &sender) {
                    return;
                } else {
                    let _ = send_frame(&sender, &error_frame());
                    return;
                }
            }
        }
    }
}

#[derive(Debug, Default)]
struct StreamState {
    sanitizers: BTreeMap<i64, StreamingSanitizer>,
    finished: BTreeSet<i64>,
    first_content: Option<f64>,
}

enum Pump {
    More,
    Done,
}

impl StreamState {
    fn pump(
        &mut self,
        reader: &mut sse::Reader,
        started: Instant,
        sender: &mpsc::UnboundedSender<Bytes>,
    ) -> Result<Pump, ()> {
        while let Some(event) = reader.next_event() {
            if event.name == sse::KEEPALIVE {
                if send_frame(sender, sse::KEEPALIVE_FRAME).is_err() {
                    return Ok(Pump::Done);
                }
                continue;
            }
            if event.data == "[DONE]" {
                if self.sanitizers.is_empty() || self.finished.len() != self.sanitizers.len() {
                    return Err(());
                }
                if send_frame(sender, &sse::data_frame("[DONE]")).is_err() {
                    return Ok(Pump::Done);
                }
                tracing::info!(
                    event = "stream_complete",
                    first_text = self.first_content.unwrap_or(0.0),
                    total = started.elapsed().as_secs_f64(),
                    "upstream stream relayed"
                );
                return Ok(Pump::Done);
            }
            if !event.name.is_empty() && event.name != "message" {
                // Hermes tool progress, approvals and status are not speech.
                continue;
            }
            let chunk: ChatChunk = match serde_json::from_str(&event.data) {
                Ok(chunk) => chunk,
                Err(_) => return Err(()),
            };
            if chunk.object.as_deref() != Some("chat.completion.chunk") {
                if chunk.extra.contains_key("error") {
                    return Err(());
                }
                continue;
            }
            let mut choices = Vec::new();
            let mut finish_choices = Vec::new();
            for choice in &chunk.choices {
                let index = choice.index();
                if self.finished.contains(&index) {
                    continue;
                }
                let sanitizer = self.sanitizers.entry(index).or_default();
                let has_finish = choice.finish_reason.is_some();
                let reason = choice
                    .finish_reason
                    .as_ref()
                    .and_then(serde_json::Value::as_str);
                if has_finish && !matches!(reason, Some("stop" | "length")) {
                    return Err(());
                }
                let delta = choice.delta.clone().unwrap_or_default();
                let content = delta.content.clone().unwrap_or_default();
                let mut text = sanitizer.feed(&content, has_finish).map_err(|_| ())?;
                if !text.is_empty() && !has_finish {
                    // Piper's sentence_stream waits for the next capitalized
                    // word; a paragraph boundary flushes the sentence now.
                    text.push_str("\n\n");
                }
                if !text.is_empty() && self.first_content.is_none() {
                    self.first_content = Some(started.elapsed().as_secs_f64());
                }
                let mut safe_delta = serde_json::Map::new();
                if !text.is_empty() {
                    safe_delta.insert("content".to_owned(), serde_json::Value::String(text));
                }
                if delta.role.as_deref() == Some("assistant") {
                    safe_delta.insert(
                        "role".to_owned(),
                        serde_json::Value::String("assistant".to_owned()),
                    );
                }
                if has_finish {
                    self.finished.insert(index);
                    finish_choices.push(serde_json::json!({
                        "index": index,
                        "delta": {},
                        "finish_reason": reason,
                    }));
                }
                if !safe_delta.is_empty() {
                    choices.push(serde_json::json!({
                        "index": index,
                        "delta": safe_delta,
                        "finish_reason": null,
                    }));
                }
            }
            if !choices.is_empty() {
                let frame = whitelisted(&chunk, choices);
                if send_value(sender, &frame).is_err() {
                    return Ok(Pump::Done);
                }
            }
            if !finish_choices.is_empty() {
                let frame = whitelisted(&chunk, finish_choices);
                if send_value(sender, &frame).is_err() {
                    return Ok(Pump::Done);
                }
            }
        }
        Ok(Pump::More)
    }
}

fn whitelisted(chunk: &ChatChunk, choices: Vec<serde_json::Value>) -> serde_json::Value {
    let mut object = serde_json::Map::new();
    if let Some(id) = &chunk.id {
        object.insert("id".to_owned(), id.clone());
    }
    if let Some(value) = &chunk.object {
        object.insert(
            "object".to_owned(),
            serde_json::Value::String(value.clone()),
        );
    }
    if let Some(created) = &chunk.created {
        object.insert("created".to_owned(), created.clone());
    }
    if let Some(model) = &chunk.model {
        object.insert("model".to_owned(), model.clone());
    }
    object.insert("choices".to_owned(), serde_json::Value::Array(choices));
    serde_json::Value::Object(object)
}

fn error_frame() -> String {
    sse::data_frame(&serde_json::to_string(&openai::error_frame()).unwrap_or_default())
}

fn send_value(sender: &mpsc::UnboundedSender<Bytes>, value: &serde_json::Value) -> Result<(), ()> {
    let encoded = serde_json::to_string(value).map_err(|_| ())?;
    send_frame(sender, &sse::data_frame(&encoded))
}

fn send_frame(sender: &mpsc::UnboundedSender<Bytes>, frame: &str) -> Result<(), ()> {
    sender.send(Bytes::from(frame.to_owned())).map_err(|_| ())
}

/// The response body backed by the relay task. Dropping it cancels the
/// upstream request, so a closed HA connection interrupts the Hermes turn.
struct RelayBody {
    receiver: mpsc::UnboundedReceiver<Bytes>,
    cancel: CancellationToken,
}

impl futures_core::Stream for RelayBody {
    type Item = Result<Bytes, std::convert::Infallible>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        self.receiver.poll_recv(cx).map(|item| item.map(Ok))
    }
}

impl Drop for RelayBody {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

async fn read_limited(mut response: reqwest::Response, max: usize) -> Option<Vec<u8>> {
    let mut data = Vec::new();
    while let Ok(Some(chunk)) = response.chunk().await {
        if data.len() + chunk.len() > max {
            return None;
        }
        data.extend_from_slice(&chunk);
    }
    Some(data)
}

fn header_str(headers: &HeaderMap, name: axum::http::HeaderName) -> Option<&str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

fn header_value(headers: &HeaderMap, name: axum::http::HeaderName) -> Option<HeaderValue> {
    headers.get(name).cloned()
}

fn upstream_unavailable() -> Response {
    send(
        StatusCode::SERVICE_UNAVAILABLE.as_u16(),
        openai::error_body(UPSTREAM_UNAVAILABLE),
        "application/json",
    )
}

/// Relays a buffered response with the shared error normalisation.
fn send(status: u16, body: Vec<u8>, content_type: &str) -> Response {
    let (body, content_type) = openai::normalize_error(status, body, content_type);
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY);
    let length = body.len();
    let mut response = Response::new(Body::from(body));
    *response.status_mut() = status;
    let headers = response.headers_mut();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_str(&content_type)
            .unwrap_or_else(|_| HeaderValue::from_static("application/json")),
    );
    if let Ok(value) = HeaderValue::from_str(&length.to_string()) {
        headers.insert(CONTENT_LENGTH, value);
    }
    headers.insert(CONNECTION, HeaderValue::from_static("close"));
    response
}

fn sse_response(body: Vec<u8>) -> Response {
    let mut response = Response::new(Body::from(body));
    *response.status_mut() = StatusCode::OK;
    let headers = response.headers_mut();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream; charset=utf-8"),
    );
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    headers.insert(CONNECTION, HeaderValue::from_static("close"));
    response
}
