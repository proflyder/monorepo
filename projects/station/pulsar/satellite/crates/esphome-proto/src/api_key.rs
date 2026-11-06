//! Разбор `api_key`: base64 из 32 байт, как ESPHome `api: encryption: key`.
//!
//! Файловый контракт живёт в `pulsar-esphome`; здесь только побайтовое декодирование.

use base64::Engine;
use base64::alphabet;
use base64::engine::DecodePaddingMode;
use base64::engine::general_purpose::{GeneralPurpose, GeneralPurposeConfig};

use crate::frame::Psk;

const ENGINE: GeneralPurpose = GeneralPurpose::new(
    &alphabet::STANDARD,
    GeneralPurposeConfig::new()
        .with_decode_padding_mode(DecodePaddingMode::Indifferent)
        // C++-декодер останавливался на первом '=' и игнорировал биты после 32-го байта.
        .with_decode_allow_trailing_bits(true),
);

/// Стандартный base64 (padding опционален, всё после '=' игнорируется); `Some` только для
/// ровно 32 декодированных байт.
pub fn decode_psk(input: &[u8]) -> Option<Psk> {
    let body = input
        .iter()
        .position(|&b| b == b'=')
        .map_or(input, |padding| &input[..padding]);
    let bytes = ENGINE.decode(body).ok()?;
    let bytes: [u8; 32] = bytes.as_slice().try_into().ok()?;
    Some(Psk::new(bytes))
}

/// Срезает пробельные символы, которые обрезал C++-сервер (" \t\r\n"; не \v и не \f).
pub fn trim(s: &[u8]) -> &[u8] {
    let ws = |c: &u8| b" \t\r\n".contains(c);
    let start = s.iter().position(|c| !ws(c)).unwrap_or(s.len());
    let end = s.iter().rposition(|c| !ws(c)).map_or(start, |i| i + 1);
    &s[start..end]
}
