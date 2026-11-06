//! Spoken phrases and the shaping rules for late answers.

use super::sanitize::{announce_text, trim_space};

/// Spoken when the agent server cannot be reached.
pub const SPEECH_AGENTS_DOWN: &str = "Сервер агентов недоступен.";
/// Spoken when a routed turn failed explicitly.
pub const SPEECH_TURN_FAILED: &str = "Агент завершил работу с ошибкой.";
/// Spoken for an empty successful answer.
pub const SPEECH_DONE: &str = "Готово.";
/// Prefix of an answer that waited more than two minutes.
pub const STALE_PREFIX: &str = "Пока вас не было: ";

/// Maps one result-poll outcome onto the text to speak, if any.
///
/// Deliberate: a timeout, a gateway error or a still-running turn speaks
/// nothing at all, because inventing speech would be worse than silence; an
/// empty successful answer becomes [`SPEECH_DONE`] and an explicitly failed
/// turn becomes [`SPEECH_TURN_FAILED`].
#[must_use]
pub fn answer_text(status: &str, speech: &str, max_chars: usize) -> Option<String> {
    let silent = status.is_empty()
        || status == "running"
        || status == "gateway_error"
        || status == "timeout";
    let mut speech = if silent {
        String::new()
    } else {
        speech.to_owned()
    };
    if trim_space(&speech).is_empty() && !silent {
        speech = if status == "failed" {
            SPEECH_TURN_FAILED.to_owned()
        } else {
            SPEECH_DONE.to_owned()
        };
    }
    let text = announce_text(&speech, max_chars);
    (!text.is_empty()).then_some(text)
}

/// Adds the "Пока вас не было: " prefix to an answer that waited too long.
#[must_use]
pub fn with_stale_prefix(speech: &str, age_seconds: i64, stale_after_seconds: i64) -> String {
    if age_seconds > stale_after_seconds {
        format!("{STALE_PREFIX}{speech}")
    } else {
        speech.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::{SPEECH_DONE, SPEECH_TURN_FAILED, answer_text, with_stale_prefix};

    #[test]
    fn silent_outcomes_speak_nothing() {
        assert_eq!(answer_text("timeout", "нет", 200), None);
        assert_eq!(answer_text("running", "нет", 200), None);
        assert_eq!(answer_text("gateway_error", "нет", 200), None);
        assert_eq!(answer_text("", "нет", 200), None);
    }

    #[test]
    fn empty_success_and_failure_get_their_phrases() {
        assert_eq!(answer_text("done", "", 200).as_deref(), Some(SPEECH_DONE));
        assert_eq!(
            answer_text("failed", "", 200).as_deref(),
            Some(SPEECH_TURN_FAILED)
        );
        assert_eq!(
            answer_text("done", "**Готово**.", 200).as_deref(),
            Some("Готово.")
        );
    }

    #[test]
    fn stale_answers_are_prefixed() {
        assert_eq!(
            with_stale_prefix("Готово.", 121, 120),
            "Пока вас не было: Готово."
        );
        assert_eq!(with_stale_prefix("Готово.", 120, 120), "Готово.");
    }
}
