//! Request bodies for `POST /v1/challenge` and `POST /v1/claim`.

use std::fmt::Write as _;

use crate::purpose::Purpose;

/// Escapes a string for embedding in a JSON string literal. Only the
/// characters the protocol can produce are escaped; control bytes become
/// lowercase `\u00XX`, byte for byte as documented.
fn json_escape(text: &str) -> String {
    let mut out: Vec<u8> = Vec::with_capacity(text.len() + 2);
    for &c in text.as_bytes() {
        match c {
            b'"' => out.extend_from_slice(b"\\\""),
            b'\\' => out.extend_from_slice(b"\\\\"),
            0x08 => out.extend_from_slice(b"\\b"),
            0x0c => out.extend_from_slice(b"\\f"),
            b'\n' => out.extend_from_slice(b"\\n"),
            b'\r' => out.extend_from_slice(b"\\r"),
            b'\t' => out.extend_from_slice(b"\\t"),
            c if c < 0x20 => out.extend_from_slice(format!("\\u{c:04x}").as_bytes()),
            c => out.push(c),
        }
    }
    // The input was UTF-8 and the replacements are ASCII, so this cannot fail.
    String::from_utf8(out).expect("escaped request body is valid utf-8")
}

pub fn challenge_request_json(machine: &str) -> String {
    format!("{{\"machine\":\"{}\"}}", json_escape(machine))
}

/// v1 claim body, no `purpose` and no long-poll.
pub fn claim_request_json(machine: &str, nonce: &str) -> String {
    format!(
        "{{\"machine\":\"{}\",\"nonce\":\"{}\",\"mac\":\"\"}}",
        json_escape(machine),
        json_escape(nonce)
    )
}

/// Purpose-aware claim body. `wait_seconds` is `Some` only for the unlock
/// long-poll; the protocol allows it exclusively together with `purpose`.
pub fn claim_request_json_for_purpose(
    machine: &str,
    nonce: &str,
    mac: &str,
    purpose: Purpose,
    wait_seconds: Option<u32>,
) -> String {
    let mut body = format!(
        "{{\"machine\":\"{}\",\"nonce\":\"{}\",\"purpose\":\"{}\",\"mac\":\"{}\"",
        json_escape(machine),
        json_escape(nonce),
        purpose.as_str(),
        json_escape(mac)
    );
    if let Some(wait_seconds) = wait_seconds {
        let _ = write!(body, ",\"waitSeconds\":{wait_seconds}");
    }
    body.push('}');
    body
}
