//! The playback output shared by the ANNOUNCE writer (`pulsar-media`) and the Sendspin sink:
//! one PCM device port ([`PcmDevice`]), xrun recovery, the reopen backoff, statistics and the
//! ANNOUNCE priming / padding / drain policy ([`Announce`]).
#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

pub mod announce;
pub mod call;
mod device;
#[cfg(test)]
mod fake;
mod stats;

pub use announce::{CAPS, OutEvent, Output, OutputTuning};
pub use call::{CallOut, CallStats, CallTuning};
pub use device::{
    AlsaDevice, Backoff, PcmDevice, PcmError, PcmInfo, PcmRequest, Recovery, Step, recover,
};
#[cfg(test)]
pub use fake::FakeDevice;
pub use stats::Stats;
