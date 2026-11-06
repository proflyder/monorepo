//! Turns assistant replies into text that Piper TTS can speak.
//!
//! Go's RE2 `\s` and `\b` are ASCII-only, so every whitespace class is written
//! out explicitly: ASCII whitespace, the C0 information separators
//! U+001C..U+001F, NEL (U+0085) and the Unicode separator categories Zs, Zl
//! and Zp. The same set drives [`is_space`], [`trim_space`] and
//! [`collapse_space`].

use std::fmt;

use caseless::default_case_fold_str;
use regex::{Regex, RegexBuilder};
use unicode_normalization::UnicodeNormalization as _;

/// Bounds the buffered streaming tail like the 10 MiB request cap; a runaway
/// stream fails instead of growing without bound.
pub const MAX_STREAM_BYTES: usize = 10 << 20;

/// The sentence-final punctuation STT may append.
const TRAILING_PUNCT: &str = ".!?…";

/// Errors of a streaming sanitizer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum StreamError {
    #[error("stream text too large")]
    TooLarge,
}

/// One character of the Unicode space set: ASCII whitespace, the C0
/// information separators, NEL and the Zs/Zl/Zp categories.
///
/// `char::is_whitespace` covers every `Zs`/`Zl`/`Zp` code point (and the
/// newline family); the information separators are missing there and are added
/// explicitly.
#[must_use]
pub fn is_space(r: char) -> bool {
    matches!(r, '\u{1c}'..='\u{1f}') || r.is_whitespace()
}

/// Trims the Unicode space set, including the C0 information separators that
/// `str::trim` leaves behind.
#[must_use]
pub fn trim_space(text: &str) -> &str {
    text.trim_matches(is_space)
}

/// `" ".join(text.split())`: split on runs of whitespace, join with one space.
#[must_use]
pub fn collapse_space(text: &str) -> String {
    text.split(is_space)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

struct Patterns {
    citation: Regex,
    md_link: Regex,
    bare_url: Regex,
    md_line_prefix: Regex,
    md_marks: Regex,
    empty_parens: Regex,
    run_of_space: Regex,
    truncated_link: Regex,
    truncated_http: Regex,
    question_tail: Regex,
}

impl Patterns {
    fn new() -> Self {
        let space = r"[ \t\n\v\f\r\x{1C}-\x{1F}\x{85}\p{Z}]";
        let not_space = r#"[^ \t\n\v\f\r\x{1C}-\x{1F}\x{85}\p{Z}\]\)>\"']"#;
        let citation = Regex::new(&format!(r"{space}*\(\[[^\]]+\]\(https?://[^)]+\)\)"));
        let multi = |pattern: String| {
            RegexBuilder::new(&pattern)
                .multi_line(true)
                .build()
                .expect("static regex")
        };
        Self {
            citation: citation.expect("static regex"),
            md_link: Regex::new(r"\[([^\]]+)\]\(https?://[^)]+\)").expect("static regex"),
            bare_url: Regex::new(&format!("https?://{not_space}+")).expect("static regex"),
            md_line_prefix: multi(format!(
                r"^{space}*(?:#{{1,6}}{space}*|[-*+]{space}+|\d+[.)]{space}+)"
            )),
            md_marks: Regex::new(r"[*_`~]+").expect("static regex"),
            empty_parens: Regex::new(&format!(r"\({space}*\)")).expect("static regex"),
            run_of_space: Regex::new(&format!("{space}+")).expect("static regex"),
            truncated_link: Regex::new(r"\[[^\]]*$|\[[^\]]+\]\([^)]*$").expect("static regex"),
            truncated_http: Regex::new(r"https?(?::/{0,2})?$").expect("static regex"),
            question_tail: Regex::new(&format!(r"[^.!?…？]*[?？]{space}*$")).expect("static regex"),
        }
    }
}

thread_local! {
    /// Compiled once per thread: the patterns never change at runtime.
    static PATTERNS: Patterns = Patterns::new();
}

fn with_patterns<R>(f: impl FnOnce(&Patterns) -> R) -> R {
    PATTERNS.with(f)
}

/// Returns a TTS-friendly version of an assistant reply.
#[must_use]
pub fn sanitize_text(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    with_patterns(|p| {
        let text = p.citation.replace_all(text, "");
        let text = p.md_link.replace_all(&text, "$1");
        let text = p.bare_url.replace_all(&text, "");
        let text = p.md_line_prefix.replace_all(&text, "");
        let text = p.md_marks.replace_all(&text, "");
        let text = p.empty_parens.replace_all(&text, "");
        let text = p.run_of_space.replace_all(&text, " ");
        trim_space(&text).to_owned()
    })
}

/// Sanitises every `choices[].message.content` string of an OpenAI Chat
/// Completions payload, in place.
pub fn sanitize_chat_payload(payload: &mut serde_json::Value) {
    let Some(choices) = payload
        .get_mut("choices")
        .and_then(|value| value.as_array_mut())
    else {
        return;
    };
    for choice in choices {
        let Some(message) = choice.get_mut("message") else {
            continue;
        };
        if let Some(content) = message.get("content").and_then(|value| value.as_str()) {
            let sanitized = sanitize_text(content);
            message["content"] = serde_json::Value::String(sanitized);
        }
    }
}

/// Releases complete sentences while keeping split links and URLs out of
/// speech. A boundary must be outside links/parentheses and followed by
/// whitespace; the tail is held until completion, so the regexes never see
/// half a URL.
#[derive(Debug, Default)]
pub struct StreamingSanitizer {
    pending: String,
    emitted: bool,
}

impl StreamingSanitizer {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds text and returns the speech that became safe to speak.
    ///
    /// # Errors
    ///
    /// Returns [`StreamError::TooLarge`] when the buffered tail exceeds
    /// [`MAX_STREAM_BYTES`].
    pub fn feed(&mut self, text: &str, final_chunk: bool) -> Result<String, StreamError> {
        self.pending.push_str(text);
        if self.pending.len() > MAX_STREAM_BYTES {
            return Err(StreamError::TooLarge);
        }
        let end = if final_chunk {
            self.pending.len()
        } else {
            boundary_end(&self.pending)
        };
        if end == 0 {
            return Ok(String::new());
        }
        let mut raw = self.pending[..end].to_owned();
        let rest = self.pending[end..].to_owned();
        self.pending = rest;
        if final_chunk {
            // A truncated/incomplete link is never safe to speak at the end.
            with_patterns(|p| {
                raw = p.truncated_link.replace_all(&raw, "").into_owned();
                raw = p.truncated_http.replace_all(&raw, "").into_owned();
            });
        }
        let cleaned = sanitize_text(&raw);
        if cleaned.is_empty() {
            return Ok(String::new());
        }
        let result = if self.emitted {
            format!(" {cleaned}")
        } else {
            cleaned
        };
        self.emitted = true;
        Ok(result)
    }
}

/// The equivalent of the scanner
///
/// ```text
/// r"https?://[^\s)\]]*|[\[\]()]|[.!?…]+(?=\s)"
/// ```
///
/// and returns the byte index after the last sentence boundary that is outside
/// a link/parenthesis. As in Go, the leftmost-first order of the alternatives
/// is preserved.
fn boundary_end(text: &str) -> usize {
    let mut brackets = 0usize;
    let mut parens = 0usize;
    let mut end = 0usize;
    let mut index = 0usize;
    while index < text.len() {
        if let Some(scheme) = url_scheme_len(&text[index..]) {
            index += scheme;
            while index < text.len() {
                let r = text[index..].chars().next().expect("in-bounds char");
                if is_space(r) || r == ')' || r == ']' {
                    break;
                }
                index += r.len_utf8();
            }
            continue;
        }
        let r = text[index..].chars().next().expect("in-bounds char");
        match r {
            '[' => {
                brackets += 1;
                index += 1;
            }
            ']' => {
                brackets = brackets.saturating_sub(1);
                index += 1;
            }
            '(' => {
                parens += 1;
                index += 1;
            }
            ')' => {
                parens = parens.saturating_sub(1);
                index += 1;
            }
            '.' | '!' | '?' | '…' => {
                while index < text.len() {
                    let next = text[index..].chars().next().expect("in-bounds char");
                    if !matches!(next, '.' | '!' | '?' | '…') {
                        break;
                    }
                    index += next.len_utf8();
                }
                // The lookahead requires whitespace after the run: a
                // sentence-final mark at the very end of the buffer is not a
                // boundary yet.
                if index < text.len() {
                    let next = text[index..].chars().next().expect("in-bounds char");
                    if is_space(next) && brackets == 0 && parens == 0 {
                        end = index;
                    }
                }
            }
            _ => index += r.len_utf8(),
        }
    }
    end
}

fn url_scheme_len(text: &str) -> Option<usize> {
    for scheme in ["https://", "http://"] {
        if text.starts_with(scheme) {
            return Some(scheme.len());
        }
    }
    None
}

/// The compact lowercase form used only for exact cancel matching: NFKC, full
/// Unicode case folding, whitespace collapsing and sentence-final STT
/// punctuation removal. Quotes, hyphens and internal commas survive so a
/// quoted or reworded phrase is not treated as an exact cancel command.
#[must_use]
pub fn normalize_utterance(text: &str) -> String {
    let normalized: String = text.nfkc().collect();
    let folded = default_case_fold_str(&normalized);
    let trimmed = trim_space(&folded);
    let trimmed = trimmed.trim_end_matches(|r| TRAILING_PUNCT.contains(r));
    collapse_space(trimmed)
}

/// Returns a short spoken phrase that cannot trigger HA's continued-conversation
/// mode.
#[must_use]
pub fn ensure_not_question(text: &str, agents_down: &str) -> String {
    let mut value = collapse_space(text);
    loop {
        if let Some(stripped) = value.strip_suffix('？') {
            value = trim_space(stripped).to_owned();
            continue;
        }
        if let Some(stripped) = value.strip_suffix('?') {
            value = trim_space(stripped).to_owned();
            continue;
        }
        break;
    }
    if value.is_empty() {
        return agents_down.to_owned();
    }
    let last = value.chars().next_back().expect("non-empty");
    if !".!…".contains(last) {
        value.push('.');
    }
    value
}

/// Reports whether `text` ends, after trailing whitespace, with a question
/// mark. HA continues the dialog only in that case.
#[must_use]
pub fn ends_with_question(text: &str) -> bool {
    let trimmed = text.trim_end_matches(is_space);
    trimmed.ends_with('?') || trimmed.ends_with('？')
}

/// Cuts text to at most `max_chars` characters, preferring the last word
/// boundary, and strips punctuation that looks wrong before an ellipsis.
fn truncate_speech(text: &str, max_chars: usize) -> String {
    let mut cut: String = text.chars().take(max_chars).collect();
    if let Some(index) = cut.rfind(' ') {
        cut.truncate(index);
    }
    cut.trim_end_matches([' ', ',', ';', ':', '—', '-'])
        .to_owned()
}

/// Sanitises and shortens a late answer for a station announcement. A question
/// keeps its trailing "?" because HA only continues the dialog when the spoken
/// text ends with one. An over-long answer prefers its final question sentence.
#[must_use]
pub fn announce_text(speech: &str, max_chars: usize) -> String {
    let text = sanitize_text(speech);
    if text.chars().count() <= max_chars {
        return text;
    }
    if ends_with_question(&text) {
        let last_question = with_patterns(|p| {
            p.question_tail
                .find(&text)
                .map_or_else(|| text.clone(), |found| found.as_str().to_owned())
        });
        let last_question = trim_space(&last_question).to_owned();
        if last_question.chars().count() <= max_chars {
            return last_question;
        }
        let cut = truncate_speech(&last_question, max_chars);
        if ends_with_question(&cut) {
            return cut;
        }
        let last = text.trim_end_matches(is_space).chars().next_back();
        return match last {
            Some(ch) => format!("{cut}{ch}"),
            None => cut,
        };
    }
    format!("{}…", truncate_speech(&text, max_chars))
}

impl fmt::Display for StreamingSanitizer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.pending)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        StreamingSanitizer, announce_text, collapse_space, ends_with_question, ensure_not_question,
        normalize_utterance, sanitize_chat_payload, sanitize_text, trim_space,
    };

    #[test]
    fn sanitize_strips_markdown_links_citations_and_urls() {
        assert_eq!(
            sanitize_text("Привет [Reuters](https://example.com/a)."),
            "Привет Reuters."
        );
        assert_eq!(
            sanitize_text("Текст ([источник](https://example.com))."),
            "Текст."
        );
        assert_eq!(
            sanitize_text("Смотри https://example.com/x?y=1 и всё."),
            "Смотри и всё."
        );
        assert_eq!(sanitize_text("# Заголовок\n- пункт"), "Заголовок пункт");
        assert_eq!(sanitize_text("**жирный** `код` ~x~"), "жирный код x");
        assert_eq!(sanitize_text("пустые () скобки"), "пустые скобки");
        assert_eq!(sanitize_text("  много\t\n пробелов  "), "много пробелов");
        assert_eq!(sanitize_text(""), "");
    }

    #[test]
    fn streaming_holds_the_tail_and_never_speaks_a_truncated_link() {
        let mut sanitizer = StreamingSanitizer::new();
        assert_eq!(sanitizer.feed("Первое предло", false).expect("feed"), "");
        assert_eq!(
            sanitizer.feed("жение. Второе", false).expect("feed"),
            "Первое предложение."
        );
        assert_eq!(
            sanitizer
                .feed(" [Reuters](https://exa", false)
                .expect("feed"),
            ""
        );
        // The sentence ends after the link; the tail stays buffered.
        assert_eq!(
            sanitizer.feed("mple.com). Хвост", false).expect("feed"),
            " Второе Reuters."
        );
        assert_eq!(sanitizer.feed("", true).expect("final"), " Хвост");
    }

    #[test]
    fn streaming_is_stable_at_every_split_point() {
        let text = "Раз. Два [ссылка](https://example.com). Три?";
        let expected = {
            let mut sanitizer = StreamingSanitizer::new();
            sanitizer.feed(text, true).expect("whole")
        };
        for split in 0..=text.len() {
            if !text.is_char_boundary(split) {
                continue;
            }
            let mut sanitizer = StreamingSanitizer::new();
            let mut out = sanitizer.feed(&text[..split], false).expect("first");
            out.push_str(&sanitizer.feed(&text[split..], true).expect("second"));
            assert_eq!(out, expected, "split {split}");
        }
    }

    #[test]
    fn streaming_is_stable_char_by_char() {
        let text = "Одно. Два https://example.com Три!";
        let expected = {
            let mut sanitizer = StreamingSanitizer::new();
            sanitizer.feed(text, true).expect("whole")
        };
        let mut sanitizer = StreamingSanitizer::new();
        let mut out = String::new();
        for ch in text.chars() {
            out.push_str(&sanitizer.feed(&ch.to_string(), false).expect("feed"));
        }
        out.push_str(&sanitizer.feed("", true).expect("final"));
        assert_eq!(out, expected);
    }

    #[test]
    fn normalize_utterance_folds_case_and_trailing_punctuation() {
        assert_eq!(normalize_utterance("  ОтМеНа!  "), "отмена");
        assert_eq!(normalize_utterance("Не\u{00a0}надо…"), "не надо");
        // Full case folding, not lowercase.
        assert_eq!(normalize_utterance("STRASSE"), "strasse");
        // Quotes survive so a quoted phrase is not an exact command.
        assert_eq!(normalize_utterance("\"отмена\""), "\"отмена\"");
    }

    #[test]
    fn ensure_not_question_strips_question_marks_and_adds_a_full_stop() {
        assert_eq!(ensure_not_question("Готово?", "down"), "Готово.");
        assert_eq!(ensure_not_question("Правда?？", "down"), "Правда.");
        assert_eq!(ensure_not_question("  ", "down"), "down");
        assert_eq!(ensure_not_question("Вопрос…", "down"), "Вопрос…");
    }

    #[test]
    fn announce_shortens_and_prefers_the_last_question() {
        assert_eq!(announce_text("**Готово**.", 200), "Готово.");
        assert_eq!(announce_text("Очень длинное предложение тут", 10), "Очень…");
        // A question longer than the budget keeps its trailing "?".
        assert_eq!(
            announce_text("Длинно. А потом вопрос, который важен?", 20),
            "А потом вопрос?"
        );
        assert_eq!(announce_text("Длинное вступление. Вопрос?", 20), "Вопрос?");
        assert!(ends_with_question(" что? "));
        assert!(!ends_with_question("что."));
    }

    #[test]
    fn chat_payload_sanitises_only_message_content() {
        let mut payload = json!({
            "choices": [
                {"message": {"role": "assistant", "content": "**Да** [x](https://e.com)."}},
                {"message": {"role": "assistant", "content": [{"type": "text"}]}}
            ]
        });
        sanitize_chat_payload(&mut payload);
        assert_eq!(payload["choices"][0]["message"]["content"], "Да x.");
        assert!(payload["choices"][1]["message"]["content"].is_array());
    }

    #[test]
    fn whitespace_helpers_cover_the_unicode_set() {
        assert_eq!(collapse_space("a\u{1c}b\u{202f}c"), "a b c");
        assert_eq!(trim_space("\u{1f}x\u{3000}"), "x");
    }
}
