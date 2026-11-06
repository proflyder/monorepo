//! Standard base64 with padding.
//!
//! PROTOCOL.md accepts non-canonical trailing bits (`"ZZ=="` decodes to
//! `0x65`), so the engine allows them while still requiring canonical padding
//! placement. Whitespace and foreign characters are rejected.

use base64::Engine as _;
use base64::alphabet;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};

use crate::Error;

const ENGINE: GeneralPurpose = GeneralPurpose::new(
    &alphabet::STANDARD,
    GeneralPurposeConfig::new()
        .with_decode_padding_mode(DecodePaddingMode::RequireCanonical)
        .with_decode_allow_trailing_bits(true),
);

pub fn b64_encode(data: &[u8]) -> String {
    ENGINE.encode(data)
}

/// Strict decoder. Returns an error for wrong length, misplaced or excessive
/// padding, whitespace and any character outside the alphabet.
pub fn b64_decode(text: &str) -> Result<Vec<u8>, Error> {
    ENGINE
        .decode(text)
        .map_err(|_| Error::new("invalid base64"))
}
