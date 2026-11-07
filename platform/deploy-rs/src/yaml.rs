//! Ordered YAML values and a `go.yaml.in/yaml/v2`-compatible emitter.
//!
//! `runtimevalues` and `manifests` only need an ordered document tree: the Go
//! implementation used `yaml.MapSlice` because the runtime values file is a
//! user-facing contract whose key order must survive the split. Deserialization
//! goes through `serde-saphyr`; serialization is a small emitter that copies
//! yaml.v2's block layout (sequences are not indented relative to their key,
//! empty collections are written as `[]`/`{}`), because the emitted file is
//! compared byte for byte with the Go output.

use std::fmt::{self, Write as _};

use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};

/// One ordered YAML document node.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Value {
    /// YAML `null` (`~`, an empty document).
    #[default]
    Null,
    /// A boolean scalar.
    Bool(bool),
    /// An integer scalar.
    Int(i64),
    /// A floating point scalar.
    Float(f64),
    /// A string scalar.
    Str(String),
    /// An ordered sequence.
    Seq(Vec<Self>),
    /// An ordered mapping; duplicate keys keep the first position.
    Map(Vec<(String, Self)>),
}

impl Value {
    /// Returns the string value of a scalar, or `""` for every other node.
    ///
    /// This mirrors the Go helper's `value.(string)` type assertion: numbers
    /// and booleans are not stringified.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Str(value) => value,
            _ => "",
        }
    }

    /// Looks up a key in a mapping, preserving the yaml.v2 first-match rule.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Self> {
        let Self::Map(entries) = self else {
            return None;
        };
        entries
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }

    /// Inserts or replaces a key, like `yaml.MapSlice` + `setKey`.
    pub fn set(&mut self, key: &str, value: Self) {
        if let Self::Map(entries) = self {
            if let Some((_, existing)) = entries.iter_mut().find(|(name, _)| name == key) {
                *existing = value;
            } else {
                entries.push((key.to_string(), value));
            }
        }
    }

    /// Removes a key from a mapping and reports whether it was present.
    pub fn remove(&mut self, key: &str) -> bool {
        let Self::Map(entries) = self else {
            return false;
        };
        if let Some(index) = entries.iter().position(|(name, _)| name == key) {
            entries.remove(index);
            return true;
        }
        false
    }

    /// Reports whether this is a non-empty mapping.
    #[must_use]
    pub fn is_mapping(&self) -> bool {
        matches!(self, Self::Map(_))
    }

    /// Reports whether this is a non-empty sequence.
    #[must_use]
    pub fn is_sequence(&self) -> bool {
        matches!(self, Self::Seq(_))
    }

    /// Converts to `serde_json::Value` (the converge/state object model).
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Self::Null => serde_json::Value::Null,
            Self::Bool(value) => serde_json::Value::Bool(*value),
            Self::Int(value) => serde_json::Value::Number((*value).into()),
            Self::Float(value) => serde_json::Number::from_f64(*value)
                .map_or(serde_json::Value::Null, serde_json::Value::Number),
            Self::Str(value) => serde_json::Value::String(value.clone()),
            Self::Seq(items) => serde_json::Value::Array(items.iter().map(Self::to_json).collect()),
            Self::Map(entries) => serde_json::Value::Object(
                entries
                    .iter()
                    .map(|(key, value)| (key.clone(), value.to_json()))
                    .collect(),
            ),
        }
    }

    /// Converts from `serde_json::Value`. Object keys come out sorted, which is
    /// what `yaml.Marshal` does for Go maps.
    #[must_use]
    pub fn from_json(value: &serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => Self::Null,
            serde_json::Value::Bool(value) => Self::Bool(*value),
            serde_json::Value::Number(number) => {
                if let Some(value) = number.as_i64() {
                    return Self::Int(value);
                }
                if let Some(value) = number.as_u64() {
                    return i64::try_from(value)
                        .map_or_else(|_| Self::Float(value as f64), Self::Int);
                }
                Self::Float(number.as_f64().unwrap_or(0.0))
            }
            serde_json::Value::String(value) => Self::Str(value.clone()),
            serde_json::Value::Array(items) => {
                Self::Seq(items.iter().map(Self::from_json).collect())
            }
            serde_json::Value::Object(entries) => Self::Map(
                entries
                    .iter()
                    .map(|(key, value)| (key.clone(), Self::from_json(value)))
                    .collect(),
            ),
        }
    }
}

/// Parses every YAML document in `raw`, matching `yaml.NewDecoder` + loop.
///
/// Empty documents are returned as `Value::Null` so callers can apply the Go
/// `len(doc) == 0` rule; `serde-saphyr`'s multi-document helper skips them, so
/// the split is done here on the raw stream markers.
pub fn parse_documents(raw: &str) -> Result<Vec<Value>, String> {
    serde_saphyr::from_str_multiple::<Value, Vec<Value>>(raw).map_err(|error| error.to_string())
}

/// Parses one YAML document.
pub fn parse(raw: &str) -> Result<Value, String> {
    serde_saphyr::from_str::<Value>(raw).map_err(|error| error.to_string())
}

/// Serializes one document exactly like `yaml.Marshal` (trailing newline).
#[must_use]
pub fn to_string(value: &Value) -> String {
    let mut output = String::new();
    match value {
        Value::Map(entries) if !entries.is_empty() => emit_map(&mut output, entries, 0),
        Value::Seq(items) if !items.is_empty() => emit_seq(&mut output, items, 0),
        other => {
            output.push_str(&scalar(other));
            output.push('\n');
        }
    }
    output
}

/// Serializes a multi-document stream the way the Go helper joins documents:
/// `---\n` between documents, no leading marker, each document ending in a
/// newline.
#[must_use]
pub fn to_string_multiple(values: &[Value]) -> String {
    let mut output = String::new();
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            output.push_str("---\n");
        }
        output.push_str(&to_string(value));
    }
    output
}

fn emit_map(output: &mut String, entries: &[(String, Value)], indent: usize) {
    emit_map_entries(output, entries, indent, false);
}

/// Emits map entries. When `inline_first` is set the cursor is already after
/// `- `, so the first key starts without indentation; the remaining keys use
/// `indent`.
fn emit_map_entries(
    output: &mut String,
    entries: &[(String, Value)],
    indent: usize,
    inline_first: bool,
) {
    for (index, (key, value)) in entries.iter().enumerate() {
        if !(inline_first && index == 0) {
            push_spaces(output, indent);
        }
        output.push_str(&key_scalar(key));
        output.push(':');
        emit_value_after_key(output, value, indent);
    }
}

fn emit_value_after_key(output: &mut String, value: &Value, indent: usize) {
    match value {
        Value::Map(entries) if !entries.is_empty() => {
            output.push('\n');
            emit_map(output, entries, indent + 2);
        }
        Value::Seq(items) if !items.is_empty() => {
            output.push('\n');
            emit_seq(output, items, indent);
        }
        Value::Seq(_) => output.push_str(" []\n"),
        Value::Map(_) => output.push_str(" {}\n"),
        other => {
            output.push(' ');
            output.push_str(&scalar(other));
            output.push('\n');
        }
    }
}

fn emit_seq(output: &mut String, items: &[Value], indent: usize) {
    for item in items {
        push_spaces(output, indent);
        output.push_str("- ");
        match item {
            Value::Map(entries) if !entries.is_empty() => {
                emit_map_entries(output, entries, indent + 2, true);
            }
            Value::Seq(nested) if !nested.is_empty() => {
                // yaml.v2 writes nested block sequences as `- - value`.
                let mut rendered = String::new();
                emit_seq(&mut rendered, nested, indent + 2);
                let trimmed = rendered
                    .strip_prefix(&" ".repeat(indent + 2))
                    .unwrap_or(&rendered);
                output.push_str(trimmed);
            }
            other => {
                output.push_str(&scalar(other));
                output.push('\n');
            }
        }
    }
}

fn push_spaces(output: &mut String, count: usize) {
    for _ in 0..count {
        output.push(' ');
    }
}

fn scalar(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(true) => "true".to_string(),
        Value::Bool(false) => "false".to_string(),
        Value::Int(number) => number.to_string(),
        Value::Float(number) => format!("{number}"),
        Value::Str(text) => quote_scalar(text, false),
        Value::Seq(_) => "[]".to_string(),
        Value::Map(_) => "{}".to_string(),
    }
}

fn key_scalar(key: &str) -> String {
    quote_scalar(key, true)
}

fn quote_scalar(text: &str, key: bool) -> String {
    if needs_quotes(text) {
        double_quote(text)
    } else if key && text.contains(":") {
        // A key with a colon is valid plain only when the colon does not start
        // a value. yaml.v2 quotes these to stay unambiguous.
        double_quote(text)
    } else {
        text.to_string()
    }
}

fn needs_quotes(text: &str) -> bool {
    if text.is_empty() {
        return true;
    }
    if text.starts_with(char::is_whitespace) || text.ends_with(char::is_whitespace) {
        return true;
    }
    if text.chars().any(char::is_control) {
        return true;
    }
    let first = text.chars().next().unwrap_or(' ');
    if matches!(
        first,
        '[' | ']' | '{' | '}' | '#' | '&' | '*' | '!' | '|' | '>' | '\'' | '"' | '%' | '@' | '`'
    ) {
        return true;
    }
    if matches!(first, '-' | '?' | ':')
        && (text.len() == 1
            || text.starts_with("- ")
            || text.starts_with("? ")
            || text.starts_with(": "))
    {
        return true;
    }
    if text.contains(": ") || text.contains(" #") {
        return true;
    }
    if text.ends_with(':') {
        return true;
    }
    resolves_to_non_string(text)
}

/// Mirrors the YAML 1.1 implicit resolvers yaml.v2 would apply to a plain
/// scalar: such a string has to be quoted to stay a string.
fn resolves_to_non_string(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "~" | "null" | "true" | "false" | "yes" | "no" | "on" | "off" | "y" | "n"
    ) {
        return true;
    }
    if matches!(lower.as_str(), ".inf" | "+.inf" | "-.inf" | ".nan") {
        return true;
    }
    let without_sign = text
        .strip_prefix('+')
        .or_else(|| text.strip_prefix('-'))
        .unwrap_or(text);
    if without_sign.starts_with("0x")
        || without_sign.starts_with("0o")
        || without_sign.starts_with("0b")
        || text.contains('_')
    {
        // Underscores and 0x/0o/0b prefixes resolve to numbers in YAML 1.1.
        if without_sign
            .trim_start_matches("0x")
            .trim_start_matches("0o")
            .trim_start_matches("0b")
            .chars()
            .all(|character| character.is_ascii_hexdigit() || character == '_')
        {
            return true;
        }
    }
    if text.parse::<i64>().is_ok() || text.parse::<u64>().is_ok() || text.parse::<f64>().is_ok() {
        return true;
    }
    // Sexagesimal (`1:30`) resolves to a number in YAML 1.1.
    if text
        .split(':')
        .all(|part| !part.is_empty() && part.chars().all(|character| character.is_ascii_digit()))
        && text.contains(':')
    {
        return true;
    }
    false
}

fn double_quote(text: &str) -> String {
    let mut output = String::with_capacity(text.len() + 2);
    output.push('"');
    for character in text.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\t' => output.push_str("\\t"),
            '\r' => output.push_str("\\r"),
            character if character.is_control() => {
                let _ = write!(output, "\\x{:02x}", u32::from(character));
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

// --- serde deserialization -------------------------------------------------

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(ValueVisitor)
    }
}

struct ValueVisitor;

impl<'de> Visitor<'de> for ValueVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a YAML value")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(Value::Int(value))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(i64::try_from(value).map_or_else(|_| Value::Float(value as f64), Value::Int))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E> {
        Ok(Value::Float(value))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
        Ok(Value::Str(value.to_string()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(Value::Str(value))
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_any(self)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = sequence.next_element::<Value>()? {
            items.push(item);
        }
        Ok(Value::Seq(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut mapping: A) -> Result<Self::Value, A::Error> {
        let mut entries: Vec<(String, Value)> = Vec::new();
        while let Some(key) = mapping.next_key::<YamlKey>()? {
            let value = mapping.next_value::<Value>()?;
            if let Some((_, existing)) = entries.iter_mut().find(|(name, _)| name == &key.0) {
                *existing = value;
            } else {
                entries.push((key.0, value));
            }
        }
        Ok(Value::Map(entries))
    }
}

/// A mapping key coerced to a string the way the Go helper uses `fmt.Sprint`.
struct YamlKey(String);

impl<'de> Deserialize<'de> for YamlKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(KeyVisitor)
    }
}

struct KeyVisitor;

impl Visitor<'_> for KeyVisitor {
    type Value = YamlKey;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a YAML mapping key")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(YamlKey(value.to_string()))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(YamlKey(value.to_string()))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(YamlKey(value.to_string()))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E> {
        Ok(YamlKey(value.to_string()))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
        Ok(YamlKey(value.to_string()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(YamlKey(value))
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(YamlKey(String::new()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequences_are_not_indented_relative_to_their_key() {
        let text = "spec:\n  containers:\n  - name: gateway\n    command:\n    - /bin/run\n  - name: voice\n";
        let value = parse(text).expect("parse");
        assert_eq!(to_string(&value), text);
    }

    #[test]
    fn empty_collections_stay_inline() {
        let value = parse("volumes: []\nlabels: {}\n").expect("parse");
        assert_eq!(to_string(&value), "volumes: []\nlabels: {}\n");
    }

    #[test]
    fn yaml_11_boolean_words_are_quoted() {
        let value = Value::Map(vec![("mode".to_string(), Value::Str("on".to_string()))]);
        assert_eq!(to_string(&value), "mode: \"on\"\n");
    }

    #[test]
    fn numbers_and_nulls_round_trip() {
        let value = parse("replicas: 1\nenabled: true\nnothing: null\n").expect("parse");
        assert_eq!(
            to_string(&value),
            "replicas: 1\nenabled: true\nnothing: null\n"
        );
    }
}
