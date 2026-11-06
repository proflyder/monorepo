//! LAN services of the daemon: the mDNS responder through which Home Assistant and Music
//! Assistant find the station, and the wake word arbiter between stations on one LAN.
#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

pub mod arbiter;
pub mod mdns;
#[allow(unsafe_code)]
mod sys;

pub(crate) use home_linux::io_error_text as errstr;
