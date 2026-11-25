use axum::http::HeaderMap;

use super::MAX_KEY_CHARS;

pub(super) fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key.chars().count() <= MAX_KEY_CHARS
        && key.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | '+' | '@')
        })
}

pub(super) fn audit_key(key: &str) -> &str {
    if valid_key(key) { key } else { "<invalid>" }
}

pub(super) fn valid_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(super) fn sha256_hex(value: &str) -> String {
    use sha2::Digest as _;
    hex::encode(sha2::Sha256::digest(value.as_bytes()))
}

pub(super) fn header_value<'a>(headers: &'a HeaderMap, name: &str) -> &'a str {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_bounded_and_ascii_safe() {
        assert!(valid_key("service_token"));
        assert!(valid_key("vpn.json"));
        assert!(!valid_key(""));
        assert!(!valid_key("with space"));
        assert!(!valid_key("with/slash"));
        assert!(!valid_key(&"a".repeat(MAX_KEY_CHARS + 1)));
    }

    #[test]
    fn hashes_are_sha256_hex() {
        assert!(valid_hash(&sha256_hex("value")));
        assert!(!valid_hash("abc"));
    }
}
