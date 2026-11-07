//! Native Sendspin player (Music Assistant's synchronized streaming protocol) on the official
//! sendspin-cpp client library, through the C shim in `shim/`. Roles: player (own ALSA sink on
//! the playback dmix), metadata, controller (transport commands), visualizer (8 log bands
//! 40 Hz–7.6 kHz, loudness, beats → `MusicFrame`).
//!
//! sendspin-cpp is only linked in the station builds (cfg `sendspin_ffi`, set by the Bazel
//! station configuration; the host build script leaves it unset); elsewhere `spawn` fails with
//! [`SendspinInitError::Unsupported`]. The event handler, the actor, the stream hold, the
//! visualizer and the identity store build and are tested on the host without C++.
#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

use std::path::PathBuf;

use thiserror::Error;

#[cfg_attr(not(sendspin_ffi), allow(dead_code))]
pub(crate) mod actor;
#[cfg_attr(not(sendspin_ffi), allow(dead_code))]
mod event;
#[cfg_attr(not(sendspin_ffi), allow(dead_code))]
mod gain;
#[cfg_attr(not(sendspin_ffi), allow(dead_code))]
mod music;
#[cfg_attr(not(sendspin_ffi), allow(dead_code))]
pub(crate) mod proto;
#[cfg_attr(not(sendspin_ffi), allow(dead_code))]
mod secrets;
#[cfg_attr(not(sendspin_ffi), allow(dead_code))]
pub(crate) mod state;
#[cfg_attr(not(sendspin_ffi), allow(dead_code))]
pub(crate) mod store;
#[cfg_attr(not(sendspin_ffi), allow(dead_code))]
pub(crate) mod visualizer;

#[cfg(sendspin_ffi)]
#[allow(unsafe_code)]
mod ffi;
#[cfg(sendspin_ffi)]
mod service;
#[cfg(sendspin_ffi)]
mod sink;
#[cfg(not(sendspin_ffi))]
mod stub;

#[cfg(sendspin_ffi)]
pub use service::{SendspinHandle, spawn};
#[cfg(not(sendspin_ffi))]
pub use stub::{SendspinHandle, spawn};

pub use store::{FileStore, IdentityStore};

/// Why the Sendspin player did not start.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SendspinInitError {
    #[error("sendspin: built without sendspin-cpp")]
    Unsupported,
    #[error("sendspin: client start failed")]
    Start,
}

pub struct SendspinConfig {
    /// Friendly name shown in Music Assistant.
    pub name: String,
    pub product_name: String,
    pub manufacturer: String,
    pub software_version: String,
    /// "aa:bb:cc:dd:ee:ff"; empty: the library detects it.
    pub mac: String,
    /// Identity / pairing persistence directory (created 0700 if missing, not recursively).
    pub state_dir: PathBuf,
    /// Playback PCM and output gates of this station.
    pub playback: &'static pulsar_model::Playback,
    pub port: u16,
}

/// Group transport commands (controller role).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transport {
    Play,
    Pause,
    Stop,
    Next,
    Previous,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SendspinEvent {
    /// A stream started (true) / ended (false); see `SendspinHandle::streaming`.
    Stream(bool),
    /// The server set the player volume, 0..1.
    Volume(f32),
    /// The server set the player mute.
    Mute(bool),
    /// Show the pairing code; "" = clear it.
    PairingCode(String),
    /// Ask the operator for the pairing gesture (`confirm_pairing_window`).
    PairingWindow(bool),
    /// Visualizer data from the server.
    Music(pulsar_core::MusicFrame),
}

/// Player volume in percent as the protocol carries it (C++ `lround(clamp(v, 0, 1) * 100)`).
fn volume_percent(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 100.0).round() as u8
}

/// Vectors of the C++ reference's pure functions (tests/golden/cxx_reference.txt): one line per
/// case, a tag, then the values; f32 as their bit pattern in hex.
#[cfg(test)]
mod golden {
    const TEXT: &str = include_str!("../tests/golden/cxx_reference.txt");

    /// The fields after the tag of every line tagged `tag`.
    pub fn lines(tag: &str) -> Vec<Vec<&'static str>> {
        TEXT.lines()
            .filter_map(|l| {
                let mut f = l.split_whitespace();
                (f.next() == Some(tag)).then(|| f.collect())
            })
            .collect()
    }

    pub fn f32(hex: &str) -> f32 {
        f32::from_bits(u32::from_str_radix(hex, 16).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_percent_matches_cxx() {
        let cases = golden::lines("volume");
        assert_eq!(cases.len(), 16);
        for f in cases {
            let want: u8 = f[1].parse().unwrap();
            assert_eq!(volume_percent(golden::f32(f[0])), want, "{f:?}");
        }
        assert_eq!(volume_percent(f32::NAN), 0);
    }

    #[test]
    fn spawn_without_sendspin_cpp_fails() {
        if cfg!(sendspin_ffi) {
            return;
        }
        let cfg = SendspinConfig {
            name: "test".into(),
            product_name: String::new(),
            manufacturer: String::new(),
            software_version: String::new(),
            mac: String::new(),
            state_dir: PathBuf::from("/nonexistent"),
            playback: &pulsar_model::profile_from_dt_id(b"axg_s420_v03")
                .unwrap()
                .audio
                .playback,
            port: 0,
        };
        assert_eq!(
            spawn(&cfg, 0.2, false, 1.0).err(),
            Some(SendspinInitError::Unsupported)
        );
    }
}
