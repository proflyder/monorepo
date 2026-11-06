//! Typed OpenAI chat/completions shapes plus the error normalisation HA's
//! `llama_cpp` adapter requires.
//!
//! Unknown fields travel through `#[serde(flatten)]`, so a request or response
//! the proxy does not model is forwarded unchanged instead of being stripped.
//!
//! The error shape is not cosmetic: the adapter calls `.get` on the error
//! value, so it is **always** a JSON object.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::domain::sanitize::trim_space;

/// The default model name used when the client did not send one.
pub const DEFAULT_MODEL: &str = "hermes-agent";

/// One chat message. `content` stays a [`Value`] because a tool result or a
/// multi-part content array must be forwarded unchanged.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Message {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Value>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A `POST /v1/chat/completions` request.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ChatRequest {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<Message>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_options: Option<Map<String, Value>>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl ChatRequest {
    /// Parses a JSON object; a non-object or invalid body is `None`.
    #[must_use]
    pub fn parse(raw: &[u8]) -> Option<Self> {
        if raw.is_empty() {
            return Some(Self::default());
        }
        serde_json::from_slice(raw).ok()
    }

    /// The text of the final message when it is a plain user turn: no tool
    /// result, no assistant text, no multi-part content.
    #[must_use]
    pub fn last_user_text(&self) -> Option<&str> {
        let last = self.messages.last()?;
        if last.role.as_deref() != Some("user") {
            return None;
        }
        let content = last.content.as_ref()?.as_str()?;
        (!trim_space(content).is_empty()).then_some(content)
    }

    /// Injects `model_options.reasoning_effort` without mutating the caller's
    /// payload.
    #[must_use]
    pub fn with_reasoning_effort(&self, effort: &str) -> Self {
        let mut out = self.clone();
        let options = out.model_options.get_or_insert_with(Map::new);
        options.insert(
            "reasoning_effort".to_owned(),
            Value::String(effort.to_owned()),
        );
        out
    }

    /// The model name, or [`DEFAULT_MODEL`].
    #[must_use]
    pub fn model_name(&self) -> &str {
        self.model
            .as_deref()
            .filter(|m| !m.is_empty())
            .unwrap_or(DEFAULT_MODEL)
    }

    /// `payload.stream is true`: only the JSON literal `true` selects the
    /// streaming reply shape.
    #[must_use]
    pub fn is_stream(&self) -> bool {
        self.stream == Some(true)
    }
}

/// The delta of one streamed choice.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Delta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One streamed choice. `finish_reason` is `Some` only for a non-null value.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ChunkChoice {
    #[serde(default)]
    pub index: Option<Value>,
    #[serde(default)]
    pub delta: Option<Delta>,
    #[serde(default)]
    pub finish_reason: Option<Value>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One decoded `chat.completion.chunk`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ChatChunk {
    #[serde(default)]
    pub id: Option<Value>,
    #[serde(default)]
    pub object: Option<String>,
    #[serde(default)]
    pub created: Option<Value>,
    #[serde(default)]
    pub model: Option<Value>,
    #[serde(default)]
    pub choices: Vec<ChunkChoice>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl ChunkChoice {
    /// The choice index, defaulting to `0` when absent (like Go's
    /// `choice.get("index", 0)`).
    #[must_use]
    pub fn index(&self) -> i64 {
        value_to_i64(self.index.as_ref()).unwrap_or(0)
    }
}

/// Reads a JSON number as an integer, matching the Go helper.
#[must_use]
pub fn value_to_i64(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    if let Some(number) = value.as_i64() {
        return Some(number);
    }
    if let Some(number) = value.as_u64() {
        return i64::try_from(number).ok();
    }
    value.as_f64().map(|number| number as i64)
}

/// A buffered `chat.completion` response. `choices[].message.content` is the
/// only field the proxy may rewrite.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ChatResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<ResponseChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Value>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One buffered choice.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ResponseChoice {
    #[serde(default)]
    pub index: Option<Value>,
    #[serde(default)]
    pub message: Option<Message>,
    #[serde(default)]
    pub finish_reason: Option<Value>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The OpenAI-shaped error body. It is always an object.
#[must_use]
pub fn error_body(message: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "error": {
            "message": message,
            "type": "server_error",
            "code": "upstream_error",
        }
    }))
    .unwrap_or_else(|_| {
        br#"{"error":{"message":"upstream error","type":"server_error","code":"upstream_error"}}"#
            .to_vec()
    })
}

/// Builds the local `chat.completion` object for a proxy-generated reply.
#[must_use]
pub fn local_completion(completion_id: &str, model: &str, created: i64, speech: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "id": completion_id,
        "object": "chat.completion",
        "created": created,
        "model": model,
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": speech},
            "finish_reason": "stop",
        }],
        "usage": {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0},
    }))
    .unwrap_or_default()
}

/// Builds one local `chat.completion.chunk`.
#[must_use]
pub fn local_chunk(
    completion_id: &str,
    model: &str,
    created: i64,
    delta: &Value,
    finish_reason: Option<&str>,
) -> Value {
    json!({
        "id": completion_id,
        "object": "chat.completion.chunk",
        "created": created,
        "model": model,
        "choices": [{
            "index": 0,
            "delta": delta,
            "finish_reason": finish_reason,
        }],
    })
}

/// The single SSE error record emitted after a mid-stream failure.
#[must_use]
pub fn error_frame() -> Value {
    json!({
        "error": {
            "message": "Voice upstream stream failed",
            "type": "server_error",
        }
    })
}

/// Maps an upstream error response onto something HA can parse. Successful
/// responses pass through untouched.
#[must_use]
pub fn normalize_error(status: u16, body: Vec<u8>, content_type: &str) -> (Vec<u8>, String) {
    if status < 400 {
        return (body, content_type.to_owned());
    }
    if let Ok(Value::Object(object)) = serde_json::from_slice::<Value>(&body) {
        match object.get("error") {
            Some(Value::Object(_)) => return (body, "application/json".to_owned()),
            Some(Value::String(message)) => {
                return (error_body(message), "application/json".to_owned());
            }
            _ => {}
        }
    }
    (
        error_body(&format!("Hermes upstream returned HTTP {status}")),
        "application/json".to_owned(),
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        ChatChunk, ChatRequest, error_body, local_completion, normalize_error, value_to_i64,
    };

    #[test]
    fn request_preserves_unknown_fields_and_injects_reasoning() {
        let raw = r#"{
            "messages": [{"role": "user", "content": "привет", "name": "owner"}],
            "model": "hermes-agent",
            "stream": true,
            "tools": [{"type": "function"}],
            "model_options": {"existing": 1}
        }"#;
        let request = ChatRequest::parse(raw.as_bytes()).expect("parse");
        assert_eq!(request.last_user_text(), Some("привет"));
        assert!(request.is_stream());
        let prepared = request.with_reasoning_effort("low");
        assert_eq!(
            prepared.model_options.as_ref().expect("options")["reasoning_effort"],
            "low"
        );
        let encoded = serde_json::to_value(&prepared).expect("encode");
        assert_eq!(encoded["tools"], json!([{"type": "function"}]));
        assert_eq!(encoded["messages"][0]["name"], "owner");
        assert_eq!(prepared.model_name(), "hermes-agent");
    }

    #[test]
    fn non_plain_user_turns_are_not_routed() {
        let tool = ChatRequest::parse(br#"{"messages":[{"role":"tool","content":"result"}]}"#)
            .expect("parse");
        assert_eq!(tool.last_user_text(), None);
        let multipart = ChatRequest::parse(
            br#"{"messages":[{"role":"user","content":[{"type":"text","text":"x"}]}]}"#,
        )
        .expect("parse");
        assert_eq!(multipart.last_user_text(), None);
        let blank =
            ChatRequest::parse(br#"{"messages":[{"role":"user","content":"  "}]}"#).expect("parse");
        assert_eq!(blank.last_user_text(), None);
    }

    #[test]
    fn chunk_keeps_only_modelled_fields_and_ignores_unknown_deltas() {
        let raw = r#"{
            "id": "chatcmpl-1",
            "object": "chat.completion.chunk",
            "created": 1000,
            "model": "hermes-agent",
            "choices": [{
                "index": 0,
                "delta": {"content": "Привет.", "role": "assistant", "reasoning_content": "x"},
                "finish_reason": null,
                "logprobs": null
            }]
        }"#;
        let chunk: ChatChunk = serde_json::from_slice(raw.as_bytes()).expect("parse");
        assert_eq!(chunk.choices.len(), 1);
        let delta = chunk.choices[0].delta.as_ref().expect("delta");
        assert_eq!(delta.content.as_deref(), Some("Привет."));
        assert_eq!(delta.extra.get("reasoning_content"), Some(&json!("x")));
        assert!(chunk.choices[0].finish_reason.is_none());
        assert_eq!(chunk.choices[0].index(), 0);
    }

    #[test]
    fn error_normalisation_always_yields_an_object() {
        let (body, content_type) =
            normalize_error(502, b"{\"error\":\"boom\"}".to_vec(), "text/plain");
        let parsed: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(parsed["error"]["message"], "boom");
        assert_eq!(content_type, "application/json");

        let (body, _) = normalize_error(500, b"not json".to_vec(), "text/plain");
        let parsed: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(
            parsed["error"]["message"],
            "Hermes upstream returned HTTP 500"
        );

        let object = br#"{"error":{"message":"kept","type":"x"}}"#;
        let (body, _) = normalize_error(429, object.to_vec(), "application/json");
        assert_eq!(body, object);
    }

    #[test]
    fn local_completion_has_the_contract_shape() {
        let body = local_completion("chatcmpl-test", "hermes-agent", 1, " ");
        let parsed: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(parsed["object"], "chat.completion");
        assert_eq!(parsed["choices"][0]["message"]["content"], " ");
        assert_eq!(parsed["usage"]["total_tokens"], 0);
    }

    #[test]
    fn value_to_i64_accepts_every_json_number_shape() {
        assert_eq!(value_to_i64(Some(&json!(42))), Some(42));
        assert_eq!(value_to_i64(Some(&json!(42.0))), Some(42));
        assert_eq!(value_to_i64(Some(&json!("42"))), None);
        assert_eq!(value_to_i64(None), None);
    }

    #[test]
    fn fallback_error_body_is_still_an_object() {
        let parsed: serde_json::Value = serde_json::from_slice(&error_body("x")).expect("json");
        assert!(parsed["error"].is_object());
    }
}
