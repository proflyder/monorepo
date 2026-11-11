//! Порты контроллера: то, что он вызывает у сервисов, и ничего больше.
//!
//! Сигнатуры повторяют handle-методы (sync, `&self`, те же id), поэтому продакшен-реализации —
//! тонкие обёртки, а модель исполнения (один `LocalSet`, акторы, события) не меняется. Тесты
//! подставляют фейки, которые записывают вызовы.

use std::path::PathBuf;

use pulsar_arbiter::ArbiterHandle;
use pulsar_audio::{FrontendHandle, Outcome};
use pulsar_clock::{ClockFace, ClockHandle, ClockMusicStyle};
use pulsar_core::{ClockShot, MusicFrame, UiState};
use pulsar_esphome::{
    ApiHandle, BinarySensorId, EventId, LightState, MediaState, NumberId, SelectId, SensorId,
    SwitchId, TextId, TextSensorId, UpdateState,
};
use pulsar_hw::{CaseColor, Effect, InputHandle, LedsHandle, MusicStyle, Rgb, VoiceColor};
use pulsar_mdns::MdnsHandle;
use pulsar_media::{Channel, Mixer as MixerHandle, PlayId, PlayState, PlayerHandle};
use pulsar_sendspin::{SendspinHandle, Transport};
use pulsar_update::{Release, RequestId, UpdaterHandle};
use pulsar_zigbee::{Mode, ZigbeeHandle};

use super::wake::ClaimToken;

/// Воспроизведение: каналы Music / Announce.
pub trait Player {
    fn play(&self, ch: Channel, urls: Vec<String>) -> PlayId;
    fn pause(&self, ch: Channel);
    fn resume(&self, ch: Channel);
    fn stop(&self, ch: Channel);
    fn state(&self, ch: Channel) -> PlayState;
    fn set_channel_gain(&self, ch: Channel, gain: f32);
}

/// Мастер-громкость железа (softvol) и mute.
pub trait Mixer {
    fn available(&self) -> bool;
    fn volume(&self) -> f32;
    fn set_volume(&mut self, v: f32);
    fn set_muted(&mut self, m: bool);
}

/// ESPHome API: сущности и голосовой пайплайн.
pub trait HaLink {
    fn voice_subscribed(&self) -> bool;
    fn voice_request_start(&self, wake_word_phrase: &str) -> u64;
    fn voice_request_stop(&self);
    fn voice_send_audio(&self, pcm: Vec<i16>);
    fn voice_send_audio_end(&self);
    fn voice_announce_finished(&self, success: bool);
    fn voice_answer_finished(&self, success: bool);
    fn set_active_wake_words(&self, ids: Vec<String>);
    fn mdns_txt(&self) -> Vec<String>;
    fn publish_media(&self, st: MediaState, volume: f32, muted: bool);
    fn publish_switch(&self, id: SwitchId, on: bool);
    fn publish_binary_sensor(&self, id: BinarySensorId, on: bool);
    fn publish_sensor(&self, id: SensorId, v: f32);
    fn publish_number(&self, id: NumberId, v: f32);
    fn publish_text(&self, id: TextId, s: &str);
    fn publish_select(&self, id: SelectId, s: &str);
    fn publish_text_sensor(&self, id: TextSensorId, s: &str);
    fn publish_light(&self, st: &LightState);
    fn fire_event(&self, id: EventId, event_type: &str);
    fn publish_update(&self, st: &UpdateState);
}

/// Микрофонный фронтенд: модели, детекторы, запись wake-событий.
pub trait Frontend {
    fn set_model(&self, dir: PathBuf);
    fn set_detection(&self, on: bool);
    fn set_custom_wake(&self, on: bool);
    fn set_commands(&self, on: bool);
    fn set_streaming(&self, on: bool);
    fn wake_handled(&self);
    fn set_preroll(&self, on: bool);
    fn set_music_analysis(&self, on: bool);
    fn set_mic_gain(&self, db: u32);
    fn set_wake_recording(&self, on: bool);
    fn set_wake_dry_run(&self, on: bool);
    fn wake_missed(&self);
    fn wake_event_outcome(&self, id: &str, outcome: Outcome, stt_text: Option<&str>);
    fn wake_event_false_alarm(&self);
}

/// Арбитраж wake word между станциями.
pub trait Arbiter {
    fn ok(&self) -> bool;
    fn claim(&self, wake: &str, level_db: f32, busy: bool, decide: bool) -> ClaimToken;
    fn cancel(&self);
}

/// Кольцо (и логотип Миди).
pub trait Leds {
    fn set_ui_state(&self, s: UiState);
    fn set_muted(&self, muted: bool);
    fn set_user_light(&self, on: bool, color: Rgb, brightness: f32, effect: Effect, ms: u32);
    fn show_volume(&self, v: f32);
    fn set_ambient_lux(&self, lux: f32);
    fn show_timer_started(&self);
    fn show_temporary(&self, effect: Effect, color: Rgb, seconds: i32);
    fn clear_temporary(&self);
    fn set_voice_indication(&self, on: bool);
    fn set_voice_color(&self, color: VoiceColor);
    fn set_speed(&self, k: f32);
    fn set_music_style(&self, style: MusicStyle);
    fn set_media_playing(&self, playing: bool);
    fn music_frame(&self, f: MusicFrame, from_sendspin: bool);
    fn set_case_color(&self, color: CaseColor);
}

/// Часы.
pub trait Clock {
    fn set_enabled(&self, on: bool);
    fn set_brightness(&self, b: f32);
    fn set_ambient_lux(&self, lux: f32);
    fn set_auto_brightness(&self, on: bool);
    fn set_timer(&self, ms_left: i64, running: bool);
    fn flash_number(&self, value: i32);
    fn show_code(&self, digits: String);
    fn clear_code(&self);
    fn set_text(&self, utf8: String);
    fn set_face(&self, face: ClockFace);
    fn set_scroll_ms(&self, ms: i32);
    fn show_temporary_text(&self, utf8: String, seconds: i32);
    fn show_temporary_face(&self, face: ClockFace, seconds: i32);
    fn set_ui_state(&self, s: UiState);
    fn set_voice_indication(&self, on: bool);
    fn play(&self, s: ClockShot);
    fn set_updating(&self, on: bool);
    fn set_music_style(&self, style: ClockMusicStyle);
    fn set_media_playing(&self, playing: bool);
    fn music_frame(&self, f: MusicFrame, from_sendspin: bool);
}

/// Визуальные выходы: кольцо и часы.
pub struct Display {
    /// Поля в порядке дропа сервисов: часы раньше кольца.
    pub clock: Box<dyn Clock>,
    pub leds: Box<dyn Leds>,
}

/// Обновления.
pub trait Updater {
    fn busy(&self) -> bool;
    fn check(&self) -> Option<RequestId>;
    fn install(&self, release: Release, binary: PathBuf) -> Option<RequestId>;
    fn add_extra_asset(&self, name: &str, path: PathBuf);
}

/// Zigbee-радио (роль и статус).
pub trait Radio {
    fn mode(&self) -> Mode;
    fn flashing(&self) -> bool;
    fn status(&self) -> String;
    fn join(&self);
    fn leave(&self);
}

/// Sendspin-плеер Music Assistant.
pub trait Sendspin {
    fn streaming(&self) -> bool;
    fn connected(&self) -> bool;
    fn set_duck_gain(&self, g: f32);
    fn report_volume(&self, volume: f32, muted: bool);
    fn transport(&self, t: Transport) -> bool;
    fn yield_to_local(&self);
    fn confirm_pairing_window(&self);
    fn goodbye_restart(&self);
}

/// Звонок на колонку (intercom): фаза и команды, без аудио.
pub trait Call {
    fn phase(&self) -> pulsar_call::CallPhase;
    fn muted(&self) -> bool;
    fn set_muted(&self, muted: bool);
    fn hangup(&self);
    /// Чанк очищенного VQE-потока 16 кГц.
    fn push_uplink(&self, pcm: &[i16]);
}

/// mDNS-реклама.
pub trait Mdns {
    fn set_txt(&self, service: &str, txt: Vec<String>);
}

impl Player for PlayerHandle {
    fn play(&self, ch: Channel, urls: Vec<String>) -> PlayId {
        Self::play(self, ch, urls)
    }

    fn pause(&self, ch: Channel) {
        Self::pause(self, ch);
    }

    fn resume(&self, ch: Channel) {
        Self::resume(self, ch);
    }

    fn stop(&self, ch: Channel) {
        Self::stop(self, ch);
    }

    fn state(&self, ch: Channel) -> PlayState {
        Self::state(self, ch)
    }

    fn set_channel_gain(&self, ch: Channel, gain: f32) {
        Self::set_channel_gain(self, ch, gain);
    }
}

impl Mixer for MixerHandle {
    fn available(&self) -> bool {
        Self::available(self)
    }

    fn volume(&self) -> f32 {
        Self::volume(self)
    }

    fn set_volume(&mut self, v: f32) {
        Self::set_volume(self, v);
    }

    fn set_muted(&mut self, m: bool) {
        Self::set_muted(self, m);
    }
}

impl HaLink for ApiHandle {
    fn voice_subscribed(&self) -> bool {
        Self::voice_subscribed(self)
    }

    fn voice_request_start(&self, wake_word_phrase: &str) -> u64 {
        Self::voice_request_start(self, wake_word_phrase)
    }

    fn voice_request_stop(&self) {
        Self::voice_request_stop(self);
    }

    fn voice_send_audio(&self, pcm: Vec<i16>) {
        Self::voice_send_audio(self, pcm);
    }

    fn voice_send_audio_end(&self) {
        Self::voice_send_audio_end(self);
    }

    fn voice_announce_finished(&self, success: bool) {
        Self::voice_announce_finished(self, success);
    }

    fn voice_answer_finished(&self, success: bool) {
        Self::voice_answer_finished(self, success);
    }

    fn set_active_wake_words(&self, ids: Vec<String>) {
        Self::set_active_wake_words(self, ids);
    }

    fn mdns_txt(&self) -> Vec<String> {
        Self::mdns_txt(self)
    }

    fn publish_media(&self, st: MediaState, volume: f32, muted: bool) {
        Self::publish_media(self, st, volume, muted);
    }

    fn publish_switch(&self, id: SwitchId, on: bool) {
        Self::publish_switch(self, id, on);
    }

    fn publish_binary_sensor(&self, id: BinarySensorId, on: bool) {
        Self::publish_binary_sensor(self, id, on);
    }

    fn publish_sensor(&self, id: SensorId, v: f32) {
        Self::publish_sensor(self, id, v);
    }

    fn publish_number(&self, id: NumberId, v: f32) {
        Self::publish_number(self, id, v);
    }

    fn publish_text(&self, id: TextId, s: &str) {
        Self::publish_text(self, id, s);
    }

    fn publish_select(&self, id: SelectId, s: &str) {
        Self::publish_select(self, id, s);
    }

    fn publish_text_sensor(&self, id: TextSensorId, s: &str) {
        Self::publish_text_sensor(self, id, s);
    }

    fn publish_light(&self, st: &LightState) {
        Self::publish_light(self, st);
    }

    fn fire_event(&self, id: EventId, event_type: &str) {
        Self::fire_event(self, id, event_type);
    }

    fn publish_update(&self, st: &UpdateState) {
        Self::publish_update(self, st);
    }
}

impl Frontend for FrontendHandle {
    fn set_model(&self, dir: PathBuf) {
        Self::set_model(self, dir);
    }
    fn set_detection(&self, on: bool) {
        Self::set_detection(self, on);
    }

    fn set_custom_wake(&self, on: bool) {
        Self::set_custom_wake(self, on);
    }

    fn set_commands(&self, on: bool) {
        Self::set_commands(self, on);
    }

    fn set_streaming(&self, on: bool) {
        Self::set_streaming(self, on);
    }

    fn wake_handled(&self) {
        Self::wake_handled(self);
    }

    fn set_preroll(&self, on: bool) {
        Self::set_preroll(self, on);
    }

    fn set_music_analysis(&self, on: bool) {
        Self::set_music_analysis(self, on);
    }

    fn set_mic_gain(&self, db: u32) {
        Self::set_mic_gain(self, db);
    }

    fn set_wake_recording(&self, on: bool) {
        Self::set_wake_recording(self, on);
    }

    fn set_wake_dry_run(&self, on: bool) {
        Self::set_wake_dry_run(self, on);
    }

    fn wake_missed(&self) {
        Self::wake_missed(self);
    }

    fn wake_event_outcome(&self, id: &str, outcome: Outcome, stt_text: Option<&str>) {
        Self::wake_event_outcome(self, id, outcome, stt_text);
    }

    fn wake_event_false_alarm(&self) {
        Self::wake_event_false_alarm(self);
    }
}

impl Arbiter for ArbiterHandle {
    fn ok(&self) -> bool {
        Self::ok(self)
    }

    fn claim(&self, wake: &str, level_db: f32, busy: bool, decide: bool) -> ClaimToken {
        Self::claim(self, wake, level_db, busy, decide).into()
    }

    fn cancel(&self) {
        Self::cancel(self);
    }
}

impl Leds for LedsHandle {
    fn set_ui_state(&self, s: UiState) {
        Self::set_ui_state(self, s);
    }

    fn set_muted(&self, muted: bool) {
        Self::set_muted(self, muted);
    }

    fn set_user_light(&self, on: bool, color: Rgb, brightness: f32, effect: Effect, ms: u32) {
        Self::set_user_light(self, on, color, brightness, effect, ms);
    }

    fn show_volume(&self, v: f32) {
        Self::show_volume(self, v);
    }

    fn set_ambient_lux(&self, lux: f32) {
        Self::set_ambient_lux(self, lux);
    }

    fn show_timer_started(&self) {
        Self::show_timer_started(self);
    }

    fn show_temporary(&self, effect: Effect, color: Rgb, seconds: i32) {
        Self::show_temporary(self, effect, color, seconds);
    }

    fn clear_temporary(&self) {
        Self::clear_temporary(self);
    }

    fn set_voice_indication(&self, on: bool) {
        Self::set_voice_indication(self, on);
    }

    fn set_voice_color(&self, color: VoiceColor) {
        Self::set_voice_color(self, color);
    }

    fn set_speed(&self, k: f32) {
        Self::set_speed(self, k);
    }

    fn set_music_style(&self, style: MusicStyle) {
        Self::set_music_style(self, style);
    }

    fn set_media_playing(&self, playing: bool) {
        Self::set_media_playing(self, playing);
    }

    fn music_frame(&self, f: MusicFrame, from_sendspin: bool) {
        Self::music_frame(self, f, from_sendspin);
    }

    fn set_case_color(&self, color: CaseColor) {
        Self::set_case_color(self, color);
    }
}

impl Clock for ClockHandle {
    fn set_enabled(&self, on: bool) {
        Self::set_enabled(self, on);
    }

    fn set_brightness(&self, b: f32) {
        Self::set_brightness(self, b);
    }

    fn set_ambient_lux(&self, lux: f32) {
        Self::set_ambient_lux(self, lux);
    }

    fn set_auto_brightness(&self, on: bool) {
        Self::set_auto_brightness(self, on);
    }

    fn set_timer(&self, ms_left: i64, running: bool) {
        Self::set_timer(self, ms_left, running);
    }

    fn flash_number(&self, value: i32) {
        Self::flash_number(self, value);
    }

    fn show_code(&self, digits: String) {
        Self::show_code(self, digits);
    }

    fn clear_code(&self) {
        Self::clear_code(self);
    }

    fn set_text(&self, utf8: String) {
        Self::set_text(self, utf8);
    }

    fn set_face(&self, face: ClockFace) {
        Self::set_face(self, face);
    }

    fn set_scroll_ms(&self, ms: i32) {
        Self::set_scroll_ms(self, ms);
    }

    fn show_temporary_text(&self, utf8: String, seconds: i32) {
        Self::show_temporary_text(self, utf8, seconds);
    }

    fn show_temporary_face(&self, face: ClockFace, seconds: i32) {
        Self::show_temporary_face(self, face, seconds);
    }

    fn set_ui_state(&self, s: UiState) {
        Self::set_ui_state(self, s);
    }

    fn set_voice_indication(&self, on: bool) {
        Self::set_voice_indication(self, on);
    }

    fn play(&self, s: ClockShot) {
        Self::play(self, s);
    }

    fn set_updating(&self, on: bool) {
        Self::set_updating(self, on);
    }

    fn set_music_style(&self, style: ClockMusicStyle) {
        Self::set_music_style(self, style);
    }

    fn set_media_playing(&self, playing: bool) {
        Self::set_media_playing(self, playing);
    }

    fn music_frame(&self, f: MusicFrame, from_sendspin: bool) {
        Self::music_frame(self, f, from_sendspin);
    }
}

impl Updater for UpdaterHandle {
    fn busy(&self) -> bool {
        Self::busy(self)
    }

    fn check(&self) -> Option<RequestId> {
        Self::check(self)
    }

    fn install(&self, release: Release, binary: PathBuf) -> Option<RequestId> {
        Self::install(self, release, binary)
    }

    fn add_extra_asset(&self, name: &str, path: PathBuf) {
        Self::add_extra_asset(self, name, path);
    }
}

impl Radio for ZigbeeHandle {
    fn mode(&self) -> Mode {
        Self::mode(self)
    }

    fn flashing(&self) -> bool {
        Self::flashing(self)
    }

    fn status(&self) -> String {
        Self::status(self).to_string()
    }

    fn join(&self) {
        Self::join(self);
    }

    fn leave(&self) {
        Self::leave(self);
    }
}

impl Sendspin for SendspinHandle {
    fn streaming(&self) -> bool {
        Self::streaming(self)
    }

    fn connected(&self) -> bool {
        Self::connected(self)
    }

    fn set_duck_gain(&self, g: f32) {
        Self::set_duck_gain(self, g);
    }

    fn report_volume(&self, volume: f32, muted: bool) {
        Self::report_volume(self, volume, muted);
    }

    fn transport(&self, t: Transport) -> bool {
        Self::transport(self, t)
    }

    fn yield_to_local(&self) {
        Self::yield_to_local(self);
    }

    fn confirm_pairing_window(&self) {
        Self::confirm_pairing_window(self);
    }

    fn goodbye_restart(&self) {
        Self::goodbye_restart(self);
    }
}

impl Mdns for MdnsHandle {
    fn set_txt(&self, service: &str, txt: Vec<String>) {
        Self::set_txt(self, service, txt);
    }
}

/// Everything the controller calls. Field order is the service teardown order of the C++
/// members (Sendspin goes first, the API server last).
pub struct Deps {
    pub sendspin: Option<Box<dyn Sendspin>>,
    pub radio: Option<Box<dyn Radio>>,
    pub updater: Box<dyn Updater>,
    pub arbiter: Box<dyn Arbiter>,
    /// Held only: the input actor runs while its handle exists (nothing calls into it).
    pub _input: Option<InputHandle>,
    pub display: Display,
    pub mixer: Box<dyn Mixer>,
    pub player: Box<dyn Player>,
    pub frontend: Box<dyn Frontend>,
    pub mdns: Option<Box<dyn Mdns>>,
    pub ha: Box<dyn HaLink>,
    pub call: Box<dyn Call>,
}

impl Call for pulsar_call::CallHandle {
    fn phase(&self) -> pulsar_call::CallPhase {
        Self::phase(self)
    }

    fn muted(&self) -> bool {
        Self::muted(self)
    }

    fn set_muted(&self, muted: bool) {
        Self::set_muted(self, muted);
    }

    fn hangup(&self) {
        Self::hangup(self);
    }

    fn push_uplink(&self, pcm: &[i16]) {
        Self::push_uplink(self, pcm);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real handle behind the port: `Self::method` must reach the inherent method, not the
    /// trait one (that would recurse). The host player is degraded, so `play` ends at once.
    #[test]
    fn player_port_calls_the_handle() {
        let profile = pulsar_model::profile_from_compatible(b"yandex,cucumber\0").unwrap();
        let (handle, _rx) = pulsar_media::spawn(&profile.audio.playback);
        let player: &dyn Player = &handle;
        assert_eq!(player.state(Channel::Music), PlayState::Idle);
        let id = player.play(Channel::Music, vec!["/a.mp3".into()]);
        assert_eq!(id, PlayId(1));
        player.set_channel_gain(Channel::Music, 0.5);
        player.stop(Channel::Music);
    }
}
