//! Standard base64 with padding.
//!
//! This is a line-by-line port of the C++ `b64_encode`/`b64_decode`. The
//! decoder is deliberately hand-written instead of using the `base64` crate:
//! the C++ reference accepts non-canonical trailing bits (`"ZZ=="` decodes to
//! `0x65`), while `base64`'s `STANDARD` engine rejects them. Keeping the port
//! verbatim keeps the wire behaviour identical and testable with the same
//! vectors. The decoder rejects non-canonical padding, embedded whitespace and
//! any character outside the alphabet.

use crate::Error;

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding.
pub fn b64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    let (chunks, rest) = data.as_chunks::<3>();
    for chunk in chunks {
        let value = (u32::from(chunk[0]) << 16) | (u32::from(chunk[1]) << 8) | u32::from(chunk[2]);
        out.push(char::from(ALPHABET[((value >> 18) & 0x3f) as usize]));
        out.push(char::from(ALPHABET[((value >> 12) & 0x3f) as usize]));
        out.push(char::from(ALPHABET[((value >> 6) & 0x3f) as usize]));
        out.push(char::from(ALPHABET[(value & 0x3f) as usize]));
    }
    match rest {
        [] => {}
        [a] => {
            let value = u32::from(*a) << 16;
            out.push(char::from(ALPHABET[((value >> 18) & 0x3f) as usize]));
            out.push(char::from(ALPHABET[((value >> 12) & 0x3f) as usize]));
            out.push('=');
            out.push('=');
        }
        [a, b] => {
            let value = (u32::from(*a) << 16) | (u32::from(*b) << 8);
            out.push(char::from(ALPHABET[((value >> 18) & 0x3f) as usize]));
            out.push(char::from(ALPHABET[((value >> 12) & 0x3f) as usize]));
            out.push(char::from(ALPHABET[((value >> 6) & 0x3f) as usize]));
            out.push('=');
        }
        _ => unreachable!("as_chunks(3) remainder is at most two bytes"),
    }
    out
}

/// Strict decoder. Returns an error for wrong length, misplaced or excessive
/// padding, whitespace and any character outside the alphabet.
pub fn b64_decode(text: &str) -> Result<Vec<u8>, Error> {
    let text = text.as_bytes();
    let mut out = Vec::with_capacity((text.len() / 4) * 3);
    if text.is_empty() {
        return Ok(out);
    }
    if !text.len().is_multiple_of(4) {
        return Err(Error::new("base64 length is not a multiple of four"));
    }
    let mut padding = 0;
    if text[text.len() - 1] == b'=' {
        padding += 1;
    }
    if text.len() >= 2 && text[text.len() - 2] == b'=' {
        padding += 1;
    }
    for (block, chunk) in text.as_chunks::<4>().0.iter().enumerate() {
        let base = block * 4;
        let mut values = [0u8; 4];
        for (j, slot) in values.iter_mut().enumerate() {
            let byte = chunk[j];
            let padding_position = padding > 0 && base + j >= text.len() - padding;
            if byte == b'=' {
                if !padding_position {
                    return Err(Error::new("base64 padding is misplaced"));
                }
                continue;
            }
            match b64_value(byte) {
                Some(value) => *slot = value,
                None => {
                    return Err(Error::new(
                        "base64 contains a character outside the alphabet",
                    ));
                }
            }
        }
        out.push((values[0] << 2) | (values[1] >> 4));
        if chunk[2] != b'=' {
            out.push(((values[1] & 0x0f) << 4) | (values[2] >> 2));
        }
        if chunk[3] != b'=' {
            out.push(((values[2] & 0x03) << 6) | values[3]);
        }
    }
    Ok(out)
}

fn b64_value(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}
