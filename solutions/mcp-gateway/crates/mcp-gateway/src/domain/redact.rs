//! Secret redaction.
//!
//! Ports `internal/redact/redact.go`. The rules run in order and every rule may
//! declare at most one capture group holding the non-secret prefix; the secret
//! itself is always outside the group, so a replacement can never reintroduce
//! it.

use std::sync::LazyLock;

use regex::{Captures, Regex};

/// Replaces every detected secret value.
pub const PLACEHOLDER: &str = "[REDACTED]";

// `\s` and `\b` are ASCII in Go's regexp. They are spelled out (respectively
// switched to ASCII with `(?-u:...)`) so the Rust port cannot start matching
// Unicode whitespace or treating Cyrillic letters as word characters.
static PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        // Authorization headers, any scheme. Group 1: the header prefix.
        r#"(?i)(authorization[ \t\n\f\r]*[:=][ \t\n\f\r]*(?:bearer|basic|token)?[ \t\n\f\r]*)[^ \t\n\f\r"',;]+"#,
        // Bare bearer tokens. Group 1: the word "bearer".
        r"(?i)(?-u:\b)(bearer[ \t\n\f\r]+)[A-Za-z0-9._~+/=-]{8,}",
        // key=value / key: value for common secret names. Group 1: the key.
        r#"(?i)(?-u:\b)((?:token|password|passwd|secret|api[_-]?key|access[_-]?key|private[_-]?key)[ \t\n\f\r]*[:=][ \t\n\f\r]*["']?)[^ \t\n\f\r"',;&]+"#,
        // JSON Web Tokens. No group: the whole match is the secret.
        r"(?-u:\b)eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}(?-u:\b)",
        // Telegram bot tokens and similar "<digits>:<secret>" forms. Group 1
        // is only the numeric bot id and the colon.
        r"(?-u:\b)([0-9]{6,}:)[A-Za-z0-9_-]{30,}(?-u:\b)",
        // Long hexadecimal blobs (keys, digests, checksums printed by tools).
        r"(?-u:\b)[0-9a-fA-F]{32,}(?-u:\b)",
        // Long base64-ish blobs. This over-approximates on purpose: a
        // 33-character unbroken token is almost never ordinary log text.
        r"(?-u:\b)[A-Za-z0-9+/_-]{33,}={0,2}(?-u:\b)",
    ]
    .into_iter()
    .map(|pattern| Regex::new(pattern).expect("a redaction pattern is a valid regex"))
    .collect()
});

/// Redacts every secret-like value in `text`.
#[must_use]
pub fn text(text: &str) -> String {
    let mut current = text.to_owned();
    for pattern in PATTERNS.iter() {
        current = redact_with(pattern, &current);
    }
    current
}

fn redact_with(pattern: &Regex, text: &str) -> String {
    pattern
        .replace_all(text, |caps: &Captures<'_>| {
            caps.get(1).map_or_else(
                || PLACEHOLDER.to_owned(),
                |prefix| format!("{}{PLACEHOLDER}", prefix.as_str()),
            )
        })
        .into_owned()
}
