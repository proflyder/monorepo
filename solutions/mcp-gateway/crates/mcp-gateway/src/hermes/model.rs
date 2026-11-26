use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::redact;

pub const TELEGRAM_SOURCE: &str = "telegram";
pub const VOICE_SOURCE: &str = "api_server";
pub const VOICE_SESSION_PREFIX: &str = "api-";
pub const REPORT_SKILL_MARKER: &str = "[IMPORTANT: The user has invoked the \"report\" skill";
pub const REPORT_COMMAND: &str = "/report";
pub const REPORT_TOOL: &str = "infra_report";
pub const APP_SESSION_PREFIX: &str = "app_";
pub const APP_LEGACY_SESSION: &str = "home-app";
pub const REPORT_SESSIONS: usize = 10;
pub const VOICE_SESSIONS: usize = 10;
pub const MAX_REPORTS: usize = 20;
pub const MAX_VOICE_REQUESTS: usize = 20;
pub const AGENT_LIMIT: usize = 60;
pub const RECENT_AGENT_HOURS: u32 = 24;
pub const FAILED_FINISH: &str = "error";
pub const HIDDEN_KIND: &str = "hidden";

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SessionRow {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub last_active: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SessionList {
    #[serde(default)]
    pub data: Vec<SessionRow>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct MessageRow {
    #[serde(default)]
    pub id: Value,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub content: Value,
    #[serde(default)]
    pub timestamp: Option<f64>,
    #[serde(default)]
    pub finish_reason: Option<String>,
    #[serde(default)]
    pub display_kind: Option<String>,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub tool_calls: Value,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct MessagePage {
    #[serde(default)]
    pub data: Vec<MessageRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Agents {
    pub available: bool,
    pub agents: Vec<Agent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    pub id: String,
    pub title: String,
    pub project: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reports {
    pub available: bool,
    pub reports: Vec<Report>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub id: String,
    pub channel: Channel,
    pub requested_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
    pub text: String,
    pub failed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Telegram,
    App,
    Voice,
}

impl Channel {
    #[must_use]
    pub fn of_session(source: &str, session: &str) -> Self {
        match source {
            TELEGRAM_SOURCE => Self::Telegram,
            _ if session.starts_with(VOICE_SESSION_PREFIX) => Self::Voice,
            _ => Self::App,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SessionPage {
    pub channel: Channel,
    pub rows: Vec<MessageRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Voice {
    pub relay_up: bool,
    pub hermes_up: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub hermes_version: String,
    pub agent_mode: bool,
    pub requests_available: bool,
    pub requests: Vec<VoiceRequest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceRequest {
    pub id: String,
    pub at: i64,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<i64>,
    pub failed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Exchange {
    pub id: String,
    pub asked_at: f64,
    pub request: String,
    pub replied_at: Option<f64>,
    pub reply: Option<String>,
    pub failed: bool,
    pub called_report_tool: bool,
}

impl Exchange {
    fn opened_by(message: &MessageRow, text: String) -> Self {
        Self {
            id: message_id(&message.id),
            asked_at: message.timestamp.unwrap_or_default(),
            request: text,
            replied_at: None,
            reply: None,
            failed: false,
            called_report_tool: false,
        }
    }

    fn note_tools(&mut self, message: &MessageRow) {
        self.called_report_tool |= names_report_tool(message);
    }

    fn answer(&mut self, message: &MessageRow, text: String) {
        self.failed = message.finish_reason.as_deref() == Some(FAILED_FINISH);
        if !text.is_empty() {
            self.replied_at = message.timestamp;
            self.reply = Some(text);
        }
    }

    fn is_report(&self) -> bool {
        let request = self.request.trim_start();
        self.called_report_tool
            || request.starts_with(REPORT_SKILL_MARKER)
            || request.starts_with(REPORT_COMMAND)
    }

    fn report(self, channel: Channel) -> Report {
        Report {
            id: self.id,
            channel,
            requested_at: whole_seconds(self.asked_at),
            completed_at: self.replied_at.map(whole_seconds),
            text: self.reply.as_deref().map(redact::text).unwrap_or_default(),
            failed: self.failed,
        }
    }

    fn voice_request(self) -> VoiceRequest {
        let latency_ms = self
            .replied_at
            .map(|replied| ((replied - self.asked_at) * 1000.0).round() as i64)
            .filter(|latency| *latency >= 0);
        VoiceRequest {
            id: self.id,
            at: whole_seconds(self.asked_at),
            text: redact::text(&self.request),
            reply: self.reply.as_deref().map(redact::text),
            latency_ms,
            failed: self.failed || self.reply.is_none(),
        }
    }
}

#[must_use]
pub fn exchanges(mut messages: Vec<MessageRow>) -> Vec<Exchange> {
    messages.sort_by(|left, right| {
        left.timestamp
            .unwrap_or_default()
            .total_cmp(&right.timestamp.unwrap_or_default())
    });
    let mut found: Vec<Exchange> = Vec::new();
    for message in messages.iter().filter(|message| is_visible(message)) {
        let text = content_text(&message.content);
        match (message.role.as_str(), found.last_mut()) {
            ("user", _) => found.push(Exchange::opened_by(message, text)),
            ("assistant", Some(open)) => {
                open.note_tools(message);
                open.answer(message, text);
            }
            ("tool", Some(open)) => open.note_tools(message),
            _ => {}
        }
    }
    found
}

#[must_use]
pub fn reports(pages: Vec<SessionPage>) -> Vec<Report> {
    let mut found: Vec<Report> = pages.into_iter().flat_map(page_reports).collect();
    found.sort_by_key(|report| std::cmp::Reverse(report.requested_at));
    found.truncate(MAX_REPORTS);
    found
}

fn page_reports(page: SessionPage) -> Vec<Report> {
    exchanges(page.rows)
        .into_iter()
        .filter(Exchange::is_report)
        .map(|exchange| exchange.report(page.channel))
        .collect()
}

fn names_report_tool(message: &MessageRow) -> bool {
    let by_result = message
        .tool_name
        .as_deref()
        .is_some_and(|name| name.contains(REPORT_TOOL));
    by_result || called_tool_names(&message.tool_calls).any(|name| name.contains(REPORT_TOOL))
}

fn called_tool_names(calls: &Value) -> impl Iterator<Item = &str> {
    calls
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|call| call.pointer("/function/name").and_then(Value::as_str))
}

#[must_use]
pub fn voice_requests(pages: Vec<Vec<MessageRow>>) -> Vec<VoiceRequest> {
    let mut found: Vec<VoiceRequest> = pages
        .into_iter()
        .flat_map(exchanges)
        .map(Exchange::voice_request)
        .collect();
    found.sort_by_key(|request| std::cmp::Reverse(request.at));
    found.truncate(MAX_VOICE_REQUESTS);
    found
}

#[must_use]
pub fn recent_sessions(list: SessionList, limit: usize, prefix: &str) -> Vec<String> {
    let mut rows: Vec<SessionRow> = list
        .data
        .into_iter()
        .filter(|row| row.id.starts_with(prefix))
        .collect();
    rows.sort_by(|left, right| {
        right
            .last_active
            .unwrap_or_default()
            .total_cmp(&left.last_active.unwrap_or_default())
    });
    rows.into_iter().take(limit).map(|row| row.id).collect()
}

#[must_use]
pub fn content_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.trim().to_owned(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_owned(),
        _ => String::new(),
    }
}

fn is_visible(message: &MessageRow) -> bool {
    message.display_kind.as_deref() != Some(HIDDEN_KIND)
}

fn message_id(id: &Value) -> String {
    match id {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn whole_seconds(unix: f64) -> i64 {
    unix.floor() as i64
}
