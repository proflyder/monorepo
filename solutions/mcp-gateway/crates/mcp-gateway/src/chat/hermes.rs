use std::collections::HashSet;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::model::ErrorKey;
use crate::tools::BoxFuture;

pub const SESSION_TITLE: &str = "Home app";
pub const THINKING_TOOL: &str = "_thinking";
pub const READ_TIMEOUT: Duration = Duration::from_secs(150);
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnEvent {
    Delta(String),
    Tool { name: String, done: bool },
    Completed(String),
    Failed(ErrorKey),
}

pub struct Turn<'a> {
    pub session: &'a str,
    pub title: Option<&'a str>,
    pub message: Value,
}

pub trait HermesApi: Send + Sync {
    fn run<'a>(&'a self, turn: Turn<'a>, sink: mpsc::Sender<TurnEvent>) -> BoxFuture<'a, ()>;

    fn title<'a>(&'a self, session: &'a str) -> BoxFuture<'a, Option<String>>;

    fn patch<'a>(&'a self, session: &'a str, fields: Value) -> BoxFuture<'a, Result<(), ErrorKey>>;

    fn delete<'a>(&'a self, session: &'a str) -> BoxFuture<'a, Result<(), ErrorKey>>;
}

pub struct Client {
    http: reqwest::Client,
    base_url: String,
    token: String,
    prompt: String,
    ready: Mutex<HashSet<String>>,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("chat::hermes::Client")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

impl Client {
    pub fn new(base_url: &str, token: String, prompt: String) -> Result<Self, String> {
        let _already_installed = rustls::crypto::ring::default_provider().install_default();
        let http = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .read_timeout(READ_TIMEOUT)
            .build()
            .map_err(|error| format!("chat hermes client: {error}"))?;
        Ok(Self {
            http,
            base_url: base_url.trim().trim_end_matches('/').to_owned(),
            token,
            prompt,
            ready: Mutex::new(HashSet::new()),
        })
    }

    fn session_url(&self, session: &str) -> String {
        format!("{}/api/sessions/{session}", self.base_url)
    }

    fn mark(&self, session: &str, ready: bool) {
        let mut known = self.ready.lock().unwrap_or_else(PoisonError::into_inner);
        if ready {
            known.insert(session.to_owned());
        } else {
            known.remove(session);
        }
    }

    fn is_ready(&self, session: &str) -> bool {
        self.ready
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(session)
    }

    async fn ensure_session(&self, session: &str, title: Option<&str>) -> Result<(), ErrorKey> {
        if self.is_ready(session) {
            return Ok(());
        }
        let mut body = json!({"id": session});
        if let Some(title) = title {
            body["title"] = Value::String(title.to_owned());
        }
        let response = self
            .http
            .post(format!("{}/api/sessions", self.base_url))
            .bearer_auth(&self.token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.to_string())
            .send()
            .await
            .map_err(|_| ErrorKey::ChatUpstreamUnavailable)?;
        match response.status().as_u16() {
            200..=299 | 409 => {
                self.mark(session, true);
                Ok(())
            }
            _ => Err(ErrorKey::ChatUpstreamUnavailable),
        }
    }

    async fn stream(&self, turn: Turn<'_>, sink: &mpsc::Sender<TurnEvent>) -> TurnEvent {
        if let Err(key) = self.ensure_session(turn.session, turn.title).await {
            return TurnEvent::Failed(key);
        }
        let body = json!({"message": turn.message, "system_message": self.prompt});
        let sent = self
            .http
            .post(format!("{}/chat/stream", self.session_url(turn.session)))
            .bearer_auth(&self.token)
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.to_string())
            .send()
            .await;
        let mut response = match sent {
            Ok(response) if response.status().is_success() => response,
            Ok(response) if response.status().as_u16() == 404 => {
                self.mark(turn.session, false);
                return TurnEvent::Failed(ErrorKey::ChatUpstreamUnavailable);
            }
            Ok(_) | Err(_) => return TurnEvent::Failed(ErrorKey::ChatUpstreamUnavailable),
        };
        let mut parser = SseParser::default();
        let mut collected = String::new();
        loop {
            let chunk = match response.chunk().await {
                Ok(Some(chunk)) => chunk,
                Ok(None) => break,
                Err(_) => return TurnEvent::Failed(ErrorKey::ChatUpstreamFailed),
            };
            for (name, data) in parser.push(&String::from_utf8_lossy(&chunk)) {
                if let Some(terminal) = apply(&name, &data, &mut collected, sink).await {
                    return terminal;
                }
            }
        }
        TurnEvent::Failed(ErrorKey::ChatUpstreamFailed)
    }
}

async fn apply(
    name: &str,
    data: &str,
    collected: &mut String,
    sink: &mpsc::Sender<TurnEvent>,
) -> Option<TurnEvent> {
    let payload: Value = serde_json::from_str(data).unwrap_or(Value::Null);
    match name {
        "assistant.delta" => {
            let delta = payload["delta"].as_str().unwrap_or_default();
            if !delta.is_empty() {
                collected.push_str(delta);
                let _sent = sink.send(TurnEvent::Delta(delta.to_owned())).await;
            }
            None
        }
        "assistant.completed" => {
            let content = payload["content"].as_str().unwrap_or_default();
            let text = if content.trim().is_empty() {
                collected.clone()
            } else {
                content.to_owned()
            };
            Some(TurnEvent::Completed(text))
        }
        "tool.started" | "tool.completed" | "tool.failed" => {
            let tool = payload["tool_name"].as_str().unwrap_or_default();
            if !tool.is_empty() && tool != THINKING_TOOL {
                let event = TurnEvent::Tool {
                    name: tool.to_owned(),
                    done: name != "tool.started",
                };
                let _sent = sink.send(event).await;
            }
            None
        }
        "error" | "run.failed" | "run.cancelled" => {
            Some(TurnEvent::Failed(ErrorKey::ChatUpstreamFailed))
        }
        _ => None,
    }
}

impl Client {
    async fn read_title(&self, session: &str) -> Option<String> {
        let response = self
            .http
            .get(self.session_url(session))
            .bearer_auth(&self.token)
            .send()
            .await
            .ok()
            .filter(|response| response.status().is_success())?;
        let body: Value = serde_json::from_str(&response.text().await.ok()?).ok()?;
        let session = body.get("session").unwrap_or(&body);
        session["title"]
            .as_str()
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_owned)
    }

    async fn send_patch(&self, session: &str, fields: Value) -> Result<(), ErrorKey> {
        let response = self
            .http
            .patch(self.session_url(session))
            .bearer_auth(&self.token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(fields.to_string())
            .send()
            .await
            .map_err(|_| ErrorKey::ChatUpstreamUnavailable)?;
        upstream_result(response.status())
    }

    async fn send_delete(&self, session: &str) -> Result<(), ErrorKey> {
        self.mark(session, false);
        let response = self
            .http
            .delete(self.session_url(session))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|_| ErrorKey::ChatUpstreamUnavailable)?;
        upstream_result(response.status())
    }
}

fn upstream_result(status: reqwest::StatusCode) -> Result<(), ErrorKey> {
    (status.is_success() || status == reqwest::StatusCode::NOT_FOUND)
        .then_some(())
        .ok_or(ErrorKey::ChatUpstreamFailed)
}

impl HermesApi for Client {
    fn run<'a>(&'a self, turn: Turn<'a>, sink: mpsc::Sender<TurnEvent>) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let terminal = self.stream(turn, &sink).await;
            let _sent = sink.send(terminal).await;
        })
    }

    fn title<'a>(&'a self, session: &'a str) -> BoxFuture<'a, Option<String>> {
        Box::pin(self.read_title(session))
    }

    fn patch<'a>(&'a self, session: &'a str, fields: Value) -> BoxFuture<'a, Result<(), ErrorKey>> {
        Box::pin(self.send_patch(session, fields))
    }

    fn delete<'a>(&'a self, session: &'a str) -> BoxFuture<'a, Result<(), ErrorKey>> {
        Box::pin(self.send_delete(session))
    }
}

#[derive(Debug, Default)]
pub struct SseParser {
    buffer: String,
}

impl SseParser {
    pub fn push(&mut self, chunk: &str) -> Vec<(String, String)> {
        self.buffer.push_str(&chunk.replace("\r\n", "\n"));
        let mut events = Vec::new();
        while let Some(end) = self.buffer.find("\n\n") {
            let block: String = self.buffer.drain(..end + 2).collect();
            if let Some(event) = parse_block(&block) {
                events.push(event);
            }
        }
        events
    }
}

fn parse_block(block: &str) -> Option<(String, String)> {
    let mut name = String::from("message");
    let mut data = Vec::new();
    for line in block.lines() {
        if let Some(value) = line.strip_prefix("event:") {
            value.trim().clone_into(&mut name);
        }
        if let Some(value) = line.strip_prefix("data:") {
            data.push(value.strip_prefix(' ').unwrap_or(value));
        }
    }
    (!data.is_empty()).then(|| (name, data.join("\n")))
}
