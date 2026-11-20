use std::sync::LazyLock;

use regex::Regex;

pub const MAX_SPEECH_RUNES: usize = 160;

static URL_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[^\t\n\f\r ]+://[^\t\n\f\r ]+").expect("the URL pattern is a valid regex")
});

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
