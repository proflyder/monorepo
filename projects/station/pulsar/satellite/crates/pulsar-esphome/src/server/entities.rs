//! Сущности: DeviceInfo, ListEntities, типизированные состояния (replay при SubscribeStates,
//! отправка подписчикам при publish) и 60-секундные диагностики.

use esphome_proto::entity::{
    self, COLOR_MODE_RGB, EntityKey, EntityKind, REPLAY_ORDER, VOICE_FEATURES, listed,
};
use esphome_proto::pb::msg;
use esphome_proto::pb::*;
use prost::Message;

use crate::types::{
    BinarySensorId, EventId, LightState, MediaState, NumberId, SelectId, SensorId, SwitchId,
    TextId, TextSensorId, UpdateState,
};

use super::{ESPHOME_VERSION, PROJECT_NAME, Server};

/// Снимок блокирующих диагностик, собранный вне цикла актора.
#[derive(Clone, Copy, Debug, Default)]
pub struct Diagnostics {
    pub rssi: Option<f32>,
    pub ip: Option<std::net::Ipv4Addr>,
    /// Публиковать ли снимок подписчикам (таймер) или только заполнить кэш (старт).
    pub publish: bool,
}

/// Состояние сущности; единственный источник истины для replay.
#[derive(Clone, Debug, PartialEq)]
pub enum EntityState {
    Bool {
        has: bool,
        value: bool,
    },
    Float {
        has: bool,
        value: f32,
    },
    Text {
        has: bool,
        value: String,
    },
    Media {
        state: MediaState,
        volume: f32,
        muted: bool,
    },
    Light(LightState),
    Update(UpdateState),
}

/// Вызов publish_* handle, применяемый актором.
pub enum Publish {
    Media(MediaState, f32, bool),
    Switch(SwitchId, bool),
    BinarySensor(BinarySensorId, bool),
    Sensor(SensorId, f32),
    Number(NumberId, f32),
    Text(TextId, String),
    Select(SelectId, String),
    TextSensor(TextSensorId, String),
    Light(LightState),
    Event(EventId, String),
    Update(Box<UpdateState>),
}

/// Приводит к 0..1; NaN проходит, как в C++-сервере.
pub(super) fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

/// Уровень wlan0 в dBm из /proc/net/wireless ("wlan0: 0000   58.  -52.  -256 ...").
pub(super) fn parse_wireless(text: &str) -> Option<f32> {
    text.lines().find_map(|l| {
        let mut w = l.split_whitespace();
        if w.next()? != "wlan0:" {
            return None;
        }
        u32::from_str_radix(w.next()?, 16).ok()?;
        w.next()?.parse::<f32>().ok()?;
        w.next()?.parse::<f32>().ok()
    })
}

/// `None`: всем подписчикам состояний; `Some(id)`: только этому клиенту (начальные состояния).
type To = Option<u64>;

impl Server {
    pub(super) fn states_on(&self) -> bool {
        self.clients.iter().any(|c| c.hello && c.states)
    }

    fn simply<T>(v: T, zero: bool) -> Option<T> {
        (!zero).then_some(v)
    }

    /// Отправляет состояние одной сущности; `to = None` — всем подписчикам.
    pub(super) fn send_state(&mut self, key: EntityKey, to: To) {
        let Some(def) = entity::find(key) else { return };
        if !listed(def, &self.cfg.capabilities) {
            return;
        }
        if matches!(
            def.kind,
            EntityKind::Event { .. } | EntityKind::Button { .. } | EntityKind::Action { .. }
        ) {
            return;
        }
        match key {
            entity::key::SENSOR_RSSI => {
                let v = self.diagnostics.rssi;
                self.send_float(
                    msg::SENSOR_STATE,
                    key,
                    EntityState::Float {
                        has: v.is_some(),
                        value: v.unwrap_or(0.0),
                    },
                    to,
                );
                return;
            }
            entity::key::SENSOR_UPTIME => {
                let v = self.started.elapsed().as_secs() as f32;
                self.send_float(
                    msg::SENSOR_STATE,
                    key,
                    EntityState::Float {
                        has: true,
                        value: v,
                    },
                    to,
                );
                return;
            }
            entity::key::TEXT_IP => {
                let ip = self.diagnostics.ip;
                self.send_text(
                    msg::TEXT_SENSOR_STATE,
                    key,
                    EntityState::Text {
                        has: ip.is_some(),
                        value: ip.map(|a| a.to_string()).unwrap_or_default(),
                    },
                    to,
                );
                return;
            }
            entity::key::TEXT_VERSION => {
                self.send_text(
                    msg::TEXT_SENSOR_STATE,
                    key,
                    EntityState::Text {
                        has: true,
                        value: self.id.project_version.clone(),
                    },
                    to,
                );
                return;
            }
            _ => {}
        }
        let state = self
            .states
            .get(&key)
            .cloned()
            .unwrap_or_else(|| match def.kind {
                EntityKind::MediaPlayer => EntityState::Media {
                    state: MediaState::Idle,
                    volume: 0.0,
                    muted: false,
                },
                EntityKind::Light => EntityState::Light(LightState::default()),
                EntityKind::Update => EntityState::Update(UpdateState::default()),
                EntityKind::Switch { .. } | EntityKind::BinarySensor { .. } => EntityState::Bool {
                    has: false,
                    value: false,
                },
                EntityKind::Sensor { .. } | EntityKind::Number { .. } => EntityState::Float {
                    has: false,
                    value: 0.0,
                },
                EntityKind::TextSensor { .. }
                | EntityKind::Select { .. }
                | EntityKind::Text { .. }
                | EntityKind::Event { .. }
                | EntityKind::Button { .. }
                | EntityKind::Action { .. } => EntityState::Text {
                    has: false,
                    value: String::new(),
                },
            });
        match (def.kind, state) {
            (
                EntityKind::MediaPlayer,
                EntityState::Media {
                    state,
                    volume,
                    muted,
                },
            ) => {
                let wire = state.wire();
                let m = MediaPlayerStateResponse {
                    key: Some(key.get()),
                    state: Self::simply(wire, wire == 0),
                    volume: Self::simply(clamp01(volume), volume == 0.0),
                    muted: muted.then_some(true),
                };
                self.emit_payload(msg::MEDIA_PLAYER_STATE, &m.encode_to_vec(), to);
            }
            (EntityKind::Switch { .. }, s) => self.send_bool(msg::SWITCH_STATE, key, s, to),
            (EntityKind::BinarySensor { .. }, s) => {
                self.send_bool(msg::BINARY_SENSOR_STATE, key, s, to);
            }
            (EntityKind::Sensor { .. }, s) => self.send_float(msg::SENSOR_STATE, key, s, to),
            (EntityKind::Number { .. }, s) => self.send_float(msg::NUMBER_STATE, key, s, to),
            (EntityKind::TextSensor { .. }, s) => {
                self.send_text(msg::TEXT_SENSOR_STATE, key, s, to);
            }
            (EntityKind::Select { .. }, s) => self.send_text(msg::SELECT_STATE, key, s, to),
            (EntityKind::Text { .. }, s) => self.send_text(msg::TEXT_STATE, key, s, to),
            (EntityKind::Light, EntityState::Light(l)) => {
                self.send_light(key, l, to);
            }
            (EntityKind::Update, EntityState::Update(u)) => {
                self.send_update(key, u, to);
            }
            _ => {}
        }
    }

    fn emit_payload(&mut self, msg_type: u32, payload: &[u8], to: To) {
        match to {
            Some(id) => self.send_msg(id, msg_type, payload),
            None => self.broadcast(msg_type, payload),
        }
    }

    /// Обновления состояний идут каждому клиенту, подписанному на состояния.
    pub(super) fn broadcast(&mut self, msg_type: u32, payload: &[u8]) {
        let ids: Vec<u64> = self
            .clients
            .iter()
            .filter(|c| c.hello && c.states)
            .map(|c| c.id)
            .collect();
        for id in ids {
            self.send_msg(id, msg_type, payload);
        }
    }

    #[allow(clippy::needless_pass_by_value)]
    fn send_bool(&mut self, msg_type: u32, key: EntityKey, s: EntityState, to: To) {
        let EntityState::Bool { has, value } = s else {
            return;
        };
        let missing = (!has).then_some(true);
        let payload = if msg_type == msg::SWITCH_STATE {
            SwitchStateResponse {
                key: Some(key.get()),
                state: value.then_some(true),
                missing_state: missing,
            }
            .encode_to_vec()
        } else {
            BinarySensorStateResponse {
                key: Some(key.get()),
                state: value.then_some(true),
                missing_state: missing,
            }
            .encode_to_vec()
        };
        self.emit_payload(msg_type, &payload, to);
    }

    #[allow(clippy::needless_pass_by_value)]
    fn send_float(&mut self, msg_type: u32, key: EntityKey, s: EntityState, to: To) {
        let EntityState::Float { has, value } = s else {
            return;
        };
        let missing = (!has).then_some(true);
        let payload = if msg_type == msg::NUMBER_STATE {
            NumberStateResponse {
                key: Some(key.get()),
                state: Self::simply(value, value == 0.0),
                missing_state: missing,
            }
            .encode_to_vec()
        } else {
            SensorStateResponse {
                key: Some(key.get()),
                state: Self::simply(value, value == 0.0),
                missing_state: missing,
            }
            .encode_to_vec()
        };
        self.emit_payload(msg_type, &payload, to);
    }

    #[allow(clippy::needless_pass_by_value)]
    fn send_text(&mut self, msg_type: u32, key: EntityKey, s: EntityState, to: To) {
        let EntityState::Text { has, value } = s else {
            return;
        };
        let missing = (!has).then_some(true);
        let state = (!value.is_empty()).then_some(value);
        let payload = match msg_type {
            msg::TEXT_SENSOR_STATE => TextSensorStateResponse {
                key: Some(key.get()),
                state,
                missing_state: missing,
            }
            .encode_to_vec(),
            msg::SELECT_STATE => SelectStateResponse {
                key: Some(key.get()),
                state,
                missing_state: missing,
            }
            .encode_to_vec(),
            _ => TextStateResponse {
                key: Some(key.get()),
                state,
                missing_state: missing,
            }
            .encode_to_vec(),
        };
        self.emit_payload(msg_type, &payload, to);
    }

    fn send_light(&mut self, key: EntityKey, l: LightState, to: To) {
        // HA ждёт rgb, нормированный к максимуму 1, а масштаб — в color_brightness.
        let m = l.r.max(l.g).max(l.b);
        let norm = |v: f32| if m > 0.0 { v / m } else { 1.0 };
        let msg = LightStateResponse {
            key: key.get(),
            state: l.on,
            brightness: l.brightness,
            color_brightness: if m > 0.0 { m } else { 1.0 },
            color_mode: COLOR_MODE_RGB,
            red: norm(l.r),
            green: norm(l.g),
            blue: norm(l.b),
            effect: Some(if l.effect.is_empty() {
                "None".to_owned()
            } else {
                l.effect
            }),
        };
        self.emit_payload(msg::LIGHT_STATE, &msg.encode_to_vec(), to);
    }

    fn send_update(&mut self, key: EntityKey, u: UpdateState, to: To) {
        let m = UpdateStateResponse {
            key: Some(key.get()),
            missing_state: (u == UpdateState::default()).then_some(true),
            in_progress: u.in_progress.then_some(true),
            has_progress: u.has_progress.then_some(true),
            progress: Self::simply(u.progress, u.progress == 0.0),
            current_version: (!u.current_version.is_empty()).then_some(u.current_version),
            latest_version: (!u.latest_version.is_empty()).then_some(u.latest_version),
            title: (!u.title.is_empty()).then_some(u.title),
            release_url: (!u.release_url.is_empty()).then_some(u.release_url),
        };
        self.emit_payload(msg::UPDATE_STATE, &m.encode_to_vec(), to);
    }

    pub(super) fn send_all_states(&mut self, id: u64) {
        for key in REPLAY_ORDER {
            if entity::find(*key).is_some_and(|d| listed(d, &self.cfg.capabilities)) {
                self.send_state(*key, Some(id));
            }
        }
    }

    pub(super) fn send_device_info(&mut self, id: u64) {
        let supported = self.cfg.key_path.is_some();
        let m = DeviceInfoResponse {
            name: Some(self.id.name.clone()),
            mac_address: Some(self.id.mac.clone()),
            esphome_version: Some(ESPHOME_VERSION.to_owned()),
            compilation_time: Some(self.id.compilation_time.clone()),
            model: Some(self.id.model.clone()),
            project_name: Some(PROJECT_NAME.to_owned()),
            project_version: Some(self.id.project_version.clone()),
            manufacturer: Some(self.id.manufacturer.clone()),
            friendly_name: Some(self.id.friendly_name.clone()),
            voice_assistant_feature_flags: Some(VOICE_FEATURES),
            api_encryption_supported: supported.then_some(true),
            api_encryption_provisionable: (supported && !self.key.has_key).then_some(true),
        };
        self.send_msg(id, msg::DEVICE_INFO_RESP, &m.encode_to_vec());
    }

    pub(super) fn send_device_caps(&mut self, id: u64) {
        let m = DeviceCapabilitiesResponse {
            voice_assistant: Some(VoiceAssistantCapabilities {
                feature_flags: Some(VOICE_FEATURES),
            }),
        };
        self.send_msg(id, msg::DEVICE_CAPS_RESP, &m.encode_to_vec());
    }

    pub(super) fn list_entities(&mut self, id: u64) {
        let opts = self.cfg.entity_options();
        for m in entity::list_entities(&self.cfg.capabilities, &opts) {
            self.send_msg(id, m.msg_type, &m.payload);
        }
    }

    pub(super) fn send_va_config(&mut self, id: u64) {
        let m = VoiceAssistantConfigurationResponse {
            available_wake_words: self
                .cfg
                .wake_words
                .iter()
                .map(|w| VoiceAssistantWakeWord {
                    id: (!w.id.is_empty()).then(|| w.id.clone()),
                    wake_word: (!w.phrase.is_empty()).then(|| w.phrase.clone()),
                    trained_languages: w.languages.clone(),
                })
                .collect(),
            active_wake_words: self.cfg.active_wake_words.clone(),
            max_active_wake_words: Some(1),
        };
        self.send_msg(id, msg::VA_CONFIG_RESP, &m.encode_to_vec());
    }

    pub(super) fn send_diagnostics(&mut self, to: To, ip_changed: bool) {
        self.send_state(entity::key::SENSOR_RSSI, to);
        self.send_state(entity::key::SENSOR_UPTIME, to);
        if ip_changed {
            self.send_state(entity::key::TEXT_IP, to);
        }
    }

    // ------------------------------------------------------------ publish (handle commands)

    pub(super) fn publish(&mut self, p: Publish) {
        let on = self.states_on();
        match p {
            Publish::Media(state, volume, muted) => {
                self.states.insert(
                    entity::key::MEDIA,
                    EntityState::Media {
                        state,
                        volume: clamp01(volume),
                        muted,
                    },
                );
                if on {
                    self.send_state(entity::key::MEDIA, None);
                }
            }
            Publish::Switch(id, value) => {
                let key = id.key();
                if !self.listed(key) {
                    return;
                }
                self.states
                    .insert(key, EntityState::Bool { has: true, value });
                if on {
                    self.send_state(key, None);
                }
            }
            Publish::BinarySensor(id, value) => {
                let key = id.key();
                if !self.listed(key) {
                    return;
                }
                self.states
                    .insert(key, EntityState::Bool { has: true, value });
                if on {
                    self.send_state(key, None);
                }
            }
            Publish::Sensor(id, value) => {
                let key = id.key();
                if !self.listed(key) {
                    return;
                }
                self.states
                    .insert(key, EntityState::Float { has: true, value });
                if on {
                    self.send_state(key, None);
                }
            }
            Publish::Number(id, value) => {
                let key = id.key();
                if !self.listed(key) {
                    return;
                }
                self.states
                    .insert(key, EntityState::Float { has: true, value });
                if on {
                    self.send_state(key, None);
                }
            }
            Publish::Text(id, value) => {
                let key = id.key();
                if !self.listed(key) {
                    return;
                }
                self.states
                    .insert(key, EntityState::Text { has: true, value });
                if on {
                    self.send_state(key, None);
                }
            }
            Publish::Select(id, value) => {
                let key = id.key();
                if !self.listed(key) {
                    return;
                }
                self.states
                    .insert(key, EntityState::Text { has: true, value });
                if on {
                    self.send_state(key, None);
                }
            }
            Publish::TextSensor(id, value) => {
                let key = id.key();
                if !self.listed(key) {
                    return;
                }
                self.states
                    .insert(key, EntityState::Text { has: true, value });
                if on {
                    self.send_state(key, None);
                }
            }
            Publish::Light(state) => {
                let key = entity::key::LIGHT_RING;
                self.states.insert(key, EntityState::Light(state));
                if on {
                    self.send_state(key, None);
                }
            }
            Publish::Event(id, event_type) => {
                let key = id.key();
                if !on || !self.listed(key) {
                    return;
                }
                let m = EventResponse {
                    key: Some(key.get()),
                    event_type: Some(event_type),
                };
                self.broadcast(msg::EVENT_RESP, &m.encode_to_vec());
            }
            Publish::Update(state) => {
                let key = entity::key::UPDATE;
                self.states
                    .insert(key, EntityState::Update((*state).clone()));
                if on {
                    self.send_state(key, None);
                }
            }
        }
    }

    fn listed(&self, key: EntityKey) -> bool {
        entity::find(key).is_some_and(|d| listed(d, &self.cfg.capabilities))
    }
}
