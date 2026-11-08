//! Сообщения сущностей и голосового ассистента: команды HA, проверенные так же, как в
//! C++-сервере, превращаются в `ApiEvent`.

use std::collections::HashMap;

use esphome_proto::entity::{
    ActionId, CLOCK_TEXT_MAX, LED_SPEED_MAX, LED_SPEED_MIN, NumberId, SelectId, SwitchId, TextId,
    key,
};
use esphome_proto::pb::msg;
use esphome_proto::pb::*;
use tracing::{debug, info, warn};

use super::decode;
use super::entities::{EntityState, clamp01};
use crate::types::{
    AnnounceRequest, ApiEvent, ButtonId, LightState, MediaCommand, MediaCommandRequest, TimerEvent,
    TimerInfo, UpdateCommand, VaEvent, followup_duration_s,
};

/// LightCommandRequest без transition: default_transition_length ESPHome. Более длинные
/// обрезаются до минуты (кадры идут весь fade).
const DEFAULT_TRANSITION_MS: u32 = 1000;
const MAX_TRANSITION_MS: u32 = 60_000;

fn lossy(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

/// Текст для часов: не более CLOCK_TEXT_MAX символов того, что получит контроллер (lossy-
/// декодирование, где каждая невалидная последовательность — один U+FFFD). Для валидного
/// UTF-8 это счёт C++; для невалидного C++ считал байты, не являющиеся continuation, и
/// пропускал, например, "a" + 64 лишних continuation как один символ.
fn clock_text(b: &[u8]) -> Option<String> {
    let s = lossy(b);
    let n = s.chars().count();
    if n > CLOCK_TEXT_MAX as usize {
        warn!("api: clock text too long ({n} chars)");
        return None;
    }
    Some(s)
}

fn va_event(v: u32) -> VaEvent {
    let e = VaEvent::from_code(v);
    if let VaEvent::Unknown(_) = e {
        debug!("api: unknown voice event type {v}");
    }
    e
}

fn media_command(v: u32) -> Option<MediaCommand> {
    use MediaCommand::*;
    Some(match v {
        0 => Play,
        1 => Pause,
        2 => Stop,
        3 => Mute,
        4 => Unmute,
        5 => Toggle,
        6 => VolumeUp,
        7 => VolumeDown,
        8 => Enqueue,
        9 => RepeatOne,
        10 => RepeatOff,
        11 => ClearPlaylist,
        12 => TurnOn,
        13 => TurnOff,
        _ => return None,
    })
}

fn timer_event(v: u32) -> Option<TimerEvent> {
    Some(match v {
        0 => TimerEvent::Started,
        1 => TimerEvent::Updated,
        2 => TimerEvent::Cancelled,
        3 => TimerEvent::Finished,
        _ => return None,
    })
}

fn switch_id(key: u32) -> Option<SwitchId> {
    Some(match key {
        k if k == key::SW_MICROPHONES.get() => SwitchId::Microphones,
        k if k == key::SW_CLOCK.get() => SwitchId::Clock,
        k if k == key::SW_CLOCK_AUTO_BRI.get() => SwitchId::ClockAutoBrightness,
        k if k == key::SW_AUTO_UPDATE.get() => SwitchId::AutoUpdate,
        k if k == key::SW_NEAREST_ANSWERS.get() => SwitchId::NearestAnswers,
        k if k == key::SW_VOICE_INDICATION.get() => SwitchId::VoiceIndication,
        k if k == key::SW_WAKE_PREROLL.get() => SwitchId::WakePreroll,
        k if k == key::SW_WAKE_EVENT_RECORDING.get() => SwitchId::WakeEventRecording,
        k if k == key::SW_WAKE_DRY_RUN.get() => SwitchId::WakeDryRun,
        _ => return None,
    })
}

impl super::Server {
    /// Сообщения после транспорта.
    pub(super) fn handle_entities(&mut self, id: u64, msg_type: u32, payload: &[u8]) {
        match msg_type {
            msg::DEVICE_INFO_REQ => self.send_device_info(id),
            msg::DEVICE_CAPS_REQ => self.send_device_caps(id),
            msg::LIST_ENTITIES_REQ => self.list_entities(id),
            msg::SUBSCRIBE_STATES_REQ => {
                if let Some(c) = self.client(id) {
                    c.states = true;
                }
                self.send_all_states(id);
            }
            msg::SUBSCRIBE_HA_SERVICES_REQ | msg::SUBSCRIBE_HA_STATES_REQ | msg::VA_AUDIO => {}
            msg::MEDIA_PLAYER_CMD => self.on_media_cmd(payload),
            msg::BUTTON_CMD => self.on_button_cmd(id, payload),
            msg::SWITCH_CMD => self.on_switch_cmd(payload),
            msg::UPDATE_CMD => {
                let m: UpdateCommandRequest = decode(payload);
                if m.key == Some(key::UPDATE.get())
                    && let Some(cmd) = match m.command.unwrap_or(0) {
                        0 => Some(UpdateCommand::None),
                        1 => Some(UpdateCommand::Install),
                        2 => Some(UpdateCommand::Check),
                        _ => None,
                    }
                {
                    self.emit(ApiEvent::Update(cmd));
                }
            }
            msg::NUMBER_CMD => self.on_number_cmd(payload),
            msg::TEXT_CMD => self.on_text_cmd(payload),
            msg::SELECT_CMD => self.on_select_cmd(payload),
            msg::EXECUTE_SERVICE_REQ => self.on_execute_service(payload),
            msg::LIGHT_CMD => self.on_light_cmd(payload),
            msg::VA_EVENT_RESP => self.on_voice_event(id, payload),
            msg::VA_TIMER_EVENT_RESP => self.on_timer_event(payload),
            msg::VA_ANNOUNCE_REQ => {
                let m: VoiceAssistantAnnounceRequest = decode(payload);
                let a = AnnounceRequest {
                    media_id: m.media_id.unwrap_or_default(),
                    text: m.text.unwrap_or_default(),
                    preannounce_media_id: m.preannounce_media_id.unwrap_or_default(),
                    start_conversation: m.start_conversation.unwrap_or(false),
                };
                info!("api: announce '{}' media={}", a.text, a.media_id);
                self.emit(ApiEvent::Announce(a));
            }
            msg::VA_CONFIG_REQ => self.send_va_config(id),
            msg::VA_SET_CONFIG => self.on_va_set_config(payload),
            // HA -> устройство TTS-аудио через API; SPEAKER не объявляется.
            _ => debug!(
                "api: ignoring message type {msg_type} ({} bytes)",
                payload.len()
            ),
        }
    }

    fn on_button_cmd(&mut self, id: u64, payload: &[u8]) {
        let m: ButtonCommandRequest = decode(payload);
        let k = m.key.unwrap_or(0);
        let peer = self.client(id).map(|c| c.peer.clone()).unwrap_or_default();
        if k == key::BUTTON_WAKE_FALSE_ALARM.get() || k == key::BUTTON_WAKE_MISSED.get() {
            let b = if k == key::BUTTON_WAKE_FALSE_ALARM.get() {
                ButtonId::WakeFalseAlarm
            } else {
                ButtonId::WakeMissed
            };
            info!("api: wake-event button {k} pressed by {peer}");
            self.emit(ApiEvent::Button(b));
            return;
        }
        if k == key::BUTTON_ZIGBEE_JOIN.get() || k == key::BUTTON_ZIGBEE_LEAVE.get() {
            if !self.cfg.capabilities.zigbee {
                return;
            }
            info!("api: Zigbee button {k} pressed by {peer}");
            let b = if k == key::BUTTON_ZIGBEE_JOIN.get() {
                ButtonId::ZigbeeJoin
            } else {
                ButtonId::ZigbeeLeave
            };
            self.emit(ApiEvent::Button(b));
            return;
        }
        if k == key::BUTTON_RESTART.get() {
            warn!("api: restart requested by {peer}");
            self.emit(ApiEvent::Restart);
        }
    }

    fn on_media_cmd(&mut self, payload: &[u8]) {
        let m: MediaPlayerCommandRequest = decode(payload);
        if m.key != Some(key::MEDIA.get()) {
            return;
        }
        let command = if m.has_command.unwrap_or(false) {
            let c = media_command(m.command.unwrap_or(0));
            if c.is_none() {
                debug!("api: unknown media command {}", m.command.unwrap_or(0));
            }
            c
        } else {
            None
        };
        self.emit(ApiEvent::MediaCommand(MediaCommandRequest {
            command,
            volume: m
                .has_volume
                .unwrap_or(false)
                .then(|| clamp01(m.volume.unwrap_or(0.0))),
            media_url: m
                .has_media_url
                .unwrap_or(false)
                .then(|| m.media_url.unwrap_or_default()),
            announcement: m
                .has_announcement
                .unwrap_or(false)
                .then(|| m.announcement.unwrap_or(false)),
        }));
    }

    fn on_switch_cmd(&mut self, payload: &[u8]) {
        let m: SwitchCommandRequest = decode(payload);
        if let Some(id) = switch_id(m.key.unwrap_or(0)) {
            self.emit(ApiEvent::Switch(id, m.state.unwrap_or(false)));
        }
    }

    fn on_number_cmd(&mut self, payload: &[u8]) {
        let m: NumberCommandRequest = decode(payload);
        let v = m.state.unwrap_or(0.0);
        if v.is_nan() {
            return;
        }
        let (id, v) = match m.key.unwrap_or(0) {
            k if k == key::NUMBER_CLOCK_BRI.get() => {
                (NumberId::ClockBrightness, v.clamp(0.0, 100.0))
            }
            k if k == key::NUMBER_CLOCK_SCROLL.get() => {
                (NumberId::ClockScroll, v.clamp(100.0, 2000.0))
            }
            k if k == key::NUMBER_LED_SPEED.get() => {
                (NumberId::LedSpeed, v.clamp(LED_SPEED_MIN, LED_SPEED_MAX))
            }
            k if k == key::NUMBER_FOLLOWUP_DURATION.get() => {
                let Some(s) = followup_duration_s(v) else {
                    return;
                };
                (NumberId::FollowupDuration, s as f32)
            }
            k if k == key::NUMBER_ANNOUNCE_VOLUME.get() => {
                (NumberId::AnnounceVolume, v.clamp(0.0, 100.0))
            }
            _ => return,
        };
        self.emit(ApiEvent::Number(id, v));
    }

    fn on_text_cmd(&mut self, payload: &[u8]) {
        let m: TextCommandRequest = decode(payload);
        if m.key != Some(key::TEXT_CLOCK.get()) {
            return;
        }
        if let Some(s) = clock_text(m.state.as_deref().unwrap_or_default()) {
            self.emit(ApiEvent::Text(TextId::ClockText, s));
        }
    }

    fn is_clock_face(&self, s: &str) -> bool {
        self.cfg.clock_faces.iter().any(|o| o == s)
    }

    fn on_select_cmd(&mut self, payload: &[u8]) {
        let m: SelectCommandRequest = decode(payload);
        let s = m.state.unwrap_or_default();
        let (id, options, what) = match m.key.unwrap_or(0) {
            k if k == key::SELECT_ZIGBEE_MODE.get() => {
                (SelectId::ZigbeeMode, &self.cfg.zigbee_modes, "Zigbee role")
            }
            k if k == key::SELECT_CLOCK_MUSIC.get() => (
                SelectId::ClockMusic,
                &self.cfg.clock_music_styles,
                "clock music",
            ),
            k if k == key::SELECT_MUSIC_VIZ.get() => (
                SelectId::MusicViz,
                &self.cfg.music_styles,
                "music visualization",
            ),
            k if k == key::SELECT_CASE_COLOR.get() => {
                (SelectId::CaseColor, &self.cfg.case_colors, "case color")
            }
            k if k == key::SELECT_VOICE_COLOR.get() => {
                (SelectId::VoiceColor, &self.cfg.voice_colors, "voice color")
            }
            k if k == key::SELECT_CLOCK_FACE.get() => {
                (SelectId::ClockFace, &self.cfg.clock_faces, "clock face")
            }
            _ => return,
        };
        if !options.contains(&s) {
            warn!("api: unknown {what} '{s}'");
            return;
        }
        self.emit(ApiEvent::Select(id, s));
    }

    fn on_execute_service(&mut self, payload: &[u8]) {
        let m: ExecuteServiceRequest = decode(payload);
        let k = m.key.unwrap_or(0);
        let args = &m.args;
        let want = if k == key::ACTION_LED_SHOW.get() {
            3
        } else {
            2
        };
        if !(k == key::ACTION_CLOCK_TEXT.get()
            || k == key::ACTION_CLOCK_FACE.get()
            || k == key::ACTION_LED_SHOW.get())
            || args.len() != want
        {
            warn!("api: bad action call key={k} args={}", args.len());
            return;
        }
        if k == key::ACTION_LED_SHOW.get() {
            let effect = lossy(args[0].string_.as_deref().unwrap_or_default().as_bytes());
            if !effect.is_empty() && effect != "None" && !self.cfg.light_effects.contains(&effect) {
                warn!("api: unknown LED effect '{effect}'");
                return;
            }
            let effect = if effect == "None" {
                String::new()
            } else {
                effect
            };
            self.emit(ApiEvent::LedShow {
                effect,
                color: lossy(args[1].string_.as_deref().unwrap_or_default().as_bytes()),
                seconds: args[2].int_.or(args[2].legacy_int).unwrap_or(0),
            });
            return;
        }
        let value = args[0].string_.as_deref().unwrap_or_default();
        let seconds = args[1].int_.or(args[1].legacy_int).unwrap_or(0);
        if k == key::ACTION_CLOCK_TEXT.get() {
            let Some(value) = clock_text(value.as_bytes()) else {
                return;
            };
            self.emit(ApiEvent::Action {
                id: ActionId::ClockShowText,
                value,
                seconds,
            });
        } else {
            let face = lossy(value.as_bytes());
            if !face.is_empty() && !self.is_clock_face(&face) {
                warn!("api: unknown clock face '{face}'");
                return;
            }
            self.emit(ApiEvent::Action {
                id: ActionId::ClockShowFace,
                value: face,
                seconds,
            });
        }
    }

    fn on_light_cmd(&mut self, payload: &[u8]) {
        let m: LightCommandRequest = decode(payload);
        if m.key != Some(key::LIGHT_RING.get()) {
            return;
        }
        let bri = m.brightness.unwrap_or(0.0);
        let cbri = m.color_brightness.unwrap_or(1.0);
        let rgb = [
            m.red.unwrap_or(0.0),
            m.green.unwrap_or(0.0),
            m.blue.unwrap_or(0.0),
        ];
        // NaN дошёл бы до LED как NaN (C++-сервер пропускал, clamp01 его сохраняет):
        // отвергается так же, как NaN number.
        if [bri, cbri, rgb[0], rgb[1], rgb[2]]
            .iter()
            .any(|v| v.is_nan())
        {
            warn!("api: light command with NaN ignored");
            return;
        }
        let mut st = match self.states.get(&key::LIGHT_RING) {
            Some(EntityState::Light(l)) => l.clone(),
            _ => LightState::default(),
        };
        if m.has_state.unwrap_or(false) {
            st.on = m.state.unwrap_or(false);
        }
        if m.has_brightness.unwrap_or(false) {
            st.brightness = clamp01(bri);
        }
        if m.has_rgb.unwrap_or(false) {
            let scale = if m.has_color_brightness.unwrap_or(false) {
                clamp01(cbri)
            } else {
                1.0
            };
            st.r = clamp01(rgb[0]) * scale;
            st.g = clamp01(rgb[1]) * scale;
            st.b = clamp01(rgb[2]) * scale;
        } else if m.has_color_brightness.unwrap_or(false) {
            let max = st.r.max(st.g).max(st.b);
            let scale = if max > 0.0 { clamp01(cbri) / max } else { 0.0 };
            st.r *= scale;
            st.g *= scale;
            st.b *= scale;
        }
        if m.has_effect.unwrap_or(false) {
            let effect = m.effect.unwrap_or_default();
            st.effect = if effect == "None" {
                String::new()
            } else {
                effect
            };
        }
        let transition_ms = if m.has_transition_length.unwrap_or(false) {
            m.transition_length.unwrap_or(0).min(MAX_TRANSITION_MS)
        } else {
            DEFAULT_TRANSITION_MS
        };
        self.emit(ApiEvent::Light {
            state: st.clone(),
            transition_ms,
        });
        self.states.insert(key::LIGHT_RING, EntityState::Light(st));
        if self.states_on() {
            self.send_state(key::LIGHT_RING, None);
        }
    }

    fn on_voice_event(&mut self, id: u64, payload: &[u8]) {
        // События пайплайна считаются только от владельца.
        if self.voice.current() != Some(id) {
            return;
        }
        let m: VoiceAssistantEventResponse = decode(payload);
        let mut data = HashMap::new();
        for d in m.data {
            data.insert(d.name.unwrap_or_default(), d.value.unwrap_or_default());
        }
        let type_v = m.event_type.unwrap_or(0);
        debug!("api: voice event {type_v}");
        self.emit(ApiEvent::VoiceEvent {
            event: va_event(type_v),
            data,
        });
    }

    fn on_timer_event(&mut self, payload: &[u8]) {
        let m: VoiceAssistantTimerEventResponse = decode(payload);
        let event = m.event_type.unwrap_or(0);
        if let Some(e) = timer_event(event) {
            self.emit(ApiEvent::Timer(TimerInfo {
                event: e,
                id: m.timer_id.unwrap_or_default(),
                name: m.name.unwrap_or_default(),
                total_seconds: m.total_seconds.unwrap_or(0),
                seconds_left: m.seconds_left.unwrap_or(0),
                is_active: m.is_active.unwrap_or(false),
            }));
        } else {
            debug!("api: unknown timer event {event} dropped");
        }
    }

    fn on_va_set_config(&mut self, payload: &[u8]) {
        let m: VoiceAssistantSetConfiguration = decode(payload);
        let mut ids: Vec<String> = Vec::new();
        let (mut ignored, mut first_ignored) = (0usize, String::new());
        for ww in m.active_wake_words {
            let known = self.cfg.wake_words.iter().any(|w| w.id == ww);
            // max_active_wake_words = 1
            if known && ids.is_empty() {
                ids.push(ww);
            } else {
                if ignored == 0 {
                    first_ignored = ww;
                }
                ignored += 1;
            }
        }
        // Одна строка, сколько бы ни пришло (C++-сервер логировал каждую).
        if ignored > 0 {
            warn!("api: ignoring {ignored} wake word(s), first '{first_ignored}'");
        }
        // Спотер всегда держит одну модель: пустой / неизвестный выбор оставляет текущее слово,
        // а контроллер сообщает возврат через set_active_wake_words.
        if ids.is_empty() {
            warn!("api: no known wake word in the new configuration, keeping the current one");
            return;
        }
        self.emit(ApiEvent::ActiveWakeWords(ids));
    }
}
