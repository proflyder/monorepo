//! Exact cancel-phrase guard: the last-resort path for HA's
//! continued-conversation mode. Matching is exact after [`normalize`], never
//! fuzzy.

use std::collections::HashSet;

use super::sanitize::normalize_utterance;

/// The exact local cancel phrases registered by the HA custom_sentences
/// StationCancel intent. The guard must not be broadened into fuzzy matching.
pub const DEFAULT_CANCEL_PHRASES: &[&str] = &[
    "отмена",
    "отмени",
    "передумал",
    "передумала",
    "не надо",
    "ничего",
    "отбой",
    "забудь",
    "неважно",
];

/// The phrase set and its normalised lookup form.
#[derive(Debug, Clone)]
pub struct CancelGuard {
    phrases: HashSet<String>,
}

impl Default for CancelGuard {
    fn default() -> Self {
        Self::new(DEFAULT_CANCEL_PHRASES)
    }
}

impl CancelGuard {
    /// Builds the lookup set from the configured phrases.
    #[must_use]
    pub fn new(phrases: &[&str]) -> Self {
        Self {
            phrases: phrases.iter().map(|phrase| normalize(phrase)).collect(),
        }
    }

    /// Returns the matched phrase, or `None`.
    #[must_use]
    pub fn matches(&self, text: &str) -> Option<String> {
        let normalized = normalize(text);
        self.phrases.contains(&normalized).then_some(normalized)
    }
}

/// The compact lookup form: NFKC, case folding, whitespace collapsing and
/// trailing STT punctuation removal.
#[must_use]
pub fn normalize(text: &str) -> String {
    normalize_utterance(text)
}

#[cfg(test)]
mod tests {
    use super::{CancelGuard, normalize};

    #[test]
    fn guard_matches_only_the_exact_normalised_phrase() {
        let guard = CancelGuard::default();
        assert_eq!(guard.matches("Отмена!"), Some("отмена".to_owned()));
        assert_eq!(
            guard.matches("  не\u{00a0}надо "),
            Some("не надо".to_owned())
        );
        assert_eq!(guard.matches("\"отмена\""), None);
        assert_eq!(guard.matches("отмени пожалуйста"), None);
        assert_eq!(guard.matches(""), None);
    }

    #[test]
    fn normalize_removes_sentence_final_punctuation_only() {
        assert_eq!(normalize("Отбой."), "отбой");
        assert_eq!(normalize("Отбой, ладно"), "отбой, ладно");
    }
}
