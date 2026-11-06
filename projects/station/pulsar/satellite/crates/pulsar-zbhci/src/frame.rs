//! Framing of the Midi's Zigbee co-processor: Telink zbhci.
//!
//! `0x55 | type BE16 | length BE16 | XOR of type, length and payload | payload | 0xAA`. Same on
//! all three firmwares (`radio/`). The firmware DMA-receives exactly one frame per buffer and
//! answers every frame with an ACK (type 0x8000, payload `[type BE16][status][0]`); a second
//! frame sent before that ACK is merged into the same buffer and rejected, so frames go out
//! stop-and-wait ([`link`]).
//!
//! [`FrameCodec`] is the [`tokio_util::codec`] form for a stream; [`scan_frames`] stays the pure
//! scanner the golden test pins to the C++ output (it takes a batch and reports how much was
//! used, which the byte-stream decoder cannot).
//!
//! [`link`]: crate::link

use std::collections::VecDeque;

use tokio_util::bytes::{Buf, BufMut, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

pub const ACK: u16 = 0x8000;
/// Sent by the application once it runs.
pub const BOOT_IND: u16 = 0x8907;
/// Crash report, then a restart.
pub const CRASH_IND: u16 = 0x8909;
/// Payload limit of the firmware's transmit path.
pub const MAX_FRAME: usize = 0xF9;
/// The crash report carries up to 4 KiB of RAM.
pub const MAX_PAYLOAD: usize = 0x1100;

/// A decoded zbhci frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub ty: u16,
    pub payload: Vec<u8>,
}

/// A zbhci frame around `payload`.
pub fn build_frame(ty: u16, payload: &[u8]) -> Vec<u8> {
    let [t0, t1] = ty.to_be_bytes();
    // The length field is 16 bits; payloads never come near it (the radio takes 0xF9).
    let [n0, n1] = (payload.len() as u16).to_be_bytes();
    let mut f = Vec::with_capacity(payload.len() + 7);
    f.extend_from_slice(&[0x55, t0, t1, n0, n1]);
    f.push(payload.iter().fold(t0 ^ t1 ^ n0 ^ n1, |x, b| x ^ b));
    f.extend_from_slice(payload);
    f.push(0xAA);
    f
}

/// Finds the frames in `rx`, calling `on_frame` for each; returns how many bytes are used up
/// (frames and garbage before them; an incomplete frame at the end stays).
pub fn scan_frames(rx: &[u8], mut on_frame: impl FnMut(u16, &[u8])) -> usize {
    let mut pos = 0;
    while rx.len() - pos >= 7 {
        if rx[pos] != 0x55 {
            pos += 1;
            continue;
        }
        let h = &rx[pos..];
        let ty = u16::from_be_bytes([h[1], h[2]]);
        let len = usize::from(u16::from_be_bytes([h[3], h[4]]));
        // Normal frames carry at most 0xF9 bytes; only the crash report is longer. A stray 0x55
        // with a bogus length must not hold back the frames behind it.
        if len
            > if ty == CRASH_IND {
                MAX_PAYLOAD
            } else {
                MAX_FRAME
            }
        {
            pos += 1;
            continue;
        }
        if h.len() < len + 7 {
            break;
        }
        let payload = &h[6..6 + len];
        let x = payload.iter().fold(h[1] ^ h[2] ^ h[3] ^ h[4], |x, b| x ^ b);
        if h[6 + len] != 0xAA || x != h[5] {
            pos += 1;
            continue;
        }
        pos += len + 7;
        on_frame(ty, payload);
    }
    pos
}

/// `tokio_util::codec` framing of a zbhci byte stream.
#[derive(Default)]
pub struct FrameCodec {
    /// Frames of the last scan not handed out yet.
    ready: VecDeque<Frame>,
}

impl Decoder for FrameCodec {
    type Item = Frame;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Frame>, std::io::Error> {
        if let Some(frame) = self.ready.pop_front() {
            return Ok(Some(frame));
        }
        let used = scan_frames(src, |ty, payload| {
            self.ready.push_back(Frame {
                ty,
                payload: payload.to_vec(),
            });
        });
        src.advance(used);
        Ok(self.ready.pop_front())
    }
}

impl Encoder<Frame> for FrameCodec {
    type Error = std::io::Error;

    fn encode(&mut self, frame: Frame, dst: &mut BytesMut) -> Result<(), std::io::Error> {
        dst.put_slice(&build_frame(frame.ty, &frame.payload));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pattern(n: usize, seed: u8) -> Vec<u8> {
        (0..n).map(|i| seed.wrapping_add((i * 13) as u8)).collect()
    }

    fn hex(p: &[u8]) -> String {
        if p.is_empty() {
            return "-".into();
        }
        p.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn assert_golden(golden: &str, actual: &str) {
        let want: Vec<&str> = golden.lines().collect();
        let got: Vec<&str> = actual.lines().collect();
        for (i, (w, g)) in want.iter().zip(&got).enumerate() {
            assert_eq!(g, w, "line {} differs from the C++ output", i + 1);
        }
        assert_eq!(got.len(), want.len(), "line count differs");
    }

    /// The C++ scanner's output on the same streams (tests/golden/link_frames.txt).
    #[test]
    fn frames_match_cxx_golden() {
        let mut out = Vec::new();
        for (ty, p) in [
            (0x0A00, vec![]),
            (0x0900, vec![]),
            (0x0A03, vec![0x3C]),
            (0x0A10, pattern(16, 1)),
            (0x0A04, pattern(0xF9, 7)),
            (0x1234, pattern(300, 9)),
        ] {
            out.push(format!("build {ty:04x} {}", hex(&build_frame(ty, &p))));
        }
        let mut case = |name: &str, chunks: Vec<Vec<u8>>| {
            out.push(format!("case {name}"));
            let mut rx = Vec::new();
            for c in chunks {
                out.push(format!("in {}", c.len()));
                rx.extend_from_slice(&c);
                let used = scan_frames(&rx, |ty, p| match ty {
                    BOOT_IND => out.push("boot".into()),
                    ACK => {} // nothing in flight: ignored
                    _ => out.push(format!("frame {ty:04x} {} {}", p.len(), hex(p))),
                });
                rx.drain(..used);
                out.push(format!("rest {}", rx.len()));
            }
        };
        let cat = |parts: &[&[u8]]| parts.concat();
        case("clean", vec![build_frame(0x8A00, &[1, 2, 3])]);
        case("empty-payload", vec![build_frame(0x8A20, &[])]);
        case(
            "garbage",
            vec![cat(&[
                &[0x00, 0x12, 0xAA, 0xFF],
                &build_frame(0x8A10, &[0x05, 0x00]),
                &[0x13, 0x37],
                &build_frame(0x8A11, &pattern(20, 3)),
                &[0x01],
            ])],
        );
        let mut bad = build_frame(0x8A10, &[0x01, 0x02]);
        bad[5] ^= 0x40;
        case(
            "bad-xor",
            vec![cat(&[&bad, &build_frame(0x8A10, &[0x03, 0x04])])],
        );
        let mut bad = build_frame(0x8A10, &[0x01, 0x02]);
        *bad.last_mut().unwrap() = 0xAB;
        case(
            "bad-end",
            vec![cat(&[&bad, &build_frame(0x8A10, &[0x05, 0x06])])],
        );
        case(
            "stray-start",
            vec![cat(&[
                &[0x55, 0x8A, 0x11, 0xFF, 0xFF, 0x00, 0x00],
                &build_frame(0x8A00, &[0x07]),
            ])],
        );
        case(
            "stray-start-plausible-length",
            vec![
                cat(&[
                    &[0x55, 0x8A, 0x11, 0x00, 0x40, 0x00],
                    &build_frame(0x8A00, &[0x08]),
                ]),
                pattern(0x40, 1),
            ],
        );
        case(
            "crash-report",
            vec![build_frame(0x8909, &pattern(0x1000, 5))],
        );
        case(
            "crash-report-max",
            vec![build_frame(0x8909, &pattern(0x1100, 6))],
        );
        case(
            "crash-report-too-long",
            vec![cat(&[
                &build_frame(0x8909, &pattern(0x1101, 6)),
                &build_frame(0x8A00, &[0x09]),
            ])],
        );
        case(
            "normal-too-long",
            vec![cat(&[
                &build_frame(0x8A11, &pattern(0xFA, 2)),
                &build_frame(0x8A00, &[0x0A]),
            ])],
        );
        case("normal-max", vec![build_frame(0x8A11, &pattern(0xF9, 4))]);
        let f = build_frame(0x8A11, &pattern(30, 8));
        case(
            "split",
            vec![f[..3].to_vec(), f[3..20].to_vec(), f[20..].to_vec()],
        );
        let mut outer = build_frame(
            0x8A11,
            &cat(&[&[0xEE], &build_frame(0x8A00, &[0x0B, 0x0C])]),
        );
        outer[5] ^= 1;
        case("nested", vec![outer]);
        case(
            "boot",
            vec![cat(&[
                &build_frame(0x8907, &[]),
                &build_frame(0x8A00, &[0x0D]),
            ])],
        );
        case(
            "ack-idle",
            vec![cat(&[
                &build_frame(0x8000, &[0x0A, 0x00, 0x00, 0x00]),
                &build_frame(0x8A00, &[0x0E]),
            ])],
        );
        assert_golden(
            include_str!("../tests/golden/link_frames.txt"),
            &out.join("\n"),
        );
    }

    /// The decoder hands out the same frames in chunks as the batch scanner.
    #[test]
    fn codec_decodes_across_chunks() {
        let mut codec = FrameCodec::default();
        let mut buf = BytesMut::new();
        let a = build_frame(0x8A00, &[1, 2, 3]);
        let b = build_frame(0x8A11, &pattern(20, 3));
        buf.extend_from_slice(&a[..4]);
        assert_eq!(codec.decode(&mut buf).unwrap(), None);
        buf.extend_from_slice(&a[4..]);
        buf.extend_from_slice(&b);
        assert_eq!(
            codec.decode(&mut buf).unwrap(),
            Some(Frame {
                ty: 0x8A00,
                payload: vec![1, 2, 3]
            })
        );
        assert_eq!(
            codec.decode(&mut buf).unwrap(),
            Some(Frame {
                ty: 0x8A11,
                payload: pattern(20, 3)
            })
        );
        assert_eq!(codec.decode(&mut buf).unwrap(), None);
        assert!(buf.is_empty());
    }
}
