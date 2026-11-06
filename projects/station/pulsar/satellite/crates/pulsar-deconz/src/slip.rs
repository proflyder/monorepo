//! deCONZ's framing: SLIP (0xC0 ends a frame, 0xDB escapes) with a 16-bit two's complement
//! checksum, LE, after the frame: `[command][seq][status][frame length LE16]...`.
//!
//! [`SlipCodec`] is the [`tokio_util::codec`] form; the server's transport uses it directly.

use tokio_util::bytes::{Buf, BufMut, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

pub const END: u8 = 0xC0;
pub const ESC: u8 = 0xDB;
pub const ESC_END: u8 = 0xDC;
pub const ESC_ESC: u8 = 0xDD;

/// SLIP bytes kept of one frame; the rest is dropped, as the C++ server did.
pub const MAX_SLIP: usize = 1024;

/// deCONZ's 16-bit two's complement of the byte sum.
pub fn checksum(f: &[u8]) -> u16 {
    let sum = f.iter().fold(0u16, |s, &b| s.wrapping_add(u16::from(b)));
    (!sum).wrapping_add(1)
}

/// The raw (still escaped) bytes of one frame between END markers.
#[derive(Default)]
pub struct SlipCodec {
    partial: Vec<u8>,
}

impl SlipCodec {
    fn unescape(raw: &[u8]) -> Vec<u8> {
        let mut f = Vec::with_capacity(raw.len());
        let mut i = 0;
        while i < raw.len() {
            let mut b = raw[i];
            if b == ESC && i + 1 < raw.len() {
                i += 1;
                b = match raw[i] {
                    ESC_END => END,
                    ESC_ESC => ESC,
                    other => other,
                };
            }
            f.push(b);
            i += 1;
        }
        f
    }
}

impl Decoder for SlipCodec {
    type Item = Vec<u8>;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Vec<u8>>, std::io::Error> {
        while let Some(b) = src.first().copied() {
            src.advance(1);
            if b == END {
                if self.partial.is_empty() {
                    continue;
                }
                let raw = std::mem::take(&mut self.partial);
                return Ok(Some(Self::unescape(&raw)));
            }
            if self.partial.len() < MAX_SLIP {
                self.partial.push(b);
            }
        }
        Ok(None)
    }
}

impl Encoder<&[u8]> for SlipCodec {
    type Error = std::io::Error;

    fn encode(&mut self, frame: &[u8], dst: &mut BytesMut) -> Result<(), std::io::Error> {
        dst.put_u8(END);
        for &b in frame {
            match b {
                END => dst.put_slice(&[ESC, ESC_END]),
                ESC => dst.put_slice(&[ESC, ESC_ESC]),
                _ => dst.put_u8(b),
            }
        }
        dst.put_u8(END);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_is_twos_complement_of_the_sum() {
        assert_eq!(checksum(&[0x0D, 0x01, 0x00, 0x09, 0x00]), 0xFFE9);
        assert_eq!(checksum(&[]), 0);
        assert_eq!(
            checksum(&[0xFF; 300]).wrapping_add((300u32 * 0xFF) as u16),
            0
        );
    }

    #[test]
    fn codec_round_trips_and_caps() {
        let mut codec = SlipCodec::default();
        let mut bytes = BytesMut::new();
        codec.encode(&[0xC0, 0xDB, 0x01], &mut bytes).unwrap();
        assert_eq!(bytes.as_ref(), [END, ESC, ESC_END, ESC, ESC_ESC, 0x01, END]);
        assert_eq!(
            codec.decode(&mut bytes).unwrap(),
            Some(vec![0xC0, 0xDB, 0x01])
        );
        assert_eq!(codec.decode(&mut bytes).unwrap(), None);
        // Bytes past MAX_SLIP are dropped, END is still recognised.
        let mut bytes = BytesMut::new();
        bytes.put_slice(&[0x55; MAX_SLIP + 10]);
        bytes.put_u8(END);
        let frame = codec.decode(&mut bytes).unwrap().unwrap();
        assert_eq!(frame.len(), MAX_SLIP);
    }
}
