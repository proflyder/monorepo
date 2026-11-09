//! Builders for `home.v1.LocalizedText`.
//!
//! The server sends stable snake_case keys, never a user-visible sentence.
//! Clients resolve the key through their own resources and fall back to the
//! legacy text field when they do not know it.

use connectrpc::ErrorDetail;
use serde_json::{Map, Value};

use crate::v1::LocalizedText;

/// A localized text with no arguments.
#[must_use]
pub fn localized(key: &str) -> LocalizedText {
    LocalizedText {
        key: key.to_owned(),
        ..LocalizedText::default()
    }
}

/// A localized text with `(name, value)` arguments.
#[must_use]
pub fn localized_with<I, K, V>(key: &str, args: I) -> LocalizedText
where
    I: IntoIterator<Item = (K, V)>,
    K: Into<String>,
    V: Into<String>,
{
    LocalizedText {
        key: key.to_owned(),
        args: args
            .into_iter()
            .map(|(name, value)| (name.into(), value.into()))
            .collect(),
        ..LocalizedText::default()
    }
}

/// The `home.v1.LocalizedText` error detail for a Connect error.
///
/// The detail carries a JSON `debug` representation so a JSON client (the
/// Android app) can read the key and arguments without decoding protobuf.
/// The legacy `message` stays on the error for older clients.
#[must_use]
pub fn error_detail(text: &LocalizedText) -> ErrorDetail {
    let mut debug = Map::new();
    debug.insert("key".to_owned(), Value::String(text.key.clone()));
    if !text.args.is_empty() {
        let args: Map<String, Value> = text
            .args
            .iter()
            .map(|(name, value)| (name.clone(), Value::String(value.clone())))
            .collect();
        debug.insert("args".to_owned(), Value::Object(args));
    }
    ErrorDetail {
        type_url: "home.v1.LocalizedText".to_owned(),
        value: None,
        debug: Some(Value::Object(debug)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_are_strings_and_empty_args_are_omitted() {
        let text = localized("scheduler.event.snoozed");
        assert_eq!(text.key, "scheduler.event.snoozed");
        assert!(text.args.is_empty());
        let detail = error_detail(&text);
        assert_eq!(detail.type_url, "home.v1.LocalizedText");
        assert_eq!(detail.value, None);
        let debug = detail.debug.expect("debug");
        assert_eq!(debug["key"], "scheduler.event.snoozed");
        assert!(debug.get("args").is_none());

        let text = localized_with("scheduler.event.snoozed", [("minutes", "9")]);
        let detail = error_detail(&text);
        assert_eq!(detail.debug.expect("debug")["args"]["minutes"], "9");
    }
}
