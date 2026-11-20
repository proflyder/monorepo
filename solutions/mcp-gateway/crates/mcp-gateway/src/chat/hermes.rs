use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::model::ErrorKey;
use crate::tools::BoxFuture;

pub const SESSION_TITLE: &str = "Home app";
pub const READ_TIMEOUT: Duration = Duration::from_secs(150);
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnEvent {
    Delta(String),
    Completed(String),
    Failed(ErrorKey),
}

pub trait HermesApi: Send + Sync {
    fn run<'a>(&'a self, message: String, sink: mpsc::Sender<TurnEvent>) -> BoxFuture<'a, ()>;
}

pub struct Client {
    http: reqwest::Client,
    base_url: String,
    token: String,
    session: String,
    prompt: String,
    session_ready: AtomicBool,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("chat::hermes::Client")
            .field("base_url", &self.base_url)
            .field("session", &self.session)
            .finish_non_exhaustive()
    }
}

impl Client {
    pub fn new(
        base_url: &str,
        token: String,
        session: &str,
        prompt: String,
    ) -> Result<Self, String> {
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
            session: session.to_owned(),
            prompt,
            session_ready: AtomicBool::new(false),
        })
    }

    async fn ensure_session(&self) -> Result<(), ErrorKey> {
        if self.session_ready.load(Ordering::Acquire) {
            return Ok(());
        }
        let response = self
            .http
            .post(format!("{}/api/sessions", self.base_url))
            .bearer_auth(&self.token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(json!({"id": self.session, "title": SESSION_TITLE}).to_string())
            .send()
            .await
            .map_err(|_| ErrorKey::ChatUpstreamUnavailable)?;
        let status = response.status().as_u16();
        match status {
            200..=299 | 409 => {
                self.session_ready.store(true, Ordering::Release);
                Ok(())
            }
            _ => Err(ErrorKey::ChatUpstreamUnavailable),
        }
    }

    async fn stream(&self, message: String, sink: &mpsc::Sender<TurnEvent>) -> TurnEvent {
        if let Err(key) = self.ensure_session().await {
            return TurnEvent::Failed(key);
        }
        let body = json!({"message": message, "system_message": self.prompt});
        let sent = self
            .http
            .post(format!(
                "{}/api/sessions/{}/chat/stream",
                self.base_url, self.session
            ))
            .bearer_auth(&self.token)
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.to_string())
            .send()
            .await;
        let mut response = match sent {
            Ok(response) if response.status().is_success() => response,
            Ok(response) if response.status().as_u16() == 404 => {
                self.session_ready.store(false, Ordering::Release);
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
        "error" | "run.failed" | "run.cancelled" => {
            Some(TurnEvent::Failed(ErrorKey::ChatUpstreamFailed))
        }
        _ => None,
    }
}

impl HermesApi for Client {
    fn run<'a>(&'a self, message: String, sink: mpsc::Sender<TurnEvent>) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let terminal = self.stream(message, &sink).await;
            let _sent = sink.send(terminal).await;
        })
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
