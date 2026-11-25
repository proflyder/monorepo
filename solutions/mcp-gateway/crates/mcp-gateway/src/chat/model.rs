use serde::{Deserialize, Serialize};

pub use super::alerts::AlertCard;
pub use super::media::Attachment;

pub const MAX_TEXT_CHARS: usize = 4000;
pub const MAX_CLIENT_ID_CHARS: usize = 64;
pub const DEFAULT_PAGE: usize = 50;
pub const MAX_PAGE: usize = 200;
pub const MAX_ATTACHMENTS: usize = 4;
pub const MAX_TITLE_CHARS: usize = 80;
pub const AUTO_TITLE_CHARS: usize = 40;
pub const PREVIEW_CHARS: usize = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
}

impl Role {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::System => "system",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Self {
        match raw {
            "user" => Self::User,
            "assistant" => Self::Assistant,
            _ => Self::System,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Text,
    Alert,
    Voice,
    Image,
    File,
}

impl Kind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Alert => "alert",
            Self::Voice => "voice",
            Self::Image => "image",
            Self::File => "file",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Self {
        match raw {
            "alert" => Self::Alert,
            "voice" => Self::Voice,
            "image" => Self::Image,
            "file" => Self::File,
            _ => Self::Text,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pending,
    Streaming,
    Done,
    Failed,
}

impl Status {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Streaming => "streaming",
            Self::Done => "done",
            Self::Failed => "failed",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Self {
        match raw {
            "pending" => Self::Pending,
            "streaming" => Self::Streaming,
            "failed" => Self::Failed,
            _ => Self::Done,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKey {
    ChatTextEmpty,
    ChatTextTooLong,
    ChatClientIdInvalid,
    ChatReplyNotFound,
    ChatNotFound,
    ChatNotRetryable,
    ChatUpstreamUnavailable,
    ChatUpstreamFailed,
    ChatInterrupted,
    ChatStorageFailed,
    ChatAttachmentTooLarge,
    ChatAttachmentUnsupported,
    ChatAttachmentNotFound,
    ChatTooManyAttachments,
    ChatVoiceFormat,
    ChatSttFailed,
    ChatStopped,
    ChatSessionNotFound,
    ChatTitleInvalid,
    ChatNotStoppable,
}

impl ErrorKey {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ChatTextEmpty => "chat_text_empty",
            Self::ChatTextTooLong => "chat_text_too_long",
            Self::ChatClientIdInvalid => "chat_client_id_invalid",
            Self::ChatReplyNotFound => "chat_reply_not_found",
            Self::ChatNotFound => "chat_not_found",
            Self::ChatNotRetryable => "chat_not_retryable",
            Self::ChatUpstreamUnavailable => "chat_upstream_unavailable",
            Self::ChatUpstreamFailed => "chat_upstream_failed",
            Self::ChatInterrupted => "chat_interrupted",
            Self::ChatStorageFailed => "chat_storage_failed",
            Self::ChatAttachmentTooLarge => "chat_attachment_too_large",
            Self::ChatAttachmentUnsupported => "chat_attachment_unsupported",
            Self::ChatAttachmentNotFound => "chat_attachment_not_found",
            Self::ChatTooManyAttachments => "chat_too_many_attachments",
            Self::ChatVoiceFormat => "chat_voice_format",
            Self::ChatSttFailed => "chat_stt_failed",
            Self::ChatStopped => "chat_stopped",
            Self::ChatSessionNotFound => "chat_session_not_found",
            Self::ChatTitleInvalid => "chat_title_invalid",
            Self::ChatNotStoppable => "chat_not_stoppable",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        [
            Self::ChatTextEmpty,
            Self::ChatTextTooLong,
            Self::ChatClientIdInvalid,
            Self::ChatReplyNotFound,
            Self::ChatNotFound,
            Self::ChatNotRetryable,
            Self::ChatUpstreamUnavailable,
            Self::ChatUpstreamFailed,
            Self::ChatInterrupted,
            Self::ChatStorageFailed,
            Self::ChatAttachmentTooLarge,
            Self::ChatAttachmentUnsupported,
            Self::ChatAttachmentNotFound,
            Self::ChatTooManyAttachments,
            Self::ChatVoiceFormat,
            Self::ChatSttFailed,
            Self::ChatStopped,
            Self::ChatSessionNotFound,
            Self::ChatTitleInvalid,
            Self::ChatNotStoppable,
        ]
        .into_iter()
        .find(|key| key.as_str() == raw)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Quote {
    pub id: String,
    pub role: Role,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub session_id: String,
    pub seq: i64,
    pub version: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    pub role: Role,
    pub kind: Kind,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<Quote>,
    pub status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorKey>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<Attachment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alert: Option<AlertCard>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub messages: Vec<Message>,
    pub has_more: bool,
    pub cursor: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendRequest {
    pub client_id: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub reply_to: Option<String>,
    #[serde(default)]
    pub attachments: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendResponse {
    pub message: Message,
    pub reply: Message,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Delta {
    pub id: String,
    pub session_id: String,
    pub offset: usize,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorBody {
    pub code: ErrorKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub role: Role,
    pub kind: Kind,
    pub text: String,
    pub status: Status,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub pinned: bool,
    pub archived: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<Preview>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionList {
    pub sessions: Vec<Session>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSession {
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPatch {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub pinned: Option<bool>,
    #[serde(default)]
    pub archived: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub id: String,
    pub session_id: String,
    pub name: String,
    pub done: bool,
}
