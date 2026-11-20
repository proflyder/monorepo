#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

mod error;
mod key;
mod server;
mod transport;
mod types;
mod voice;

use std::rc::Rc;

use home_actor::Handle;
use tokio::sync::mpsc::UnboundedReceiver;

pub use error::SpawnError;
pub use esphome_proto::entity::{
    ActionId, BinarySensorId, ButtonId, Capabilities, EventId, NumberId, SelectId, SensorId,
    SwitchId, TextId, TextSensorId,
};
pub use server::{ESPHOME_VERSION, PROJECT_NAME, mac_plain, txt_records};
pub use types::*;

use server::{Cmd, Publish, Shared};

pub fn spawn(
    id: DeviceIdentity,
    cfg: ApiConfig,
) -> Result<(ApiHandle, UnboundedReceiver<ApiEvent>), SpawnError> {
    server::spawn(id, cfg)
}

#[derive(Clone)]
pub struct ApiHandle {
    cmds: Handle<Cmd>,
    shared: Rc<Shared>,
    live: Rc<()>,
}

impl Drop for ApiHandle {
    fn drop(&mut self) {
        if Rc::strong_count(&self.live) == 1 {
            let _ = self.cmds.send(Cmd::Shutdown);
        }
    }
}

impl ApiHandle {
    pub fn connected(&self) -> bool {
        self.shared.connected.get()
    }

    pub fn voice_subscribed(&self) -> bool {
        self.shared.voice_subscribed.get()
    }

    pub fn mdns_txt(&self) -> Vec<String> {
        self.shared.mdns_txt.borrow().clone()
    }

    pub fn voice_request_start(&self, wake_word_phrase: &str) -> u64 {
        let run = self.shared.run_seq.get() + 1;
        self.shared.run_seq.set(run);
        self.send(Cmd::VoiceRequestStart {
            run,
            phrase: wake_word_phrase.to_owned(),
        });
        run
    }

    pub fn voice_request_stop(&self) {
        self.send(Cmd::VoiceRequestStop);
    }

    pub fn voice_send_audio(&self, pcm: Vec<i16>) {
        self.send(Cmd::VoiceSendAudio(pcm));
    }

    pub fn voice_send_audio_end(&self) {
        self.send(Cmd::VoiceAudioEnd);
    }

    pub fn voice_announce_finished(&self, success: bool) {
        self.send(Cmd::VoiceAnnounceFinished(success));
    }

    pub fn voice_answer_finished(&self, success: bool) {
        self.send(Cmd::VoiceAnswerFinished(success));
    }

    pub fn set_active_wake_words(&self, ids: Vec<String>) {
        self.send(Cmd::SetActiveWakeWords(ids));
    }

    pub fn publish_media(&self, st: MediaState, volume: f32, muted: bool) {
        self.publish(Publish::Media(st, volume, muted));
    }

    pub fn publish_switch(&self, id: SwitchId, on: bool) {
        self.publish(Publish::Switch(id, on));
    }

    pub fn publish_binary_sensor(&self, id: BinarySensorId, on: bool) {
        self.publish(Publish::BinarySensor(id, on));
    }

    pub fn publish_sensor(&self, id: SensorId, v: f32) {
        self.publish(Publish::Sensor(id, v));
    }

    pub fn publish_number(&self, id: NumberId, v: f32) {
        self.publish(Publish::Number(id, v));
    }

    pub fn publish_text(&self, id: TextId, s: &str) {
        self.publish(Publish::Text(id, s.to_owned()));
    }

    pub fn publish_select(&self, id: SelectId, s: &str) {
        self.publish(Publish::Select(id, s.to_owned()));
    }

    pub fn publish_text_sensor(&self, id: TextSensorId, s: &str) {
        self.publish(Publish::TextSensor(id, s.to_owned()));
    }

    pub fn publish_light(&self, st: &LightState) {
        self.publish(Publish::Light(st.clone()));
    }

    pub fn fire_event(&self, id: EventId, event_type: &str) {
        self.publish(Publish::Event(id, event_type.to_owned()));
    }

    pub fn publish_update(&self, st: &UpdateState) {
        self.publish(Publish::Update(Box::new(st.clone())));
    }

    fn publish(&self, p: Publish) {
        self.send(Cmd::Publish(p));
    }

    fn send(&self, cmd: Cmd) {
        let _ = self.cmds.send(cmd);
    }
}
