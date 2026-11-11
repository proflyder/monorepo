//! Volume, mute, ducking, the HA media player and the Sendspin hooks (controller.cpp "media").

use pulsar_core::volume::curve_gain;
use pulsar_core::{ClockShot, now_ms};
use pulsar_esphome::{MediaCommand, MediaCommandRequest};
use pulsar_media::{Channel, PlayState};
use pulsar_model::VolumeCurve;
use pulsar_sendspin::SendspinEvent;
use tracing::debug;

use super::earcons;
use super::logic::{self, cxx_max, cxx_min};
use super::{Controller, Pending};

/// Gain of the music while a dialog, announcement, timer ring or closing earcon is on.
const DUCK_GAIN: f32 = 0.15;

/// Volume, mute and ducking of both playback channels and Sendspin. The gain formulas live
/// here only: `apply_gains` and the Sendspin start-up used to compute `master_gain` separately.
pub struct Policy {
    curve: &'static VolumeCurve,
    ducked: bool,
    /// «Громкость объявлений», 0..1: absolute volume on the model curve for spoken
    /// announcements and TTS answers only. 1.0 means «as the master volume» because the
    /// announce channel can never be louder than the hardware master (see `announce_gain`).
    announce_volume: f32,
    /// The ANNOUNCE channel currently carries an announcement or a TTS answer (scaled by
    /// `announce_volume`), not an earcon or the timer ring (which stay on the master volume).
    announce_speech: bool,
}

impl Policy {
    pub fn new(curve: &'static VolumeCurve, announce_volume: f32) -> Self {
        Self {
            curve,
            ducked: false,
            announce_volume: cxx_min(1.0, cxx_max(0.0, announce_volume)),
            announce_speech: false,
        }
    }

    /// Sets the duck state; true when it changed (the gains then have to be re-applied).
    pub fn set_ducked(&mut self, want: bool) -> bool {
        if want == self.ducked {
            return false;
        }
        self.ducked = want;
        true
    }

    /// Master output gain on the model's curve (software volume models, e.g. the Mini).
    pub fn master_gain(&self, volume: f32) -> f32 {
        pulsar_core::volume::master_gain(self.curve, volume)
    }

    /// Music channel and Sendspin software gain: mute zeroes it, duck scales it.
    pub fn music_gain(&self, volume: f32, muted: bool) -> f32 {
        if muted {
            0.0
        } else {
            self.master_gain(volume) * if self.ducked { DUCK_GAIN } else { 1.0 }
        }
    }

    /// Announce channel gain: mute zeroes it, speech is never ducked; earcons and the timer
    /// ring stay on the master volume.
    ///
    /// «Громкость объявлений» is an absolute volume on the same dB curve as the master, not a
    /// multiplier: 5 % there sounds like 5 % of the station. The channel carries the ratio
    /// `curve_gain(announce) / curve_gain(volume)` on top of `master_gain`, so on the Midi the
    /// hardware softvol still applies the master and the channel only attenuates when the
    /// announcement is quieter. The ratio is capped at 1: the announcement can never be louder
    /// than the current master volume. For software-volume models (the Mini) `master_gain` is
    /// the curve itself, so the product is the absolute announce gain.
    pub fn announce_gain(&self, volume: f32, muted: bool) -> f32 {
        if muted {
            return 0.0;
        }
        let master = self.master_gain(volume);
        if !self.announce_speech {
            return master;
        }
        let current = curve_gain(self.curve, volume);
        if current <= 0.0 {
            // Master is at the bottom of the range: the announcement cannot be louder.
            return master;
        }
        let target = curve_gain(self.curve, self.announce_volume);
        master * (target / current).min(1.0)
    }

    /// «Громкость объявлений» (0..1, absolute on the model curve); true when the current
    /// announce gain changed.
    pub fn set_announce_volume(&mut self, volume: f32) -> bool {
        let volume = cxx_min(1.0, cxx_max(0.0, volume));
        if volume == self.announce_volume {
            return false;
        }
        self.announce_volume = volume;
        self.announce_speech
    }

    /// Marks what the next ANNOUNCE playback carries; true when the gain changed.
    pub fn set_announce_speech(&mut self, speech: bool) -> bool {
        if speech == self.announce_speech {
            return false;
        }
        self.announce_speech = speech;
        true
    }
}

impl Controller {
    pub(super) fn update_duck(&mut self) {
        let want = logic::want_duck(
            self.phase(),
            self.session.announcing(),
            self.timers_ha.ringing(),
            self.session.closing_earcon(),
        );
        if self.media.set_ducked(want) {
            self.apply_gains();
        }
    }

    /// Mute: softvol raw 0 is the bottom of its dB range (-60 dB on the Midi), still audible, so
    /// mute also zeroes the software gains of both playback channels and Sendspin. On models
    /// without a usable ALSA master (the Mini) this is also where the master volume is applied.
    pub(super) fn apply_gains(&self) {
        let music = self.media.music_gain(self.st.volume, self.st.muted);
        self.deps.player.set_channel_gain(Channel::Music, music);
        self.deps.player.set_channel_gain(
            Channel::Announce,
            self.media.announce_gain(self.st.volume, self.st.muted),
        );
        if let Some(ss) = &self.deps.sendspin {
            ss.set_duck_gain(music);
        }
    }

    /// What the ANNOUNCE playback now starting carries: true for a spoken announcement or a TTS
    /// answer (scaled by «Громкость объявлений»), false for an earcon or the timer ring (full
    /// master volume). Every `play` on ANNOUNCE goes through this, so the flag always matches the
    /// sound on the channel.
    pub(super) fn set_announce_speech(&mut self, speech: bool) {
        if self.media.set_announce_speech(speech) {
            self.apply_gains();
        }
    }

    pub(super) fn set_muted(&mut self, m: bool) {
        self.st.muted = m;
        self.deps.mixer.set_muted(m);
        self.apply_gains();
        if let Some(ss) = &self.deps.sendspin {
            ss.report_volume(self.st.volume, self.st.muted);
        }
        self.publish_media();
        self.persist();
    }

    /// `feedback`: a local change (buttons, voice), shown on the ring and the clock; it also
    /// unmutes.
    pub(super) fn set_volume(&mut self, v: f32, feedback: bool) {
        let before = (self.st.volume, self.st.muted);
        self.st.volume = cxx_min(1.0, cxx_max(0.0, v));
        self.deps.mixer.set_volume(self.st.volume);
        if self.st.muted && feedback {
            self.st.muted = false;
            self.deps.mixer.set_muted(false);
        }
        self.apply_gains();
        if feedback {
            self.deps.display.leds.show_volume(self.st.volume);
            // mul_add: the station C++ fuses `volume * 100 + 0.5` (fmadd on aarch64, vfma.f32 on
            // armhf, checked in the -O2 asm of Controller::set_volume); the rounding follows it.
            self.deps
                .display
                .clock
                .flash_number(self.st.volume.mul_add(100.0, 0.5) as i32);
            self.volume_click(before);
        }
        if let Some(ss) = &self.deps.sendspin {
            ss.report_volume(self.st.volume, self.st.muted);
        }
        self.publish_media();
        self.persist();
    }

    /// Stock click after a local change (keys, voice words); `before` = (volume, muted) before it.
    fn volume_click(&mut self, before: (f32, bool)) {
        if !logic::want_volume_click(
            before,
            (self.st.volume, self.st.muted),
            self.session.phase(),
            self.wake.is_claiming(),
            self.session.announcing(),
            self.timers_ha.ringing(),
            self.session.closing_earcon(),
        ) {
            return;
        }
        // Music is not ducked under it; its Done has no Pending entry and is ignored.
        self.set_announce_speech(false);
        self.deps.player.play(
            Channel::Announce,
            vec![earcons::path(self.model.earcons.volume)],
        );
    }

    /// Reads the player state as it is now: right after a command to the player it is still the
    /// previous one (as in C++); the change comes as `PlayerEvent::State`, which publishes again.
    pub(super) fn publish_media(&self) {
        let s = logic::media_state(
            self.session.announcing(),
            self.deps.player.state(Channel::Music),
            self.sendspin_streaming(),
        );
        self.deps.ha.publish_media(s, self.st.volume, self.st.muted);
        self.update_music();
        // The local command spotter follows playback: on while music/announcement/
        // timer speech plays, off in silence and during a dialog.
        self.update_commands();
    }

    pub(super) fn update_music(&self) {
        let ps = self.deps.player.state(Channel::Music);
        let media =
            matches!(ps, PlayState::Playing | PlayState::Loading) || self.sendspin_streaming();
        self.deps.display.leds.set_media_playing(media);
        self.deps.display.clock.set_media_playing(media);
        // The speaker-reference analysis only runs while its result can be shown.
        self.deps.frontend.set_music_analysis(
            media
                && ((super::music_viz(&self.st) != pulsar_hw::MusicStyle::Off
                    && !self.st.light.on)
                    || (super::clock_music(&self.st) != pulsar_clock::ClockMusicStyle::Off
                        && self.st.clock_on)),
        );
    }

    pub(super) fn on_media_command(&mut self, cmd: MediaCommandRequest) {
        if let Some(url) = cmd.media_url {
            if cmd.announcement == Some(true) {
                // Media-player announcements report completion through state only.
                let seq = self.session.start_announce();
                self.update_duck();
                self.update_ui();
                self.set_announce_speech(true);
                let id = self.deps.player.play(Channel::Announce, vec![url]);
                self.pending.insert(id, Pending::MediaAnnounce { seq });
            } else {
                if let Some(ss) = &self.deps.sendspin {
                    ss.yield_to_local(); // HA media takes the speaker from Sendspin
                }
                self.deps.player.play(Channel::Music, vec![url]);
            }
        }
        if let Some(v) = cmd.volume {
            self.set_volume(v, false);
        }
        if let Some(c) = cmd.command {
            let ss_on = self.sendspin_streaming();
            let music = || self.deps.player.state(Channel::Music);
            match c {
                MediaCommand::Play => {
                    if ss_on
                        && music() == PlayState::Paused
                        && let Some(ss) = &self.deps.sendspin
                    {
                        ss.yield_to_local();
                    }
                    self.deps.player.resume(Channel::Music);
                }
                MediaCommand::Pause => {
                    if ss_on || matches!(music(), PlayState::Playing | PlayState::Loading) {
                        self.deps.display.clock.play(ClockShot::Pause);
                    }
                    // While Sendspin plays, HA's pause/stop leave the Sendspin group (the
                    // speaker stops).
                    if ss_on
                        && music() != PlayState::Playing
                        && let Some(ss) = &self.deps.sendspin
                    {
                        ss.yield_to_local();
                    }
                    self.deps.player.pause(Channel::Music);
                }
                MediaCommand::Toggle => {
                    if music() == PlayState::Playing {
                        self.deps.display.clock.play(ClockShot::Pause);
                        self.deps.player.pause(Channel::Music);
                    } else if ss_on {
                        self.deps.display.clock.play(ClockShot::Pause);
                        if let Some(ss) = &self.deps.sendspin {
                            ss.yield_to_local();
                        }
                    } else {
                        self.deps.player.resume(Channel::Music);
                    }
                }
                MediaCommand::Stop => {
                    self.deps.player.stop(Channel::Music);
                    if ss_on && let Some(ss) = &self.deps.sendspin {
                        ss.yield_to_local();
                    }
                }
                MediaCommand::Mute | MediaCommand::Unmute => {
                    self.set_muted(c == MediaCommand::Mute);
                }
                MediaCommand::VolumeUp | MediaCommand::VolumeDown => {
                    let up = c == MediaCommand::VolumeUp;
                    self.set_volume(
                        pulsar_core::volume::step_level(&self.model.volume, self.st.volume, up),
                        false,
                    );
                }
                _ => debug!("media command {} ignored", c as u32),
            }
        }
        self.publish_media();
    }

    pub(super) fn on_sendspin(&mut self, ev: SendspinEvent) {
        match ev {
            SendspinEvent::Stream(active) => {
                // Music Assistant took over the speaker: the HA media channel yields to it.
                if active && self.deps.player.state(Channel::Music) != PlayState::Idle {
                    self.deps.player.stop(Channel::Music);
                }
                // The HA media channel playing here took the speaker over (yield_to_local): not a
                // pause in Music Assistant. on_player clears the stamp if its state comes later.
                let taken_over = matches!(
                    self.deps.player.state(Channel::Music),
                    PlayState::Playing | PlayState::Loading
                );
                self.session
                    .on_sendspin_stream(active, taken_over, now_ms());
                self.publish_media();
            }
            SendspinEvent::Volume(v) => self.set_volume(v, false),
            SendspinEvent::Mute(m) => self.set_muted(m),
            SendspinEvent::PairingCode(code) => {
                if code.is_empty() {
                    self.deps.display.clock.clear_code();
                } else {
                    self.deps.display.clock.show_code(code);
                }
            }
            SendspinEvent::PairingWindow(open) => {
                self.sendspin_pairing_prompt = open;
                if open {
                    self.deps.display.clock.show_code("----".into());
                } else {
                    self.deps.display.clock.clear_code();
                }
            }
            SendspinEvent::Music(f) => {
                self.deps.display.leds.music_frame(f, true);
                self.deps.display.clock.music_frame(f, true);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn midi() -> &'static pulsar_model::DeviceProfile {
        pulsar_model::profile_from_compatible(b"yandex,cucumber\0").unwrap()
    }

    fn mini() -> &'static pulsar_model::DeviceProfile {
        pulsar_model::profile_from_dt_id(b"axg_s420_v03\0").unwrap()
    }

    fn policy_for(model: &'static pulsar_model::DeviceProfile, announce_volume: f32) -> Policy {
        Policy::new(&model.volume, announce_volume)
    }

    #[test]
    fn equal_sliders_keep_the_ordinary_master_gain_on_the_midi() {
        // Midi has hardware volume: 5 % is raw 49, and the announce channel adds no gain.
        let mut p = policy_for(midi(), 0.05);
        p.set_announce_speech(true);
        assert_eq!(p.announce_gain(0.05, false), 1.0);
        assert_eq!(p.announce_gain(0.05, false), p.master_gain(0.05));
        // The default 100 % is capped at the master, so it still means «as the main volume».
        let mut p = policy_for(midi(), 1.0);
        p.set_announce_speech(true);
        assert_eq!(p.announce_gain(0.05, false), p.master_gain(0.05));
    }

    #[test]
    fn quieter_announce_volume_follows_the_curve() {
        let model = midi();
        let mut p = policy_for(model, 0.05);
        p.set_announce_speech(true);
        let gain = p.announce_gain(0.5, false);
        let expected = curve_gain(&model.volume, 0.05) / curve_gain(&model.volume, 0.5);
        assert!((gain - expected).abs() < 1e-7, "{gain} vs {expected}");
        assert!(gain < p.master_gain(0.5));
    }

    #[test]
    fn louder_announce_volume_is_capped_at_the_master() {
        let mut p = policy_for(midi(), 0.5);
        p.set_announce_speech(true);
        assert_eq!(p.announce_gain(0.05, false), p.master_gain(0.05));
        // At 100 % the hardware master is the limit, not +26 dB over it.
        let mut p = policy_for(midi(), 1.0);
        p.set_announce_speech(true);
        assert_eq!(p.announce_gain(0.05, false), p.master_gain(0.05));
    }

    #[test]
    fn mute_zeroes_the_announce_gain() {
        let mut p = policy_for(midi(), 0.05);
        p.set_announce_speech(true);
        assert_eq!(p.announce_gain(0.5, true), 0.0);
    }

    #[test]
    fn software_volume_multiplies_the_ratio_with_the_master() {
        let model = mini();
        let mut p = policy_for(model, 0.05);
        p.set_announce_speech(true);
        // Equal sliders: the channel gain is the ordinary 5 % software gain.
        assert_eq!(p.announce_gain(0.05, false), p.master_gain(0.05));
        assert!((p.announce_gain(0.05, false) - curve_gain(&model.volume, 0.05)).abs() < 1e-7);
        // Quieter announcement: the channel gain is the absolute curve gain.
        let mut p = policy_for(model, 0.05);
        p.set_announce_speech(true);
        assert!((p.announce_gain(0.5, false) - curve_gain(&model.volume, 0.05)).abs() < 1e-7);
        // Louder announcement: capped at the software master.
        let mut p = policy_for(model, 1.0);
        p.set_announce_speech(true);
        assert_eq!(p.announce_gain(0.05, false), p.master_gain(0.05));
    }

    #[test]
    fn announce_volume_change_reapplies_only_while_speech_plays() {
        let mut p = policy_for(midi(), 1.0);
        assert!(
            !p.set_announce_volume(0.4),
            "idle: the value is stored, nothing audible changes"
        );
        assert!(p.set_announce_speech(true), "speech is now scaled to 0.4");
        assert!(!p.set_announce_volume(0.4), "same value");
        assert!(
            p.set_announce_volume(0.6),
            "a new value changes speech gain"
        );
        assert_eq!(
            p.announce_gain(0.5, false),
            p.master_gain(0.5),
            "0.6 is above the 0.5 master"
        );
        assert!(p.set_announce_volume(0.4));
        assert!(p.announce_gain(0.5, false) < p.master_gain(0.5));
        // A zero announce volume is silence for announcements, not mute of the station.
        assert!(p.set_announce_volume(0.0));
        assert_eq!(p.announce_gain(0.5, false), 0.0);
        assert_eq!(p.music_gain(0.5, false), p.master_gain(0.5));
        // Earcons and the timer ring switch the speech scale off entirely.
        assert!(p.set_announce_speech(false));
        assert_eq!(p.announce_gain(0.5, false), p.master_gain(0.5));
    }
}
