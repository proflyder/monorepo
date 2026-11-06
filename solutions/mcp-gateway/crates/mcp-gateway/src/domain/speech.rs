//! Spoken-line normalization.
//!
//! Ports `internal/tools/tools.go` (`Speech`). The voice path speaks the
//! resulting line verbatim, so it must be a single line without markdown, URLs
//! or more than [`MAX_SPEECH_RUNES`] runes.

use std::sync::LazyLock;

use regex::Regex;

/// The voice budget for the spoken line.
pub const MAX_SPEECH_RUNES: usize = 160;

// Go's `\S` is ASCII: `[^\t\n\f\r ]`. The pattern is spelled out so the Rust
// port does not gain Unicode whitespace semantics the Go regexp did not have.
static URL_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[^\t\n\f\r ]+://[^\t\n\f\r ]+").expect("the URL pattern is a valid regex")
});

/// Normalizes a spoken line: single line, no markdown, no URLs, at most
/// [`MAX_SPEECH_RUNES`] runes.
#[must_use]
pub fn speech(raw: &str) -> String {
    let replaced = URL_PATTERN.replace_all(raw, "ссылка");
    let mut out = String::with_capacity(replaced.len());
    let mut pending_space = false;
    for r in replaced.chars() {
        if r.is_whitespace() {
            pending_space = true;
            continue;
        }
        if "*_`#>|[]".contains(r) {
            continue;
        }
        if pending_space && !out.is_empty() {
            out.push(' ');
        }
        pending_space = false;
        out.push(r);
    }
    let out = out.trim();
    let runes: Vec<char> = out.chars().collect();
    if runes.len() > MAX_SPEECH_RUNES {
        let head: String = runes[..MAX_SPEECH_RUNES - 1].iter().collect();
        return format!("{}…", head.trim_end());
    }
    out.to_owned()
}
