#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

mod codec;
mod config;
#[allow(unsafe_code)]
mod ffi;
mod jitter;
mod machine;
mod rtc;
mod service;
mod signaling;

pub use codec::Codec;
pub use config::{CA_FILE, CALL_DIR, CallConfig, ConfigError, TOKEN_FILE, URL_FILE};
pub use machine::{CallPhase, HangupReason};
pub use service::{CallHandle, Downlink, SinkStats, spawn};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallEvent {
    Incoming { caller: String },
    Active,
    Ended,
}

pub const UPLINK_RATE: u32 = 16_000;
pub const CALL_RATE: u32 = 48_000;
pub const FRAME_MS: u32 = 20;
pub const FRAME_SAMPLES: usize = (CALL_RATE as usize * FRAME_MS as usize) / 1000;
pub const UPLINK_CHUNK: usize = 512;

pub const RING_MS: u64 = 1200;
pub const JITTER_MIN_MS: u32 = 40;
pub const JITTER_MAX_MS: u32 = 100;
pub const JITTER_GIVEUP_MS: u32 = 400;
