//! Сервер ESPHome Native API: открывает колонку Home Assistant как assist satellite,
//! media player и вспомогательные сущности (порт 6053; plaintext, пока не провижинен ключ
//! Noise, затем Noise). Клиентов может быть несколько (HA, просмотрщик логов, пробы):
//! состояния идут каждому подписчику, голосовой пайплайн — одному владельцу.
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

/// Привязывает порт API, читает ключ шифрования и запускает актор в текущем `LocalSet`.
pub fn spawn(
    id: DeviceIdentity,
    cfg: ApiConfig,
) -> Result<(ApiHandle, UnboundedReceiver<ApiEvent>), SpawnError> {
    server::spawn(id, cfg)
}

/// Команды актору сервера; сервер останавливается (DisconnectRequest каждому клиенту, порт
/// закрыт), когда падает последний клон.
#[derive(Clone)]
pub struct ApiHandle {
    cmds: Handle<Cmd>,
    shared: Rc<Shared>,
    /// Один `Rc` на клон handle: его падение — сигнал актору завершиться.
    live: Rc<()>,
}

impl Drop for ApiHandle {
    fn drop(&mut self) {
        if Rc::strong_count(&self.live) == 1 {
            // Актор держит свой конец канала, поэтому закрытия канала мало: шлём остановку.
            let _ = self.cmds.send(Cmd::Shutdown);
        }
    }
}

impl ApiHandle {
    /// Какой-то клиент завершил hello.
    ///
    /// В отличие от C++-сервера, ошибка записи или клиент, переставший читать, никогда не
    /// закрывают клиента внутри вызова этого handle: I/O делает актор, поэтому сразу после
    /// `voice_request_start` / `voice_request_stop` / `voice_send_audio` /
    /// `voice_announce_finished` / `publish_*` / `fire_event` геттеры ещё возвращают прежнее
    /// состояние, а исход приходит событиями (`VoiceStartFailed`, `VoiceAudioOverflow`,
    /// `VoiceSubscribed(false)`, возможно затем `VoiceSubscribed(true)`, когда пайплайн
    /// забирает другой клиент, `ClientDisconnected`). Актор обновляет геттеры до отправки
    /// этих событий.
    pub fn connected(&self) -> bool {
        self.shared.connected.get()
    }

    /// Клиент владеет голосовым пайплайном (см. `connected`, когда это меняется).
    pub fn voice_subscribed(&self) -> bool {
        self.shared.voice_subscribed.get()
    }

    /// TXT-записи `_esphomelib._tcp` для текущего состояния ключа: вычислены в `spawn`, затем
    /// актором через 100 мс после смены ключа клиентом, до `MdnsTxtChanged`.
    pub fn mdns_txt(&self) -> Vec<String> {
        self.shared.mdns_txt.borrow().clone()
    }

    /// Запускает run пайплайна (VoiceAssistantRequest start=true) у владельца голоса; пустая
    /// фраза — запуск кнопкой или продолжение разговора. Возвращает id run (1, 2, …), который
    /// несут `VoiceStartFailed` и `VoiceAudioOverflow`, если случились в этом run.
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

    /// 16 кГц моно, пока пайплайн слушает. Когда связь с владельцем отстаёт на 256 КиБ,
    /// сервер завершает run (start=false), шлёт `VoiceAudioOverflow` и отбрасывает аудио до
    /// следующего `voice_request_start`.
    pub fn voice_send_audio(&self, pcm: Vec<i16>) {
        self.send(Cmd::VoiceSendAudio(pcm));
    }

    /// Мягкий конец микрофонного аудио (`VoiceAssistantAudio.end = true`). HA трактует его как
    /// `handle_stop(False)` и завершает текущую стадию на том, что уже есть, в отличие от
    /// [`Self::voice_request_stop`], который шлёт `VoiceAssistantRequest.start = false`
    /// (abort). Детектор в контроллере шлёт не более одного за диалог.
    pub fn voice_send_audio_end(&self) {
        self.send(Cmd::VoiceAudioEnd);
    }

    /// VoiceAssistantAnnounceFinished для анонса HA.
    pub fn voice_announce_finished(&self, success: bool) {
        self.send(Cmd::VoiceAnnounceFinished(success));
    }

    /// То же сообщение для TTS-ответа текущего run (HA остаётся "responding" с TTS_START до
    /// его прихода). Как stop и audio, отбрасывается после смены владельца до следующего
    /// `voice_request_start`: команда относится к run прошлого владельца.
    pub fn voice_answer_finished(&self, success: bool) {
        self.send(Cmd::VoiceAnswerFinished(success));
    }

    /// Wake word id, сообщаемые HA как активные с этого момента.
    pub fn set_active_wake_words(&self, ids: Vec<String>) {
        self.send(Cmd::SetActiveWakeWords(ids));
    }

    // Публикация состояний: сервер хранит последнее значение каждой сущности (replay при
    // подписке на состояния) и шлёт его каждому подписчику.

    /// Громкость обрезается до 0..1.
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

    /// Отправляется, только когда Zigbee-сущности есть (`ApiConfig::zigbee_modes` непуст).
    pub fn publish_text_sensor(&self, id: TextSensorId, s: &str) {
        self.publish(Publish::TextSensor(id, s.to_owned()));
    }

    pub fn publish_light(&self, st: &LightState) {
        self.publish(Publish::Light(st.clone()));
    }

    /// Событие сущности Button ("action", "action_long", "volume_up", "volume_down");
    /// без подписчика на состояния отбрасывается, как в C++-сервере.
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
        // Не удалось только если актор уже завершился (runtime гасится).
        let _ = self.cmds.send(cmd);
    }
}
