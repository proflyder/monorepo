//! `encoding/json.Marshal`-compatible serialization of `serde_json::Value`.
//!
//! `serde_json` writes compact JSON but does not HTML-escape `<`, `>`, `&` or
//! U+2028/U+2029, while Go's encoder does by default. The converge replace
//! payload and the rollback restore payload are byte contracts with the Go
//! binary, so they are serialized here instead. Object keys are sorted by
//! `serde_json::Map` (a `BTreeMap`), which is also what Go does for maps.

use serde_json::Value;

/// Serializes a JSON value exactly like `json.Marshal`.
#[must_use]
pub fn marshal(value: &Value) -> String {
    let mut output = String::new();
    append(&mut output, value);
    output
}

fn append(output: &mut String, value: &Value) {
    match value {
        Value::Null => output.push_str("null"),
        Value::Bool(true) => output.push_str("true"),
        Value::Bool(false) => output.push_str("false"),
        Value::Number(number) => output.push_str(&number.to_string()),
        Value::String(text) => append_string(output, text),
        Value::Array(items) => {
            output.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                append(output, item);
            }
            output.push(']');
        }
        Value::Object(entries) => {
            output.push('{');
            for (index, (key, item)) in entries.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                append_string(output, key);
                output.push(':');
                append(output, item);
            }
            output.push('}');
        }
    }
}

/// Go's `encodeState.string`: named escapes for common controls, `\u00XX` for
/// other control bytes, HTML-safe escapes for `<> &`, and `\u2028`/`\u2029`.
fn append_string(output: &mut String, text: &str) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    output.push('"');
    for character in text.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            '\u{08}' => output.push_str("\\b"),
            '\u{0c}' => output.push_str("\\f"),
            '<' => output.push_str("\\u003c"),
            '>' => output.push_str("\\u003e"),
            '&' => output.push_str("\\u0026"),
            '\u{2028}' => output.push_str("\\u2028"),
            '\u{2029}' => output.push_str("\\u2029"),
            character if character < '\u{20}' => {
                let code = u32::from(character);
                output.push_str("\\u00");
                output.push(HEX[((code >> 4) & 0xf) as usize] as char);
                output.push(HEX[(code & 0xf) as usize] as char);
            }
            character => output.push(character),
        }
    }
    output.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_sorted_and_html_characters_are_escaped() {
        let value: Value =
            serde_json::from_str(r#"{"z":"a<b&c>","a":1,"list":[true,null]}"#).expect("json");
        assert_eq!(
            marshal(&value),
            r#"{"a":1,"list":[true,null],"z":"a\u003cb\u0026c\u003e"}"#
        );
    }

    #[test]
    fn control_characters_use_go_escapes() {
        let value = Value::String("a\n\t\u{1}\u{2028}".to_string());
        assert_eq!(marshal(&value), "\"a\\n\\t\\u0001\\u2028\"");
    }
}
