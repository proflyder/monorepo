//! Redaction of pairing secrets in log lines. The sendspin-cpp host log writes directly to
//! stderr (`fprintf`). Only the Bazel station build compiles sendspin-cpp, and it does so with
//! `-Dfprintf=pulsar_filtered_fprintf`, so the shim passes every line through [`redact`]. The
//! legacy `build.rs` path links prebuilt sendspin libraries made elsewhere and does not filter
//! them; a cargo host build without sendspin-cpp has no library log at all.

use std::borrow::Cow;

/// Marker for a removed value.
const HIDDEN: &str = "<redacted>";

/// Masks `SP:` pairing tokens and the value of a `… Pairing PSK: <value>` line.
pub fn redact(line: &str) -> Cow<'_, str> {
    let without_token = redact_tokens(line);
    redact_psk(without_token)
}

fn redact_psk(line: Cow<'_, str>) -> Cow<'_, str> {
    let Some(pski) = line.find("Pairing PSK") else {
        return line;
    };
    let mut start = pski + "Pairing PSK".len();
    let bytes = line.as_bytes();
    while start < bytes.len() && bytes[start].is_ascii_whitespace() {
        start += 1;
    }
    if start < bytes.len() && bytes[start] == b':' {
        start += 1;
        while start < bytes.len() && bytes[start].is_ascii_whitespace() {
            start += 1;
        }
    }
    let end = line[start..]
        .find(|c: char| c.is_ascii_whitespace())
        .map_or(line.len(), |i| start + i);
    if start >= end {
        return line;
    }
    let mut out = String::with_capacity(line.len() - (end - start) + HIDDEN.len());
    out.push_str(&line[..start]);
    out.push_str(HIDDEN);
    out.push_str(&line[end..]);
    Cow::Owned(out)
}

fn redact_tokens(line: &str) -> Cow<'_, str> {
    let mut out = String::new();
    let mut rest = line;
    let mut changed = false;
    while let Some(i) = rest.find("SP:") {
        let (before, token) = rest.split_at(i);
        out.push_str(before);
        let body = &token[3..];
        let len = body
            .bytes()
            .take_while(|b| b.is_ascii_uppercase() || (b'2'..=b'7').contains(b))
            .count();
        if len == 0 {
            out.push_str("SP:");
            rest = &token[3..];
            continue;
        }
        // Short tokens are hidden entirely; long ones keep a recognisable head and tail.
        if len <= 12 {
            out.push_str("SP:");
            out.push_str(HIDDEN);
        } else {
            out.push_str("SP:");
            out.push_str(&body[..4]);
            out.push('…');
            out.push_str(&body[len - 4..len]);
        }
        changed = true;
        rest = &body[len..];
    }
    if !changed {
        return Cow::Borrowed(line);
    }
    out.push_str(rest);
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provisioning_line_drops_the_value() {
        let line = "I sendspin.record_store: Provisioned Sendspin Pairing PSK: 3f9c0a17b2e44d8f\n";
        let out = redact(line);
        assert!(!out.contains("3f9c0a17"), "{out}");
        assert!(
            out.contains("Provisioned Sendspin Pairing PSK: <redacted>"),
            "{out}"
        );
    }

    #[test]
    fn a_failed_provisioning_warning_is_redacted_too() {
        let line = "W sendspin.record_store: Provisioned Sendspin Pairing PSK 3f9c0a17 failed; a token will not survive\n";
        let out = redact(line);
        assert!(!out.contains("3f9c0a17"), "{out}");
        assert!(out.contains("<redacted> failed"), "{out}");
    }

    #[test]
    fn pairing_tokens_are_masked() {
        let line = "sendspin: pairing token SP:ABCDEFGHIJKLMNOPQRSTUVWXYZ234567\n";
        let out = redact(line);
        assert!(!out.contains("ABCDEFGHIJKLMNOP"), "{out}");
        assert!(out.contains("SP:ABCD…4567"), "{out}");
        // A short token gives away nothing.
        assert_eq!(redact("token SP:ABCDEF").as_ref(), "token SP:<redacted>");
        // Ordinary lines pass unchanged.
        let plain = "sendspin: stream start flac 48000 Hz/2 ch/16 bit";
        assert_eq!(redact(plain), plain);
    }

    #[test]
    fn every_occurrence_is_redacted() {
        let out = redact("SP:ABCDEFGHIJKLMNOP and SP:2345672345672345");
        assert!(!out.contains("ABCDEFGHIJKLMNOP"), "{out}");
        assert!(!out.contains("234567234567"), "{out}");
        assert!(out.contains("SP:ABCD…MNOP"), "{out}");
        assert!(out.contains("SP:2345…2345"), "{out}");
    }
}
