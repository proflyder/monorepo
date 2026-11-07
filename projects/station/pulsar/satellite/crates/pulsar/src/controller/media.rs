//! Volume, mute, ducking, the HA media player and the Sendspin hooks (controller.cpp "media").

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
}

impl Policy {
    pub fn new(curve: &'static VolumeCurve) -> Self {
        Self {
            curve,
            ducked: false,
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

    /// Announce channel gain: mute zeroes it, speech is never ducked.
    pub fn announce_gain(&self, volume: f32, muted: bool) -> f32 {
        if muted { 0.0 } else { self.master_gain(volume) }
    }
}

impl Controller {
    pub(super) fn update_duck(&mut self) {
        let want = logic::want_duck(
            self.phase,
            self.announcing,
            self.timers_ha.ringing(),
            self.closing_earcon,
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
    fn volume_click(&self, before: (f32, bool)) {
        if !logic::want_volume_click(
            before,
            (self.st.volume, self.st.muted),
            self.phase,
            self.arbitrating,
            self.announcing,
            self.timers_ha.ringing(),
            self.closing_earcon,
        ) {
            return;
        }
        // Music is not ducked under it; its Done has no Pending entry and is ignored.
        self.deps.player.play(
            Channel::Announce,
            vec![earcons::path(self.model.earcons.volume)],
        );
    }

    /// Reads the player state as it is now: right after a command to the player it is still the
    /// previous one (as in C++); the change comes as `PlayerEvent::State`, which publishes again.
    pub(super) fn publish_media(&self) {
        let s = logic::media_state(
            self.announcing,
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
                self.announcing = true;
                self.announce_seq = self.announce_seq.next();
                let seq = self.announce_seq;
                self.update_duck();
                self.update_ui();
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
                self.sendspin_ended_ms = (!active && !taken_over).then(now_ms);
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
