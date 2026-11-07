//! Ordered JSON with Python `json.dumps` formatting.
//!
//! The OCI helpers must re-serialize `index.json` exactly like Python's
//! `json.dumps(index, indent=2)`: object key order is preserved and strings
//! escape everything outside printable ASCII except `<> &`. `serde_json::Value`
//! sorts map keys (and escapes HTML), so this module keeps a small ordered tree.

use std::fmt;

use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};

/// One JSON node with insertion-ordered objects.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    /// `null`.
    Null,
    /// `true`/`false`.
    Bool(bool),
    /// A numeric literal, kept as its textual representation.
    Number(String),
    /// A string.
    String(String),
    /// An array.
    Array(Vec<Self>),
    /// An insertion-ordered object.
    Object(Vec<(String, Self)>),
}

impl Json {
    /// Looks up an object member.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Self> {
        let Self::Object(entries) = self else {
            return None;
        };
        entries
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }

    /// Mutably looks up an object member.
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Self> {
        let Self::Object(entries) = self else {
            return None;
        };
        entries
            .iter_mut()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }

    /// Inserts or replaces a member, keeping its original position.
    pub fn set(&mut self, key: &str, value: Self) {
        if let Self::Object(entries) = self {
            if let Some((_, existing)) = entries.iter_mut().find(|(name, _)| name == key) {
                *existing = value;
            } else {
                entries.push((key.to_string(), value));
            }
        }
    }

    /// Reports whether this is an object.
    #[must_use]
    pub fn is_object(&self) -> bool {
        matches!(self, Self::Object(_))
    }

    /// Parses `data` preserving object key order and numeric literals.
    pub fn parse(data: &[u8]) -> Result<Self, String> {
        let mut deserializer = serde_json::Deserializer::from_slice(data);
        let value = Self::deserialize(&mut deserializer).map_err(|error| error.to_string())?;
        deserializer.end().map_err(|error| error.to_string())?;
        Ok(value)
    }
}

impl<'de> Deserialize<'de> for Json {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(JsonVisitor)
    }
}

struct JsonVisitor;

impl<'de> Visitor<'de> for JsonVisitor {
    type Value = Json;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(Json::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(Json::Number(value.to_string()))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(Json::Number(value.to_string()))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E> {
        let number = serde_json::Number::from_f64(value)
            .map_or_else(|| "null".to_string(), |number| number.to_string());
        Ok(Json::Number(number))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
        Ok(Json::String(value.to_string()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(Json::String(value))
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(Json::Null)
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(Json::Null)
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_any(self)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = sequence.next_element::<Json>()? {
            items.push(item);
        }
        Ok(Json::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut mapping: A) -> Result<Self::Value, A::Error> {
        let mut entries: Vec<(String, Json)> = Vec::new();
        while let Some(key) = mapping.next_key::<String>()? {
            let value = mapping.next_value::<Json>()?;
            if let Some((_, existing)) = entries.iter_mut().find(|(name, _)| name == &key) {
                *existing = value;
            } else {
                entries.push((key, value));
            }
        }
        Ok(Json::Object(entries))
    }
}

/// Renders like `json.dumps(value, indent=2)`: two spaces per level, no
/// trailing newline, ASCII-only output.
#[must_use]
pub fn dump_indent(value: &Json) -> Vec<u8> {
    let mut buffer = Vec::with_capacity(256);
    append_json(&mut buffer, value, 2, 0);
    buffer
}

/// Renders like `json.dumps(value)`: Python's default separators (`, ` and
/// `: `) on a single line.
#[must_use]
pub fn dump_compact(value: &Json) -> Vec<u8> {
    let mut buffer = Vec::with_capacity(128);
    append_json(&mut buffer, value, 0, 0);
    buffer
}

fn append_json(buffer: &mut Vec<u8>, value: &Json, indent: usize, level: usize) {
    match value {
        Json::Null => buffer.extend_from_slice(b"null"),
        Json::Bool(true) => buffer.extend_from_slice(b"true"),
        Json::Bool(false) => buffer.extend_from_slice(b"false"),
        Json::Number(number) => buffer.extend_from_slice(number.as_bytes()),
        Json::String(text) => append_python_string(buffer, text),
        Json::Array(items) => {
            if items.is_empty() {
                buffer.extend_from_slice(b"[]");
                return;
            }
            buffer.push(b'[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    append_item_separator(buffer, indent, level + 1);
                } else if indent > 0 {
                    append_newline_indent(buffer, indent, level + 1);
                }
                append_json(buffer, item, indent, level + 1);
            }
            if indent > 0 {
                append_newline_indent(buffer, indent, level);
            }
            buffer.push(b']');
        }
        Json::Object(entries) => {
            if entries.is_empty() {
                buffer.extend_from_slice(b"{}");
                return;
            }
            buffer.push(b'{');
            for (index, (key, item)) in entries.iter().enumerate() {
                if index > 0 {
                    append_item_separator(buffer, indent, level + 1);
                } else if indent > 0 {
                    append_newline_indent(buffer, indent, level + 1);
                }
                append_python_string(buffer, key);
                buffer.extend_from_slice(b": ");
                append_json(buffer, item, indent, level + 1);
            }
            if indent > 0 {
                append_newline_indent(buffer, indent, level);
            }
            buffer.push(b'}');
        }
    }
}

fn append_item_separator(buffer: &mut Vec<u8>, indent: usize, level: usize) {
    buffer.push(b',');
    if indent > 0 {
        append_newline_indent(buffer, indent, level);
    } else {
        buffer.push(b' ');
    }
}

fn append_newline_indent(buffer: &mut Vec<u8>, indent: usize, level: usize) {
    buffer.push(b'\n');
    for _ in 0..indent * level {
        buffer.push(b' ');
    }
}

/// Escapes a string exactly like Python's `json.dumps` with
/// `ensure_ascii=True`: short escapes where Python has one, `\uXXXX` for
/// everything outside printable ASCII (lower-case hex, surrogate pairs for
/// non-BMP runes), and `<`, `>`, `&` literal.
fn append_python_string(buffer: &mut Vec<u8>, value: &str) {
    buffer.push(b'"');
    for character in value.chars() {
        match character {
            '"' => buffer.extend_from_slice(b"\\\""),
            '\\' => buffer.extend_from_slice(b"\\\\"),
            '\n' => buffer.extend_from_slice(b"\\n"),
            '\t' => buffer.extend_from_slice(b"\\t"),
            '\r' => buffer.extend_from_slice(b"\\r"),
            '\u{08}' => buffer.extend_from_slice(b"\\b"),
            '\u{0c}' => buffer.extend_from_slice(b"\\f"),
            character if ('\u{20}'..='\u{7e}').contains(&character) => {
                buffer.push(character as u8);
            }
            character => {
                let mut units = [0u16; 2];
                for unit in character.encode_utf16(&mut units) {
                    append_unicode_escape(buffer, *unit);
                }
            }
        }
    }
    buffer.push(b'"');
}

fn append_unicode_escape(buffer: &mut Vec<u8>, unit: u16) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    buffer.extend_from_slice(b"\\u");
    buffer.push(HEX[usize::from((unit >> 12) & 0xf)]);
    buffer.push(HEX[usize::from((unit >> 8) & 0xf)]);
    buffer.push(HEX[usize::from((unit >> 4) & 0xf)]);
    buffer.push(HEX[usize::from(unit & 0xf)]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indent_matches_python_and_keeps_order() {
        let value = Json::parse(
            br#"{"schemaVersion": 2, "manifests": [{"size": 417, "digest": "sha256:aa"}]}"#,
        )
        .expect("parse");
        let text = String::from_utf8(dump_indent(&value)).expect("utf8");
        assert_eq!(
            text,
            "{\n  \"schemaVersion\": 2,\n  \"manifests\": [\n    {\n      \"size\": 417,\n      \"digest\": \"sha256:aa\"\n    }\n  ]\n}"
        );
    }

    #[test]
    fn compact_uses_python_separators() {
        let value = Json::parse(br#"{"a": [1, "x"], "b": null}"#).expect("parse");
        assert_eq!(
            String::from_utf8(dump_compact(&value)).expect("utf8"),
            r#"{"a": [1, "x"], "b": null}"#
        );
    }

    #[test]
    fn strings_are_ascii_only_but_keep_html_characters() {
        let value = Json::String("a<b>&c\u{00e9}\u{1f600}".to_string());
        assert_eq!(
            String::from_utf8(dump_compact(&value)).expect("utf8"),
            "\"a<b>&c\\u00e9\\ud83d\\ude00\""
        );
    }
}
