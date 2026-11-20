use crate::Claim;

const MAGIC: &[u8] = b"pulsar-wake/1";
const MAX_DATAGRAM: usize = 127;
const MAGIC_FIELD: usize = 15;
const STATION_FIELD: usize = 31;
const WAKE_FIELD: usize = 15;

pub fn level_text(level: f32) -> String {
    if level.is_nan() {
        let sign = if level.is_sign_negative() { "-" } else { "" };
        return format!("{sign}nan");
    }
    format!("{level:.1}")
}

pub fn format(c: &Claim) -> Option<String> {
    let d = format!(
        "pulsar-wake/1 {} {} {} {}",
        c.station,
        c.wake,
        u8::from(c.busy),
        level_text(c.level_db)
    );
    (d.len() <= MAX_DATAGRAM).then_some(d)
}

pub fn parse(datagram: &[u8]) -> Option<Claim> {
    let end = datagram
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(datagram.len());
    let mut s = Scan {
        s: &datagram[..end],
        pos: 0,
    };
    let magic = s.word(MAGIC_FIELD)?;
    let station = s.word(STATION_FIELD)?;
    let wake = s.word(WAKE_FIELD)?;
    let busy = s.int_nonzero()?;
    let level = s.float()?;
    if !level.is_finite() || !(-100.0..=0.0).contains(&level) || magic != MAGIC {
        return None;
    }
    Some(Claim {
        station: String::from_utf8_lossy(station).into_owned(),
        wake: String::from_utf8_lossy(wake).into_owned(),
        busy,
        level_db: level,
    })
}

fn is_space(b: u8) -> bool {
    b == b' ' || (b'\t'..=b'\r').contains(&b)
}

struct Scan<'a> {
    s: &'a [u8],
    pos: usize,
}

impl<'a> Scan<'a> {
    fn skip_space(&mut self) {
        while self.s.get(self.pos).is_some_and(|&b| is_space(b)) {
            self.pos += 1;
        }
    }

    fn take_while(&mut self, f: impl Fn(u8) -> bool) -> usize {
        let start = self.pos;
        while self.s.get(self.pos).is_some_and(|&b| f(b)) {
            self.pos += 1;
        }
        self.pos - start
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.pos).copied()
    }

    fn word(&mut self, width: usize) -> Option<&'a [u8]> {
        self.skip_space();
        let start = self.pos;
        while self.pos - start < width && self.peek().is_some_and(|b| !is_space(b)) {
            self.pos += 1;
        }
        (self.pos > start).then(|| &self.s[start..self.pos])
    }

    fn int_nonzero(&mut self) -> Option<bool> {
        self.skip_space();
        if matches!(self.peek(), Some(b'+' | b'-')) {
            self.pos += 1;
        }
        let start = self.pos;
        let digits = self.take_while(|b| b.is_ascii_digit());
        (digits > 0).then(|| self.s[start..self.pos].iter().any(|&b| b != b'0'))
    }

    fn float(&mut self) -> Option<f32> {
        self.skip_space();
        let start = self.pos;
        if matches!(self.peek(), Some(b'+' | b'-')) {
            self.pos += 1;
        }
        let int_digits = self.take_while(|b| b.is_ascii_digit());
        if int_digits == 1
            && self.s[self.pos - 1] == b'0'
            && matches!(self.peek(), Some(b'x' | b'X'))
        {
            return None;
        }
        let frac_digits = if self.peek() == Some(b'.') {
            self.pos += 1;
            self.take_while(|b| b.is_ascii_digit())
        } else {
            0
        };
        if int_digits + frac_digits == 0 {
            return None;
        }
        let mantissa_end = self.pos;
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if self.take_while(|b| b.is_ascii_digit()) == 0 {
                self.pos = mantissa_end;
            }
        }
        std::str::from_utf8(&self.s[start..self.pos])
            .ok()?
            .parse()
            .ok()
    }
}
