use std::time::{SystemTime, UNIX_EPOCH};

use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;
use subtle::ConstantTimeEq;
use thiserror::Error;

pub const PERIOD_SECONDS: u64 = 30;
pub const DIGITS: usize = 6;
const SKEW_STEPS: [i64; 3] = [-1, 0, 1];
const MIN_SECRET_BYTES: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum Error {
    #[error("TOTP secret is not base32")]
    Secret,
}

#[derive(Clone)]
pub struct Totp {
    key: Vec<u8>,
}

impl Totp {
    pub fn new(base32: &str) -> Result<Self, Error> {
        decode_base32(base32)
            .map(|key| Self { key })
            .ok_or(Error::Secret)
    }

    #[must_use]
    pub fn code(&self, unix: u64) -> String {
        format_code(hotp(&self.key, unix / PERIOD_SECONDS))
    }

    #[must_use]
    pub fn verify(&self, candidate: &str, unix: u64) -> Option<u64> {
        let candidate = candidate.trim();
        if candidate.len() != DIGITS || !candidate.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        let counter = i64::try_from(unix / PERIOD_SECONDS).ok()?;
        for offset in SKEW_STEPS {
            let Some(step) = counter.checked_add(offset).filter(|step| *step >= 0) else {
                continue;
            };
            let step = u64::try_from(step).ok()?;
            if format_code(hotp(&self.key, step))
                .as_bytes()
                .ct_eq(candidate.as_bytes())
                .into()
            {
                return Some(step);
            }
        }
        None
    }
}

#[must_use]
pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn hotp(key: &[u8], counter: u64) -> u32 {
    let mut mac = Hmac::<Sha1>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(&counter.to_be_bytes());
    let digest = mac.finalize().into_bytes();
    let offset = usize::from(digest[19] & 0x0f);
    let value = (u32::from(digest[offset] & 0x7f) << 24)
        | (u32::from(digest[offset + 1]) << 16)
        | (u32::from(digest[offset + 2]) << 8)
        | u32::from(digest[offset + 3]);
    value % 1_000_000
}

fn format_code(value: u32) -> String {
    format!("{value:06}")
}

fn decode_base32(value: &str) -> Option<Vec<u8>> {
    let mut buffer = 0u32;
    let mut bits = 0u32;
    let mut output = Vec::new();
    for byte in value.bytes() {
        if byte == b' ' || byte == b'-' || byte == b'\t' {
            continue;
        }
        if byte == b'=' {
            break;
        }
        let digit = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a',
            b'2'..=b'7' => byte - b'2' + 26,
            _ => return None,
        };
        buffer = (buffer << 5) | u32::from(digit);
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            output.push(u8::try_from((buffer >> bits) & 0xff).ok()?);
        }
    }
    if output.len() < MIN_SECRET_BYTES {
        return None;
    }
    Some(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RFC_SECRET: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    #[test]
    fn the_rfc_6238_vector_is_truncated_to_six_digits() {
        let totp = Totp::new(RFC_SECRET).expect("totp");
        assert_eq!(totp.code(59), "287082");
        assert_eq!(totp.code(1_111_111_109), "081804");
        assert_eq!(totp.verify("287082", 59), Some(1));
    }

    #[test]
    fn one_step_of_clock_skew_is_accepted() {
        let totp = Totp::new(RFC_SECRET).expect("totp");
        let code = totp.code(30);
        assert_eq!(totp.verify(&code, 0), Some(1));
        assert_eq!(totp.verify(&code, 30), Some(1));
        assert_eq!(totp.verify(&code, 59), Some(1));
        assert!(totp.verify(&code, 90).is_none());
    }

    #[test]
    fn malformed_codes_secrets_and_whitespace_are_handled() {
        let totp = Totp::new("gezd gnbv gy3t qojq gezd gnbv gy3t qojq").expect("totp");
        assert_eq!(totp.code(59), "287082");
        assert!(totp.verify("28708", 59).is_none());
        assert!(totp.verify("abcdef", 59).is_none());
        assert!(totp.verify("", 59).is_none());
        assert!(Totp::new("not-base32!").is_err());
        assert!(Totp::new("").is_err());
        assert!(Totp::new("AAAA").is_err());
    }
}
