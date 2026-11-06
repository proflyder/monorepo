//! Фрейминг Native API. Первый байт клиента выбирает режим: 0x00 — plaintext
//! (`[0x00][varint len][varint type][message]`), 0x01 — Noise.
//!
//! Noise-транспорт ESPHome: кадры `[0x01][len BE16][payload]`. Первый кадр клиента —
//! «client hello» (содержимое зарезервировано, привязывается к handshake через prologue),
//! сервер отвечает server hello, затем один обмен NNpsk0, кадры которого начинаются со
//! status-байта (0 = ok, 1 = отказ + текст причины). Data-кадры несут зашифрованный
//! `[type BE16][len BE16][message]`.

use std::ops::Range;

use prost::encoding::{decode_varint, encode_varint};
use snow::{HandshakeState, TransportState};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Слишком большие plaintext-кадры бессмысленны: клиент отбрасывается.
pub const MAX_FRAME: usize = 1 << 20;
pub const NOISE_PROTOCOL: &str = "Noise_NNpsk0_25519_ChaChaPoly_SHA256";
const NOISE_MAC: usize = 16;
const NOISE_MAX_FRAME: usize = 0xffff;
/// Hello и handshake-кадры короткие (NNpsk0: сообщения по 48 байт); более длинный отвергается.
const HANDSHAKE_MAX: usize = 128;
const DH_LEN: usize = 32;
/// «varint len + varint type» занимает до 20 байт после индикатора; заголовок, не разобранный
/// за них, не разберётся уже никогда.
const PLAIN_HEADER_MAX: usize = 21;

/// Предварительный ключ Noise: 32 байта из `api_key`, обнуляются при drop.
#[derive(Clone, Default, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub struct Psk([u8; 32]);

impl Psk {
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn is_zero(&self) -> bool {
        self.0.iter().all(|&b| b == 0)
    }
}

impl std::fmt::Debug for Psk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Psk(***)")
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Plain<'a> {
    /// Ждать ещё байт.
    Incomplete,
    BadPreamble,
    MalformedHeader,
    TooLarge,
    Frame {
        msg_type: u32,
        msg: &'a [u8],
        used: usize,
    },
}

/// Разбирает один plaintext-кадр в начале `p` (непустой).
pub fn parse_plain(p: &[u8]) -> Plain<'_> {
    if p[0] != 0x00 {
        return Plain::BadPreamble;
    }
    let mut q = 1;
    let (Some(len), Some(msg_type)) = (varint(p, &mut q), varint(p, &mut q)) else {
        return if p.len() > PLAIN_HEADER_MAX {
            Plain::MalformedHeader
        } else {
            Plain::Incomplete
        };
    };
    let Some(len) = usize::try_from(len).ok().filter(|&l| l <= MAX_FRAME) else {
        return Plain::TooLarge;
    };
    if p.len() - q < len {
        return Plain::Incomplete;
    }
    Plain::Frame {
        msg_type: msg_type as u32,
        msg: &p[q..q + len],
        used: q + len,
    }
}

pub fn append_plain(out: &mut Vec<u8>, msg_type: u32, msg: &[u8]) {
    out.push(0x00);
    encode_varint(msg.len() as u64, out);
    encode_varint(u64::from(msg_type), out);
    out.extend_from_slice(msg);
}

/// Декодирует один varint в `p[*pos..]`, сдвигая `pos`. `None`, если обрезан или длиннее 10
/// байт (у декодера prost те же ограничения).
fn varint(p: &[u8], pos: &mut usize) -> Option<u64> {
    if *pos >= p.len() {
        return None;
    }
    let mut slice = &p[*pos..];
    let before = slice.len();
    let v = decode_varint(&mut slice).ok()?;
    *pos += before - slice.len();
    Some(v)
}

#[derive(Debug, PartialEq, Eq)]
pub enum NoiseFrame<'a> {
    Incomplete,
    BadIndicator,
    BadHandshakeLen,
    Frame { body: &'a [u8], used: usize },
}

/// Разбирает один Noise-кадр в начале `p` (непустой).
pub fn parse_noise(p: &[u8], in_handshake: bool) -> NoiseFrame<'_> {
    if p[0] != 0x01 {
        return NoiseFrame::BadIndicator;
    }
    if p.len() < 3 {
        return NoiseFrame::Incomplete;
    }
    let len = usize::from(u16::from_be_bytes([p[1], p[2]]));
    if in_handshake && len > HANDSHAKE_MAX {
        return NoiseFrame::BadHandshakeLen;
    }
    if p.len() - 3 < len {
        return NoiseFrame::Incomplete;
    }
    NoiseFrame::Frame {
        body: &p[3..3 + len],
        used: 3 + len,
    }
}

/// Кадр вне шифра: hello, сообщения handshake, отказы.
pub fn append_noise_raw(out: &mut Vec<u8>, payload: &[&[u8]]) {
    let len: usize = payload.iter().map(|p| p.len()).sum();
    out.push(0x01);
    out.extend_from_slice(&(len as u16).to_be_bytes());
    for p in payload {
        out.extend_from_slice(p);
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum HandshakeError {
    /// Отказать с этим текстом.
    Reject(&'static str),
    /// Handshake-сообщение клиента не проверилось: неверный ключ (`mac`) или ошибка протокола.
    ReadFailed {
        mac: bool,
    },
    SplitFailed,
}

#[derive(Debug, PartialEq, Eq)]
pub enum EncryptError {
    /// Handshake не завершён: сообщение отбрасывается.
    NotReady,
    TooLarge,
    Failed,
}

/// Сторона-респондер одного соединения.
pub enum Noise {
    ClientHello,
    Handshake(Box<HandshakeState>),
    Data(Box<TransportState>),
}

impl Noise {
    pub fn in_handshake(&self) -> bool {
        !matches!(self, Self::Data(_))
    }

    pub fn is_client_hello(&self) -> bool {
        matches!(self, Self::ClientHello)
    }

    /// Client hello: ставит в очередь server hello (выбранный протокол 0x01, имя узла и MAC,
    /// которые aioesphomeapi сверяет с ожиданиями HA до handshake) и готовит NNpsk0-респондер
    /// с prologue «NoiseAPIInit» + BE16-длина + client hello.
    pub fn start(
        &mut self,
        client_hello: &[u8],
        psk: &Psk,
        name: &str,
        mac: &str,
        out: &mut Vec<u8>,
    ) -> Result<(), snow::Error> {
        append_noise_raw(
            out,
            &[b"\x01", name.as_bytes(), b"\0", mac.as_bytes(), b"\0"],
        );
        let mut prologue = Vec::with_capacity(14 + client_hello.len());
        prologue.extend_from_slice(b"NoiseAPIInit");
        prologue.extend_from_slice(&(client_hello.len() as u16).to_be_bytes());
        prologue.extend_from_slice(client_hello);
        let hs = snow::Builder::new(NOISE_PROTOCOL.parse()?)
            .psk(0, psk.as_bytes())?
            .prologue(&prologue)?
            .build_responder()?;
        *self = Self::Handshake(Box::new(hs));
        Ok(())
    }

    /// Сообщение инициатора `psk, e` (после его status-байта 0x00); ставит в очередь наш `e, ee`
    /// и переходит в transport-режим.
    pub fn handshake(&mut self, body: &[u8], out: &mut Vec<u8>) -> Result<(), HandshakeError> {
        let Self::Handshake(hs) = self else {
            return Err(HandshakeError::Reject("Handshake error"));
        };
        if hs.is_my_turn() {
            return Err(HandshakeError::Reject("Handshake error"));
        }
        let Some((&status, msg)) = body.split_first() else {
            return Err(HandshakeError::Reject("Empty handshake message"));
        };
        if status != 0x00 {
            return Err(HandshakeError::Reject("Bad handshake error byte"));
        }
        // noise-c (C++-сервер) отвергал обрезанное сообщение и нулевой эфемерный ключ до
        // проверки MAC: это ошибки протокола, а не неверный ключ. snow первое назвал бы
        // ошибкой расшифровки, а второе принял бы.
        if msg.len() < DH_LEN + NOISE_MAC || msg[..DH_LEN].iter().all(|&b| b == 0) {
            return Err(HandshakeError::ReadFailed { mac: false });
        }
        let mut payload = [0u8; HANDSHAKE_MAX];
        if let Err(e) = hs.read_message(msg, &mut payload) {
            return Err(HandshakeError::ReadFailed {
                mac: matches!(e, snow::Error::Decrypt),
            });
        }
        if !hs.is_my_turn() {
            return Err(HandshakeError::Reject("Handshake error"));
        }
        let mut reply = [0u8; HANDSHAKE_MAX];
        let n = match hs.write_message(&[], &mut reply[1..]) {
            Ok(n) if hs.is_handshake_finished() => n,
            _ => return Err(HandshakeError::Reject("Handshake error")),
        };
        append_noise_raw(out, &[&reply[..n + 1]]);
        let Self::Handshake(hs) = std::mem::replace(self, Self::ClientHello) else {
            unreachable!("checked above");
        };
        let ts = hs
            .into_transport_mode()
            .map_err(|_| HandshakeError::SplitFailed)?;
        *self = Self::Data(Box::new(ts));
        Ok(())
    }

    /// Расшифровывает data-кадр в `buf`; возвращает тип сообщения и диапазон сообщения в `buf`
    /// либо причину отбросить клиента.
    pub fn decrypt(
        &mut self,
        body: &[u8],
        buf: &mut Vec<u8>,
    ) -> Result<(u32, Range<usize>), &'static str> {
        let Self::Data(ts) = self else {
            return Err("noise decrypt failed");
        };
        if buf.len() < body.len() {
            buf.resize(body.len(), 0);
        }
        let n = ts
            .read_message(body, &mut buf[..body.len()])
            .map_err(|_| "noise decrypt failed")?;
        if n < 4 {
            return Err("short noise message");
        }
        let msg_type = u32::from(u16::from_be_bytes([buf[0], buf[1]]));
        let len = usize::from(u16::from_be_bytes([buf[2], buf[3]]));
        if len > n - 4 {
            return Err("bad noise message length");
        }
        Ok((msg_type, 4..4 + len))
    }

    /// Дописывает один зашифрованный data-кадр; `scratch` временно хранит открытый текст.
    pub fn append_encrypted(
        &mut self,
        out: &mut Vec<u8>,
        msg_type: u32,
        msg: &[u8],
        scratch: &mut Vec<u8>,
    ) -> Result<(), EncryptError> {
        let Self::Data(ts) = self else {
            return Err(EncryptError::NotReady);
        };
        let body = 4 + msg.len();
        if body + NOISE_MAC > NOISE_MAX_FRAME {
            return Err(EncryptError::TooLarge);
        }
        scratch.clear();
        scratch.extend_from_slice(&(msg_type as u16).to_be_bytes());
        scratch.extend_from_slice(&(msg.len() as u16).to_be_bytes());
        scratch.extend_from_slice(msg);
        let start = out.len();
        out.resize(start + 3 + body + NOISE_MAC, 0);
        if let Ok(n) = ts.write_message(scratch, &mut out[start + 3..]) {
            out[start] = 0x01;
            out[start + 1..start + 3].copy_from_slice(&(n as u16).to_be_bytes());
            out.truncate(start + 3 + n);
            Ok(())
        } else {
            out.truncate(start);
            Err(EncryptError::Failed)
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub const NAME: &str = "yandex-midi-test";
    pub const MAC: &str = "aabbccddeeff";

    /// Клиентская сторона Noise-транспорта ESPHome (роль aioesphomeapi) на snow.
    pub struct Initiator {
        pub hs: Option<HandshakeState>,
        pub ts: Option<TransportState>,
    }

    impl Initiator {
        /// Client hello (пустой) + первое handshake-сообщение, как их шлёт aioesphomeapi.
        pub fn new(psk: &Psk) -> (Self, Vec<u8>) {
            let mut prologue = b"NoiseAPIInit".to_vec();
            prologue.extend_from_slice(&[0, 0]);
            let mut hs = snow::Builder::new(NOISE_PROTOCOL.parse().unwrap())
                .psk(0, psk.as_bytes())
                .unwrap()
                .prologue(&prologue)
                .unwrap()
                .build_initiator()
                .unwrap();
            let mut msg = [0u8; 128];
            msg[0] = 0x00;
            let n = hs.write_message(&[], &mut msg[1..]).unwrap();
            let mut out = Vec::new();
            append_noise_raw(&mut out, &[]);
            append_noise_raw(&mut out, &[&msg[..n + 1]]);
            (
                Self {
                    hs: Some(hs),
                    ts: None,
                },
                out,
            )
        }

        /// Читает handshake-сообщение сервера (сначала status-байт).
        pub fn finish(&mut self, body: &[u8]) -> Result<(), String> {
            if body.first() != Some(&0x00) {
                return Err(String::from_utf8_lossy(&body[1..]).into_owned());
            }
            let mut hs = self.hs.take().unwrap();
            let mut payload = [0u8; 128];
            hs.read_message(&body[1..], &mut payload)
                .map_err(|e| e.to_string())?;
            self.ts = Some(hs.into_transport_mode().unwrap());
            Ok(())
        }

        pub fn encrypt(&mut self, msg_type: u16, msg: &[u8]) -> Vec<u8> {
            let mut plain = msg_type.to_be_bytes().to_vec();
            plain.extend_from_slice(&(msg.len() as u16).to_be_bytes());
            plain.extend_from_slice(msg);
            let mut ct = vec![0u8; plain.len() + 16];
            let n = self
                .ts
                .as_mut()
                .unwrap()
                .write_message(&plain, &mut ct)
                .unwrap();
            let mut out = Vec::new();
            append_noise_raw(&mut out, &[&ct[..n]]);
            out
        }

        pub fn decrypt(&mut self, body: &[u8]) -> (u16, Vec<u8>) {
            let mut plain = vec![0u8; body.len()];
            let n = self
                .ts
                .as_mut()
                .unwrap()
                .read_message(body, &mut plain)
                .unwrap();
            let t = u16::from_be_bytes([plain[0], plain[1]]);
            let len = usize::from(u16::from_be_bytes([plain[2], plain[3]]));
            assert_eq!(len, n - 4);
            (t, plain[4..n].to_vec())
        }
    }

    /// Разбивает поток байт на Noise-кадры.
    pub fn noise_bodies(mut p: &[u8]) -> Vec<Vec<u8>> {
        let mut v = Vec::new();
        while !p.is_empty() {
            let NoiseFrame::Frame { body, used } = parse_noise(p, false) else {
                panic!("bad frame {p:?}");
            };
            v.push(body.to_vec());
            p = &p[used..];
        }
        v
    }

    fn server_frames(psk: &Psk, input: &[u8]) -> (Noise, Vec<u8>, Result<(), HandshakeError>) {
        let mut noise = Noise::ClientHello;
        let mut out = Vec::new();
        let NoiseFrame::Frame { body, used } = parse_noise(input, true) else {
            panic!()
        };
        noise.start(body, psk, NAME, MAC, &mut out).unwrap();
        let NoiseFrame::Frame { body, .. } = parse_noise(&input[used..], true) else {
            panic!()
        };
        let r = noise.handshake(body, &mut out);
        (noise, out, r)
    }

    #[test]
    fn plain_frames() {
        let mut out = Vec::new();
        append_plain(&mut out, 1, b"hi");
        append_plain(&mut out, 300, &[7u8; 200]);
        assert_eq!(&out[..5], b"\x00\x02\x01hi");
        let Plain::Frame {
            msg_type: 1,
            msg: b"hi",
            used: 5,
        } = parse_plain(&out)
        else {
            panic!("{:?}", parse_plain(&out));
        };
        let second = &out[5..];
        let Plain::Frame {
            msg_type: 300,
            msg,
            used,
        } = parse_plain(second)
        else {
            panic!();
        };
        assert_eq!((msg.len(), used), (200, second.len()));
        // Любое усечение валидного кадра ждёт ещё байт.
        for n in 1..second.len() {
            assert_eq!(parse_plain(&second[..n]), Plain::Incomplete, "{n}");
        }
        assert_eq!(parse_plain(b"\x01\x00\x00"), Plain::BadPreamble);
        assert_eq!(parse_plain(b"\x07"), Plain::BadPreamble);
    }

    #[test]
    fn plain_header_limits() {
        // 1 MiB принимается (ждём тело), на байт больше — нет.
        let mut p = vec![0u8];
        encode_varint(MAX_FRAME as u64, &mut p);
        encode_varint(9, &mut p);
        assert_eq!(parse_plain(&p), Plain::Incomplete);
        let mut p = vec![0u8];
        encode_varint(MAX_FRAME as u64 + 1, &mut p);
        encode_varint(9, &mut p);
        assert_eq!(parse_plain(&p), Plain::TooLarge);
        let mut p = vec![0u8];
        encode_varint(u64::MAX, &mut p);
        encode_varint(9, &mut p);
        assert_eq!(parse_plain(&p), Plain::TooLarge);
        // Заголовок, который не заканчивается: неполный до 21 байта, дальше — malformed.
        let mut p = vec![0u8];
        p.extend_from_slice(&[0x80; 20]);
        assert_eq!(parse_plain(&p), Plain::Incomplete);
        p.push(0x80);
        assert_eq!(parse_plain(&p), Plain::MalformedHeader);
        // 11-байтовый varint — malformed, даже если заканчивается.
        let mut p = vec![0u8, 1];
        p.extend_from_slice(&[0xff; 10]);
        p.extend_from_slice(&[0x01; 10]);
        assert_eq!(parse_plain(&p), Plain::MalformedHeader);
    }

    #[test]
    fn noise_frame_limits() {
        assert_eq!(parse_noise(b"\x00", true), NoiseFrame::BadIndicator);
        assert_eq!(parse_noise(b"\x01\x00", true), NoiseFrame::Incomplete);
        assert_eq!(
            parse_noise(b"\x01\x00\x81", true),
            NoiseFrame::BadHandshakeLen
        );
        assert_eq!(parse_noise(b"\x01\x00\x81", false), NoiseFrame::Incomplete);
        assert_eq!(parse_noise(b"\x01\x00\x02a", true), NoiseFrame::Incomplete);
        assert_eq!(
            parse_noise(b"\x01\x00\x02abc", true),
            NoiseFrame::Frame {
                body: b"ab",
                used: 5
            }
        );
        let mut big = vec![0x01, 0xff, 0xff];
        big.resize(3 + 0xffff, 0);
        assert!(matches!(
            parse_noise(&big, false),
            NoiseFrame::Frame { used: 65538, .. }
        ));
    }

    #[test]
    fn noise_handshake_and_data_both_ways() {
        let psk = Psk::new(std::array::from_fn(|i| i as u8 * 7));
        let (mut client, hello) = Initiator::new(&psk);
        let (mut noise, out, r) = server_frames(&psk, &hello);
        r.unwrap();
        let bodies = noise_bodies(&out);
        assert_eq!(bodies.len(), 2);
        assert_eq!(bodies[0], format!("\x01{NAME}\0{MAC}\0").as_bytes());
        assert_eq!(bodies[1].len(), 1 + 48);
        client.finish(&bodies[1]).unwrap();
        assert!(!noise.in_handshake());

        let frame = client.encrypt(7, b"");
        let NoiseFrame::Frame { body, .. } = parse_noise(&frame, false) else {
            panic!()
        };
        let mut buf = Vec::new();
        let (t, range) = noise.decrypt(body, &mut buf).unwrap();
        assert_eq!((t, &buf[range]), (7, &b""[..]));
        let frame = client.encrypt(1, b"\x0a\x03abc");
        let NoiseFrame::Frame { body, .. } = parse_noise(&frame, false) else {
            panic!()
        };
        let (t, range) = noise.decrypt(body, &mut buf).unwrap();
        assert_eq!((t, &buf[range]), (1, &b"\x0a\x03abc"[..]));

        let mut out = Vec::new();
        let mut scratch = Vec::new();
        noise
            .append_encrypted(&mut out, 90, b"\x08\x01", &mut scratch)
            .unwrap();
        let big = vec![5u8; NOISE_MAX_FRAME - 4 - NOISE_MAC];
        noise
            .append_encrypted(&mut out, 106, &big, &mut scratch)
            .unwrap();
        assert_eq!(
            noise.append_encrypted(
                &mut out,
                106,
                &[0; NOISE_MAX_FRAME - 4 - NOISE_MAC + 1],
                &mut scratch
            ),
            Err(EncryptError::TooLarge)
        );
        let bodies = noise_bodies(&out);
        assert_eq!(bodies.len(), 2);
        assert_eq!(client.decrypt(&bodies[0]), (90, b"\x08\x01".to_vec()));
        assert_eq!(client.decrypt(&bodies[1]), (106, big));

        // Испорченный и короткий кадры отбрасывают клиента.
        let mut frame = client.encrypt(7, b"");
        let last = frame.len() - 1;
        frame[last] ^= 1;
        assert_eq!(
            noise.decrypt(&frame[3..], &mut buf),
            Err("noise decrypt failed")
        );
    }

    #[test]
    fn noise_data_length_checks() {
        let psk = Psk::new([3u8; 32]);
        let (mut client, hello) = Initiator::new(&psk);
        let (mut noise, out, r) = server_frames(&psk, &hello);
        r.unwrap();
        client.finish(&noise_bodies(&out)[1]).unwrap();
        let mut buf = Vec::new();
        // Три байта открытого текста: нет места для типа и длины.
        let ts = client.ts.as_mut().unwrap();
        let mut ct = [0u8; 64];
        let n = ts.write_message(b"\x00\x07\x00", &mut ct).unwrap();
        assert_eq!(
            noise.decrypt(&ct[..n], &mut buf),
            Err("short noise message")
        );
        // Длина за пределами расшифрованных байт.
        let n = ts.write_message(b"\x00\x07\x00\x02a", &mut ct).unwrap();
        assert_eq!(
            noise.decrypt(&ct[..n], &mut buf),
            Err("bad noise message length")
        );
        // Хвост после объявленной длины игнорируется.
        let n = ts.write_message(b"\x00\x07\x00\x01ab", &mut ct).unwrap();
        let (t, range) = noise.decrypt(&ct[..n], &mut buf).unwrap();
        assert_eq!((t, &buf[range]), (7, &b"a"[..]));
        assert_eq!(
            noise.decrypt(&ct[..10], &mut buf),
            Err("noise decrypt failed")
        );
    }

    #[test]
    fn wrong_psk_is_a_mac_failure() {
        let (_, hello) = Initiator::new(&Psk::new([1u8; 32]));
        let (noise, out, r) = server_frames(&Psk::new([2u8; 32]), &hello);
        assert_eq!(r, Err(HandshakeError::ReadFailed { mac: true }));
        assert!(noise.in_handshake());
        // Наружу ушёл только server hello.
        assert_eq!(noise_bodies(&out).len(), 1);
    }

    #[test]
    fn handshake_rejections() {
        let psk = Psk::new([0u8; 32]);
        let check = |body: &[u8]| {
            let mut noise = Noise::ClientHello;
            let mut out = Vec::new();
            noise.start(&[], &psk, NAME, MAC, &mut out).unwrap();
            noise.handshake(body, &mut out)
        };
        assert_eq!(
            check(b""),
            Err(HandshakeError::Reject("Empty handshake message"))
        );
        assert_eq!(
            check(b"\x01abc"),
            Err(HandshakeError::Reject("Bad handshake error byte"))
        );
        // Обрезанные сообщения и нулевой эфемерный ключ — ошибки протокола, как в noise-c.
        assert_eq!(
            check(&[0u8; 1 + 31]),
            Err(HandshakeError::ReadFailed { mac: false })
        );
        let mut msg = vec![0u8];
        msg.extend_from_slice(&[9u8; 32 + 15]);
        assert_eq!(check(&msg), Err(HandshakeError::ReadFailed { mac: false }));
        let mut msg = vec![0u8];
        msg.extend_from_slice(&[0u8; 32 + 16]);
        assert_eq!(check(&msg), Err(HandshakeError::ReadFailed { mac: false }));
        // Мусор нужной длины не проходит MAC.
        let mut msg = vec![0u8];
        msg.extend_from_slice(&[9u8; 32 + 16]);
        assert_eq!(check(&msg), Err(HandshakeError::ReadFailed { mac: true }));
        // До client hello.
        let mut out = Vec::new();
        assert_eq!(
            Noise::ClientHello.handshake(b"\x00", &mut out),
            Err(HandshakeError::Reject("Handshake error"))
        );
    }

    #[test]
    fn prologue_binds_the_client_hello() {
        // Prologue инициатора рассчитан на пустой client hello; сервер, увидевший другой,
        // выведет другие ключи.
        let psk = Psk::new([4u8; 32]);
        let (_, hello) = Initiator::new(&psk);
        let mut noise = Noise::ClientHello;
        let mut out = Vec::new();
        noise.start(b"x", &psk, NAME, MAC, &mut out).unwrap();
        let NoiseFrame::Frame { body, .. } = parse_noise(&hello[3..], true) else {
            panic!()
        };
        assert_eq!(
            noise.handshake(body, &mut out),
            Err(HandshakeError::ReadFailed { mac: true })
        );
    }
}
