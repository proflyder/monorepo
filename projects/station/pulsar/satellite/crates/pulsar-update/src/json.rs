//! Minimal JSON reader for the two small documents the GHCR path returns: the token response and
//! the OCI manifest. Only what those need; no dependency of its own.

#[derive(Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Self>),
    Object(Vec<(String, Self)>),
}

impl Json {
    pub(crate) fn get(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Object(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub(crate) fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    pub(crate) fn as_array(&self) -> Option<&[Self]> {
        match self {
            Self::Array(items) => Some(items),
            _ => None,
        }
    }
}

/// Nested documents from a registry are small; the limit keeps a bad one from recursing deeply.
const MAX_DEPTH: u32 = 32;

/// The whole input must be one JSON value (surrounding whitespace allowed).
pub fn parse(input: &[u8]) -> Option<Json> {
    let mut p = Parser { s: input, i: 0 };
    p.ws();
    let v = p.value(0)?;
    p.ws();
    (p.i == input.len()).then_some(v)
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while matches!(self.s.get(self.i), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
    }

    fn byte(&mut self) -> Option<u8> {
        let b = *self.s.get(self.i)?;
        self.i += 1;
        Some(b)
    }

    fn eat(&mut self, b: u8) -> bool {
        if self.s.get(self.i) == Some(&b) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn value(&mut self, depth: u32) -> Option<Json> {
        if depth > MAX_DEPTH {
            return None;
        }
        match *self.s.get(self.i)? {
            b'n' => self.literal(b"null", Json::Null),
            b't' => self.literal(b"true", Json::Bool(true)),
            b'f' => self.literal(b"false", Json::Bool(false)),
            b'"' => Some(Json::String(self.string()?)),
            b'[' => self.array(depth),
            b'{' => self.object(depth),
            b'-' | b'0'..=b'9' => self.number(),
            _ => None,
        }
    }

    fn literal(&mut self, text: &[u8], v: Json) -> Option<Json> {
        if self.s[self.i..].starts_with(text) {
            self.i += text.len();
            Some(v)
        } else {
            None
        }
    }

    fn digits(&mut self) -> usize {
        let start = self.i;
        while self.s.get(self.i).is_some_and(u8::is_ascii_digit) {
            self.i += 1;
        }
        self.i - start
    }

    fn number(&mut self) -> Option<Json> {
        let start = self.i;
        self.eat(b'-');
        if self.digits() == 0 {
            return None;
        }
        if self.eat(b'.') && self.digits() == 0 {
            return None;
        }
        if matches!(self.s.get(self.i), Some(b'e' | b'E')) {
            self.i += 1;
            if matches!(self.s.get(self.i), Some(b'+' | b'-')) {
                self.i += 1;
            }
            if self.digits() == 0 {
                return None;
            }
        }
        let text = std::str::from_utf8(&self.s[start..self.i]).ok()?;
        text.parse().ok().map(Json::Number)
    }

    fn array(&mut self, depth: u32) -> Option<Json> {
        self.eat(b'[');
        let mut items = Vec::new();
        self.ws();
        if self.eat(b']') {
            return Some(Json::Array(items));
        }
        loop {
            items.push(self.value(depth + 1)?);
            self.ws();
            if self.eat(b']') {
                return Some(Json::Array(items));
            }
            if !self.eat(b',') {
                return None;
            }
            self.ws();
        }
    }

    fn object(&mut self, depth: u32) -> Option<Json> {
        self.eat(b'{');
        let mut fields = Vec::new();
        self.ws();
        if self.eat(b'}') {
            return Some(Json::Object(fields));
        }
        loop {
            let key = self.string()?;
            self.ws();
            if !self.eat(b':') {
                return None;
            }
            self.ws();
            fields.push((key, self.value(depth + 1)?));
            self.ws();
            if self.eat(b'}') {
                return Some(Json::Object(fields));
            }
            if !self.eat(b',') {
                return None;
            }
            self.ws();
        }
    }

    fn string(&mut self) -> Option<String> {
        if !self.eat(b'"') {
            return None;
        }
        let mut out = Vec::new();
        loop {
            match *self.s.get(self.i)? {
                b'"' => {
                    self.i += 1;
                    return String::from_utf8(out).ok();
                }
                b'\\' => {
                    self.i += 1;
                    self.escape(&mut out)?;
                }
                b if b < 0x20 => return None,
                _ => {
                    let start = self.i;
                    while let Some(&b) = self.s.get(self.i) {
                        if b == b'"' || b == b'\\' || b < 0x20 {
                            break;
                        }
                        self.i += 1;
                    }
                    out.extend_from_slice(&self.s[start..self.i]);
                }
            }
        }
    }

    fn escape(&mut self, out: &mut Vec<u8>) -> Option<()> {
        match self.byte()? {
            b'"' => out.push(b'"'),
            b'\\' => out.push(b'\\'),
            b'/' => out.push(b'/'),
            b'b' => out.push(0x08),
            b'f' => out.push(0x0c),
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b't' => out.push(b'\t'),
            b'u' => {
                let c = self.unicode()?;
                let mut buf = [0; 4];
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
            _ => return None,
        }
        Some(())
    }

    fn unicode(&mut self) -> Option<char> {
        let hi = self.hex4()?;
        let code = if (0xd800..0xdc00).contains(&hi) {
            if self.byte()? != b'\\' || self.byte()? != b'u' {
                return None;
            }
            let lo = self.hex4()?;
            if !(0xdc00..0xe000).contains(&lo) {
                return None;
            }
            0x10000 + ((hi - 0xd800) << 10) + (lo - 0xdc00)
        } else if (0xdc00..0xe000).contains(&hi) {
            return None;
        } else {
            hi
        };
        char::from_u32(code)
    }

    fn hex4(&mut self) -> Option<u32> {
        let mut n = 0;
        for _ in 0..4 {
            n = n * 16 + char::from(self.byte()?).to_digit(16)?;
        }
        Some(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_documents_of_the_registry() {
        let token = parse(br#"{"token":"abc.DEF-_="}"#).expect("token");
        assert_eq!(
            token.get("token").and_then(Json::as_str),
            Some("abc.DEF-_=")
        );

        let manifest = parse(
            br#"{
                "schemaVersion": 2,
                "layers": [
                    {"digest": "sha256:aa", "annotations": {"org.opencontainers.image.title": "a.bin"}},
                    {"digest": "sha256:bb", "annotations": {"org.opencontainers.image.title": "SHA256SUMS"}}
                ],
                "annotations": {"org.opencontainers.image.version": "v1.2.3"}
            }"#,
        )
        .expect("manifest");
        assert_eq!(
            manifest
                .get("annotations")
                .and_then(|a| a.get("org.opencontainers.image.version"))
                .and_then(Json::as_str),
            Some("v1.2.3")
        );
        assert_eq!(
            manifest
                .get("layers")
                .and_then(Json::as_array)
                .map(<[_]>::len),
            Some(2)
        );
    }

    #[test]
    fn escapes_and_rejects_broken_input() {
        let v = parse(br#""a\"b\\c\/d\n\u00e9\ud83d\ude00""#).expect("string");
        assert_eq!(v.as_str(), Some("a\"b\\c/d\né😀"));
        for bad in [
            &b""[..],
            b"{",
            b"{\"a\":}",
            b"[1,]",
            b"nul",
            b"\"unterminated",
            b"\"\\ud800\"",
            b"1 2",
        ] {
            assert_eq!(parse(bad), None, "{:?}", String::from_utf8_lossy(bad));
        }
        assert!(parse(&b"[".repeat(64)).is_none());
    }
}
