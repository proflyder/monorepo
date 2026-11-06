//! Playback through the firmware's GStreamer (channels Music / Announce; Announce is played by
//! our own ALSA writer) and the master volume
//! (ALSA "Master Playback Volume", softvol of the model's playback PCM).
#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

#[allow(unsafe_code)]
mod gst;
pub mod mixer;
mod pcm_out;
mod player;

pub use mixer::Mixer;
pub use player::{Channel, PlayId, PlayState, PlayerEvent, PlayerHandle, spawn};

/// Узкая часть профиля модели для ALSA-фасада: затворы усилителя и «SPK mute».
pub(crate) fn output_gates(playback: &pulsar_model::Playback) -> pulsar_alsa::OutputGates {
    pulsar_alsa::OutputGates {
        amp_enable_paths: playback.amp_enable_paths,
        spk_mute_ctl: playback.spk_mute_ctl,
    }
}

/// Входы `pulsar_alsa::open_playback` для PCM модели.
pub(crate) fn playback_spec(playback: &pulsar_model::Playback) -> pulsar_alsa::PlaybackSpec {
    pulsar_alsa::PlaybackSpec {
        pcm: playback.pcm,
        gates: output_gates(playback),
    }
}
