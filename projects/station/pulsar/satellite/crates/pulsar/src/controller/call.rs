use pulsar_call::{CallEvent, CallPhase};
use pulsar_core::now_ms;
use pulsar_media::Channel;
use tracing::info;

use super::{Controller, earcons};

impl Controller {
    pub(super) fn on_call_event(&mut self, ev: Option<CallEvent>) {
        let Some(ev) = ev else {
            self.rx.call = None;
            return;
        };
        match ev {
            CallEvent::Incoming { .. } => {
                if self.call_active {
                    return;
                }
                self.call_active = true;
                self.call_started_ms = now_ms();
                pulsar_metrics::set_call_active(true);
                pulsar_metrics::call_incoming();
                info!("call: incoming, station answers");
                self.deps.frontend.set_streaming(true);
                self.deps.frontend.set_detection(false);
                self.deps.frontend.set_commands(false);
                self.deps.call.set_muted(self.mic_muted());
                self.call_paused_music = self.music_pause();
                self.deps.player.play(
                    Channel::Announce,
                    vec![earcons::path(self.model.earcons.timer)],
                );
                self.update_ui();
            }
            CallEvent::Active => {
                if self.call_active {
                    info!("call: media active");
                    self.deps.player.stop(Channel::Announce);
                    self.update_ui();
                }
            }
            CallEvent::Ended => {
                if !self.call_active {
                    return;
                }
                self.call_active = false;
                pulsar_metrics::set_call_active(false);
                pulsar_metrics::call_ended((now_ms() - self.call_started_ms) as f64 / 1000.0);
                info!("call: ended");
                self.deps.player.stop(Channel::Announce);
                self.deps.frontend.set_streaming(false);
                self.update_detection();
                self.update_commands();
                if self.call_paused_music {
                    self.call_paused_music = false;
                    self.music_resume();
                }
                self.update_ui();
                self.publish_media();
            }
        }
    }

    #[cfg(test)]
    pub(super) fn in_call(&self) -> bool {
        self.call_active
    }

    pub(super) fn call_phase(&self) -> CallPhase {
        self.deps.call.phase()
    }
}
