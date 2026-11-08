//! Playback through the firmware's GStreamer (channels Music / Announce; Announce is played by
//! the ANNOUNCE writer of `pulsar-playback`) and the master volume
//! (ALSA "Master Playback Volume", softvol of the model's playback PCM).
#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

pub(crate) mod backend;
pub(crate) mod core;
#[allow(unsafe_code)]
mod gst;
pub mod mixer;
mod player;

pub use core::{Channel, PlayId, PlayState, PlayerEvent};
pub use mixer::{Mixer, MixerError};
pub use player::{PlayerHandle, PlayerInitError, spawn, try_spawn};

/// Канал `Call` (голос собеседника): отдельный потоковый writer с маленьким буфером.
/// Отдельно от GStreamer-каналов Music/Announce: звонок не элемент, а непрерывный поток.
#[must_use]
pub fn call_output(playback: &'static pulsar_model::Playback) -> Option<pulsar_playback::CallOut> {
    pulsar_playback::CallOut::spawn(playback, pulsar_playback::CallTuning::CALL)
}

/// Узкая часть профиля модели для ALSA-фасада: затворы усилителя и «SPK mute».
pub(crate) fn output_gates(playback: &pulsar_model::Playback) -> pulsar_alsa::OutputGates {
    pulsar_alsa::OutputGates {
        amp_enable_paths: playback.amp_enable_paths,
        spk_mute_ctl: playback.spk_mute_ctl,
    }
}
