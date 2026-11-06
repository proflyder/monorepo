//! Transport of the Station Midi's Zigbee co-processor: Telink zbhci framing and the serial link
//! with its stop-and-wait ACKs, retries, hardware reset and boot detection.
//!
//! The chip is a Telink B91 on `/dev/ttyS2` at 115200 8N1 without flow control, running one of
//! pulsar's firmwares (`radio/`) or Yandex's gateway. [`frame`] documents the framing;
//! [`link`] the link.
//!
//! # Internal API
//!
//! - [`link::Link`] owns the byte stream (any [`tokio::io::AsyncRead`] +
//!   [`tokio::io::AsyncWrite`]) and the reset/boot lines ([`link::RadioLines`]). [`Link::open`]
//!   opens the station's tty (through `home-linux`) and starts a reset; [`Link::next`] is
//!   cancel-safe and yields [`link::LinkEvent`]s: frames, ACKs of frames sent with a
//!   [`link::AckTag`] (the C++ `AckCb` continuations) and boots of the radio's application.
//!   The link's own timers (reset pulse, boot wait, ACK retries, holds) only advance while
//!   `next()` is polled, so its owner polls it continuously from `open` on, as the C++ loop
//!   always ran.
//! - The tty is opened and configured by `home-linux`; this crate only needs the async stream.
//!   [`link::Link::with_io`] is the same link over an in-memory duplex, which is how the tests
//!   drive the real link against a fake radio and recording lines. Test suites take a real
//!   pseudo-terminal from `home-linux`'s `testing` feature.
//!
//! The roles, the coordinator protocol and flashing live in `pulsar-zigbee`; the deCONZ client
//! protocol in `pulsar-deconz`.
#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

pub mod bytes;
pub mod frame;
pub mod link;

pub use frame::{Frame, FrameCodec, build_frame, scan_frames};
pub use link::{AckTag, Link, LinkEvent, RadioError, RadioLines, SysfsLines};
