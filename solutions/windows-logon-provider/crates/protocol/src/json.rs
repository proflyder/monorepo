//! Minimal flat-object JSON reader.
//!
//! Only string, integer and boolean values are allowed; nested objects,
//! arrays, floats, exponents, `null` and duplicate keys are rejected. Bodies
//! over 64 KiB are refused before parsing.

use std::collections::BTreeMap;

use crate::Error;

/// Bodies above this size are rejected before parsing.
pub(crate) const MAX_JSON_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum JsonValue {
    String(String),
    Number(i64),
    Boolean(bool),
}

pub(crate) type JsonObject = BTreeMap<String, JsonValue>;

/// Parses a flat JSON object. Keys may appear in any order and may repeat only
/// once; unknown keys are kept so callers can inspect them.
pub(crate) fn parse(body: &str) -> Result<JsonObject, Error> {
    Reader::new(body.as_bytes()).parse()
}

pub(crate) fn find_value<'a>(object: &'a JsonObject, key: &str) -> Option<&'a JsonValue> {
    object.get(key)
}

pub(crate) fn get_string<'a>(object: &'a JsonObject, key: &str) -> Option<&'a str> {
    match find_value(object, key) {
        Some(JsonValue::String(text)) => Some(text),
        _ => None,
    }
}

pub(crate) fn get_bool(object: &JsonObject, key: &str) -> Option<bool> {
    match find_value(object, key) {
        Some(JsonValue::Boolean(value)) => Some(*value),
        _ => None,
    }
}

fn is_json_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\r' | b'\n')
}

struct Reader<'a> {
    body: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(body: &'a [u8]) -> Self {
        Self { body, pos: 0 }
    }

    fn parse(mut self) -> Result<JsonObject, Error> {
        if self.body.len() > MAX_JSON_BYTES {
            return Err(Error::new("body too large"));
        }
        let mut out = JsonObject::new();
        self.skip_space();
        if !self.consume(b'{') {
            return Err(Error::new("expected object"));
        }
        self.skip_space();
        if !self.consume(b'}') {
            loop {
                let key = self.string_literal().map_err(|_| Error::new("bad key"))?;
                self.skip_space();
                if !self.consume(b':') {
                    return Err(Error::new("expected ':'"));
                }
                self.skip_space();
                let value = self.value_literal()?;
                if out.insert(key, value).is_some() {
                    return Err(Error::new("duplicate key"));
                }
                self.skip_space();
                if self.consume(b',') {
                    self.skip_space();
                    continue;
                }
                if self.consume(b'}') {
                    break;
                }
                return Err(Error::new("expected ',' or '}'"));
            }
        }
        self.skip_space();
        if self.pos != self.body.len() {
            return Err(Error::new("trailing data"));
        }
        Ok(out)
    }

    fn skip_space(&mut self) {
        while self.pos < self.body.len() && is_json_space(self.body[self.pos]) {
            self.pos += 1;
        }
    }

    fn consume(&mut self, expected: u8) -> bool {
        if self.pos < self.body.len() && self.body[self.pos] == expected {
            self.pos += 1;
            return true;
        }
        false
    }

    fn hex4(&mut self) -> Option<u32> {
        if self.pos + 4 > self.body.len() {
            return None;
        }
        let mut value = 0u32;
        for _ in 0..4 {
            let c = self.body[self.pos];
            value <<= 4;
            value |= match c {
                b'0'..=b'9' => u32::from(c - b'0'),
                b'a'..=b'f' => u32::from(c - b'a' + 10),
                b'A'..=b'F' => u32::from(c - b'A' + 10),
                _ => return None,
            };
            self.pos += 1;
        }
        Some(value)
    }

    fn string_literal(&mut self) -> Result<String, Error> {
        if !self.consume(b'"') {
            return Err(Error::new("expected string"));
        }
        let mut out: Vec<u8> = Vec::new();
        while self.pos < self.body.len() {
            let c = self.body[self.pos];
            if c == b'"' {
                self.pos += 1;
                return String::from_utf8(out).map_err(|_| Error::new("invalid utf-8 in string"));
            }
            if c < 0x20 {
                return Err(Error::new("control character in string"));
            }
            if c != b'\\' {
                out.push(c);
                self.pos += 1;
                continue;
            }
            self.pos += 1;
            if self.pos >= self.body.len() {
                return Err(Error::new("unterminated escape"));
            }
            let escape = self.body[self.pos];
            self.pos += 1;
            match escape {
                b'"' => out.push(b'"'),
                b'\\' => out.push(b'\\'),
                b'/' => out.push(b'/'),
                b'b' => out.push(0x08),
                b'f' => out.push(0x0c),
                b'n' => out.push(b'\n'),
                b'r' => out.push(b'\r'),
                b't' => out.push(b'\t'),
                b'u' => {
                    let unit = self.hex4().ok_or(Error::new("bad \\u escape"))?;
                    let code_point = if (0xd800..=0xdbff).contains(&unit) {
                        if self.pos + 2 > self.body.len()
                            || self.body[self.pos] != b'\\'
                            || self.body[self.pos + 1] != b'u'
                        {
                            return Err(Error::new("unpaired high surrogate"));
                        }
                        self.pos += 2;
                        let low = self.hex4().ok_or(Error::new("bad low surrogate"))?;
                        if !(0xdc00..=0xdfff).contains(&low) {
                            return Err(Error::new("bad low surrogate"));
                        }
                        0x1_0000 + ((unit - 0xd800) << 10) + (low - 0xdc00)
                    } else if (0xdc00..=0xdfff).contains(&unit) {
                        return Err(Error::new("unpaired low surrogate"));
                    } else {
                        unit
                    };
                    append_utf8(&mut out, code_point);
                }
                _ => return Err(Error::new("bad escape")),
            }
        }
        Err(Error::new("unterminated string"))
    }

    fn value_literal(&mut self) -> Result<JsonValue, Error> {
        if self.pos >= self.body.len() {
            return Err(Error::new("unsupported value"));
        }
        if self.body[self.pos] == b'"' {
            return self.string_literal().map(JsonValue::String);
        }
        if self.body[self.pos..].starts_with(b"true") {
            self.pos += 4;
            return Ok(JsonValue::Boolean(true));
        }
        if self.body[self.pos..].starts_with(b"false") {
            self.pos += 5;
            return Ok(JsonValue::Boolean(false));
        }
        let start = self.pos;
        if self.body[self.pos] == b'-' {
            self.pos += 1;
        }
        let digits = self.pos;
        while self.pos < self.body.len() && self.body[self.pos].is_ascii_digit() {
            self.pos += 1;
        }
        if self.pos == digits {
            self.pos = start;
            return Err(Error::new("unsupported value"));
        }
        if self.pos - digits > 1 && self.body[digits] == b'0' {
            return Err(Error::new("unsupported value"));
        }
        // Keeps every value well inside i64.
        if self.pos - digits > 18 {
            return Err(Error::new("unsupported value"));
        }
        let mut number: i64 = 0;
        let negative = self.body[start] == b'-';
        for &digit in &self.body[digits..self.pos] {
            number = number * 10 + i64::from(digit - b'0');
        }
        if negative {
            number = -number;
        }
        Ok(JsonValue::Number(number))
    }
}

fn append_utf8(out: &mut Vec<u8>, code_point: u32) {
    match char::from_u32(code_point) {
        Some(c) => {
            let mut buffer = [0u8; 4];
            out.extend_from_slice(c.encode_utf8(&mut buffer).as_bytes());
        }
        // Surrogates were rejected before this point, so the code point is
        // always a scalar value. Keeping the fallback non-panicking matches
        // the helper, which never rejects an already-validated scalar.
        None => out.push(b'?'),
    }
}
