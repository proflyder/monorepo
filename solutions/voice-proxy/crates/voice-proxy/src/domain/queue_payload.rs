use serde::{Deserialize, Serialize};

pub const TASK_TYPE: &str = "voice.announce";
pub const QUEUE_PREFIX: &str = "voice-";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnnouncePayload {
    pub text: String,
    pub profile: String,
    pub thread_id: String,
    pub seq: String,
    pub created_unix: i64,
}

impl AnnouncePayload {
    #[must_use]
    pub fn new(
        text: impl Into<String>,
        profile: impl Into<String>,
        thread_id: impl Into<String>,
        seq: impl Into<String>,
        created_unix: i64,
    ) -> Self {
        Self {
            text: text.into(),
            profile: profile.into(),
            thread_id: thread_id.into(),
            seq: seq.into(),
            created_unix,
        }
    }

    pub fn encode(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    #[must_use]
    pub fn decode(raw: &str) -> Option<Self> {
        if raw.is_empty() {
            return None;
        }
        serde_json::from_str(raw).ok()
    }
}

#[must_use]
pub fn queue_name(profile: &str) -> String {
    format!("{QUEUE_PREFIX}{profile}")
}

#[must_use]
pub fn idempotency_key(thread_id: &str, seq: &str) -> String {
    format!("voice:{thread_id}:{seq}")
}

#[cfg(test)]
mod tests {
    use super::{AnnouncePayload, TASK_TYPE, idempotency_key, queue_name};

    const GOLDEN: &str = r#"{"text":"Готово.","profile":"work","thread_id":"thr_example","seq":"done:42","created_unix":1700000000}"#;

    #[test]
    fn payload_bytes_match_the_go_golden_fixture() {
        let payload =
            AnnouncePayload::new("Готово.", "work", "thr_example", "done:42", 1_700_000_000);
        assert_eq!(payload.encode().expect("encode"), GOLDEN);
        assert_eq!(AnnouncePayload::decode(GOLDEN), Some(payload));
    }

    #[test]
    fn task_wire_constants_are_stable() {
        assert_eq!(TASK_TYPE, "voice.announce");
        assert_eq!(queue_name("work"), "voice-work");
        assert_eq!(queue_name("personal"), "voice-personal");
        assert_eq!(
            idempotency_key("thr_example", "done:42"),
            "voice:thr_example:done:42"
        );
    }

    #[test]
    fn invalid_payload_decodes_to_none() {
        assert_eq!(AnnouncePayload::decode(""), None);
        assert_eq!(AnnouncePayload::decode("[]"), None);
        assert_eq!(AnnouncePayload::decode("{\"text\":1}"), None);
    }
}
