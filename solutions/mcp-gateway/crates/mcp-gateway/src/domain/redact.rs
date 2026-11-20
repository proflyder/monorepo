use std::sync::LazyLock;

use regex::{Captures, Regex};

pub const PLACEHOLDER: &str = "[REDACTED]";

static PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r#"(?i)(authorization[ \t\n\f\r]*[:=][ \t\n\f\r]*(?:bearer|basic|token)?[ \t\n\f\r]*)[^ \t\n\f\r"',;]+"#,
        r"(?i)(?-u:\b)(bearer[ \t\n\f\r]+)[A-Za-z0-9._~+/=-]{8,}",
        r#"(?i)(?-u:\b)((?:token|password|passwd|secret|api[_-]?key|access[_-]?key|private[_-]?key)[ \t\n\f\r]*[:=][ \t\n\f\r]*["']?)[^ \t\n\f\r"',;&]+"#,
        r"(?-u:\b)eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}(?-u:\b)",
        r"(?-u:\b)([0-9]{6,}:)[A-Za-z0-9_-]{30,}(?-u:\b)",
        r"(?-u:\b)[0-9a-fA-F]{32,}(?-u:\b)",
        r"(?-u:\b)[A-Za-z0-9+/_-]{33,}={0,2}(?-u:\b)",
    ]
    .into_iter()
    .map(|pattern| Regex::new(pattern).expect("a redaction pattern is a valid regex"))
    .collect()
});

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
