//! protobuf-сообщения ESPHome Native API, написанные вручную на prost.
//!
//! Объявлены только сообщения, которые сервер отправляет или читает; номера полей и типы — из
//! vendored `api.proto` (файл `api.proto` в этом крейте). Скалярные поля, которые C++-референс
//! писал даже при значении protobuf по умолчанию (его `Writer::… force = true`: заголовки
//! сущностей, уровни логов, пустые строки опций), объявлены `Option<T>`: `Some(default)`
//! сохраняет ноль на проводе, а обычное поле было бы опущено.

use prost::Message;

pub mod msg;

/// `key` пишется всегда: HA ведёт по нему реестр сущностей.
#[derive(Clone, PartialEq, Eq, Message)]
pub struct HelloRequest {
    #[prost(string, optional, tag = "1")]
    pub client_info: Option<String>,
    #[prost(uint32, optional, tag = "2")]
    pub api_version_major: Option<u32>,
    #[prost(uint32, optional, tag = "3")]
    pub api_version_minor: Option<u32>,
}

#[derive(Clone, PartialEq, Eq, Message)]
pub struct HelloResponse {
    #[prost(uint32, optional, tag = "1")]
    pub api_version_major: Option<u32>,
    #[prost(uint32, optional, tag = "2")]
    pub api_version_minor: Option<u32>,
    #[prost(string, optional, tag = "3")]
    pub server_info: Option<String>,
    #[prost(string, optional, tag = "4")]
    pub name: Option<String>,
}

#[derive(Clone, PartialEq, Eq, Message)]
pub struct DisconnectRequest {}

#[derive(Clone, PartialEq, Eq, Message)]
pub struct DisconnectResponse {}

#[derive(Clone, PartialEq, Eq, Message)]
pub struct PingRequest {}

#[derive(Clone, PartialEq, Eq, Message)]
pub struct PingResponse {}

#[derive(Clone, PartialEq, Eq, Message)]
pub struct DeviceInfoRequest {}

#[derive(Clone, PartialEq, Eq, Message)]
pub struct DeviceInfoResponse {
    #[prost(string, optional, tag = "2")]
    pub name: Option<String>,
    #[prost(string, optional, tag = "3")]
    pub mac_address: Option<String>,
    #[prost(string, optional, tag = "4")]
    pub esphome_version: Option<String>,
    #[prost(string, optional, tag = "5")]
    pub compilation_time: Option<String>,
    #[prost(string, optional, tag = "6")]
    pub model: Option<String>,
    #[prost(string, optional, tag = "8")]
    pub project_name: Option<String>,
    #[prost(string, optional, tag = "9")]
    pub project_version: Option<String>,
    #[prost(string, optional, tag = "12")]
    pub manufacturer: Option<String>,
    #[prost(string, optional, tag = "13")]
    pub friendly_name: Option<String>,
    #[prost(uint32, optional, tag = "17")]
    pub voice_assistant_feature_flags: Option<u32>,
    #[prost(bool, optional, tag = "19")]
    pub api_encryption_supported: Option<bool>,
    #[prost(bool, optional, tag = "26")]
    pub api_encryption_provisionable: Option<bool>,
}

#[derive(Clone, PartialEq, Eq, Message)]
pub struct VoiceAssistantCapabilities {
    #[prost(uint32, optional, tag = "1")]
    pub feature_flags: Option<u32>,
}

#[derive(Clone, PartialEq, Eq, Message)]
pub struct DeviceCapabilitiesRequest {}

#[derive(Clone, PartialEq, Eq, Message)]
pub struct DeviceCapabilitiesResponse {
    #[prost(message, optional, tag = "2")]
    pub voice_assistant: Option<VoiceAssistantCapabilities>,
}

#[derive(Clone, PartialEq, Eq, Message)]
pub struct ListEntitiesRequest {}

#[derive(Clone, PartialEq, Eq, Message)]
pub struct ListEntitiesDoneResponse {}

#[derive(Clone, PartialEq, Eq, Message)]
pub struct SubscribeStatesRequest {}

#[derive(Clone, PartialEq, Eq, Message)]
pub struct SubscribeHomeassistantServicesRequest {}

#[derive(Clone, PartialEq, Eq, Message)]
pub struct SubscribeHomeAssistantStatesRequest {}

#[derive(Clone, PartialEq, Eq, Message)]
pub struct BinarySensorStateResponse {
    #[prost(fixed32, optional, tag = "1")]
    pub key: Option<u32>,
    #[prost(bool, optional, tag = "2")]
    pub state: Option<bool>,
    #[prost(bool, optional, tag = "3")]
    pub missing_state: Option<bool>,
}

/// LightStateResponse. Порядок полей — как у C++-сервера (`1,2,3,10,11,4,5,6,9`), а не по
/// тегам: golden-захват референса сравнивается байт в байт.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LightStateResponse {
    pub key: u32,
    pub state: bool,
    pub brightness: f32,
    pub color_brightness: f32,
    pub color_mode: u32,
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub effect: Option<String>,
}

impl Message for LightStateResponse {
    fn encode_raw(&self, buf: &mut impl prost::bytes::BufMut) {
        use prost::encoding::{bool as e_bool, fixed32, float, string, uint32};
        fixed32::encode(1, &self.key, buf);
        if self.state {
            e_bool::encode(2, &self.state, buf);
        }
        if self.brightness != 0.0 {
            float::encode(3, &self.brightness, buf);
        }
        if self.color_brightness != 0.0 {
            float::encode(10, &self.color_brightness, buf);
        }
        if self.color_mode != 0 {
            uint32::encode(11, &self.color_mode, buf);
        }
        if self.red != 0.0 {
            float::encode(4, &self.red, buf);
        }
        if self.green != 0.0 {
            float::encode(5, &self.green, buf);
        }
        if self.blue != 0.0 {
            float::encode(6, &self.blue, buf);
        }
        if let Some(effect) = &self.effect {
            string::encode(9, effect, buf);
        }
    }

    fn merge_field(
        &mut self,
        tag: u32,
        wire_type: prost::encoding::WireType,
        buf: &mut impl prost::bytes::Buf,
        ctx: prost::encoding::DecodeContext,
    ) -> Result<(), prost::DecodeError> {
        use prost::encoding::{bool as e_bool, fixed32, float, skip_field, string, uint32};
        match tag {
            1 => fixed32::merge(wire_type, &mut self.key, buf, ctx),
            2 => e_bool::merge(wire_type, &mut self.state, buf, ctx),
            3 => float::merge(wire_type, &mut self.brightness, buf, ctx),
            10 => float::merge(wire_type, &mut self.color_brightness, buf, ctx),
            11 => uint32::merge(wire_type, &mut self.color_mode, buf, ctx),
            4 => float::merge(wire_type, &mut self.red, buf, ctx),
            5 => float::merge(wire_type, &mut self.green, buf, ctx),
            6 => float::merge(wire_type, &mut self.blue, buf, ctx),
            9 => string::merge(
                wire_type,
                self.effect.get_or_insert_with(String::new),
                buf,
                ctx,
            ),
            _ => skip_field(wire_type, tag, buf, ctx),
        }
    }

    fn encoded_len(&self) -> usize {
        use prost::encoding::{bool as e_bool, fixed32, float, string, uint32};
        let mut len = fixed32::encoded_len(1, &self.key);
        if self.state {
            len += e_bool::encoded_len(2, &self.state);
        }
        if self.brightness != 0.0 {
            len += float::encoded_len(3, &self.brightness);
        }
        if self.color_brightness != 0.0 {
            len += float::encoded_len(10, &self.color_brightness);
        }
        if self.color_mode != 0 {
            len += uint32::encoded_len(11, &self.color_mode);
        }
        if self.red != 0.0 {
            len += float::encoded_len(4, &self.red);
        }
        if self.green != 0.0 {
            len += float::encoded_len(5, &self.green);
        }
        if self.blue != 0.0 {
            len += float::encoded_len(6, &self.blue);
        }
        if let Some(effect) = &self.effect {
            len += string::encoded_len(9, effect);
        }
        len
    }

    fn clear(&mut self) {
        *self = Self::default();
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct SensorStateResponse {
    #[prost(fixed32, optional, tag = "1")]
    pub key: Option<u32>,
    #[prost(float, optional, tag = "2")]
    pub state: Option<f32>,
    #[prost(bool, optional, tag = "3")]
    pub missing_state: Option<bool>,
}

#[derive(Clone, PartialEq, Message)]
pub struct SwitchStateResponse {
    #[prost(fixed32, optional, tag = "1")]
    pub key: Option<u32>,
    #[prost(bool, optional, tag = "2")]
    pub state: Option<bool>,
    #[prost(bool, optional, tag = "4")]
    pub missing_state: Option<bool>,
}

#[derive(Clone, PartialEq, Message)]
pub struct TextSensorStateResponse {
    #[prost(fixed32, optional, tag = "1")]
    pub key: Option<u32>,
    #[prost(string, optional, tag = "2")]
    pub state: Option<String>,
    #[prost(bool, optional, tag = "3")]
    pub missing_state: Option<bool>,
}

#[derive(Clone, PartialEq, Message)]
pub struct SubscribeLogsRequest {
    #[prost(uint32, optional, tag = "1")]
    pub level: Option<u32>,
    #[prost(bool, optional, tag = "2")]
    pub dump_config: Option<bool>,
}

#[derive(Clone, PartialEq, Message)]
pub struct SubscribeLogsResponse {
    #[prost(uint32, optional, tag = "1")]
    pub level: Option<u32>,
    #[prost(bytes = "vec", optional, tag = "3")]
    pub message: Option<Vec<u8>>,
}

#[derive(Clone, PartialEq, Message)]
pub struct LightCommandRequest {
    #[prost(fixed32, optional, tag = "1")]
    pub key: Option<u32>,
    #[prost(bool, optional, tag = "2")]
    pub has_state: Option<bool>,
    #[prost(bool, optional, tag = "3")]
    pub state: Option<bool>,
    #[prost(bool, optional, tag = "4")]
    pub has_brightness: Option<bool>,
    #[prost(float, optional, tag = "5")]
    pub brightness: Option<f32>,
    #[prost(bool, optional, tag = "20")]
    pub has_color_brightness: Option<bool>,
    #[prost(float, optional, tag = "21")]
    pub color_brightness: Option<f32>,
    #[prost(bool, optional, tag = "6")]
    pub has_rgb: Option<bool>,
    #[prost(float, optional, tag = "7")]
    pub red: Option<f32>,
    #[prost(float, optional, tag = "8")]
    pub green: Option<f32>,
    #[prost(float, optional, tag = "9")]
    pub blue: Option<f32>,
    #[prost(bool, optional, tag = "18")]
    pub has_effect: Option<bool>,
    #[prost(string, optional, tag = "19")]
    pub effect: Option<String>,
    #[prost(bool, optional, tag = "14")]
    pub has_transition_length: Option<bool>,
    #[prost(uint32, optional, tag = "15")]
    pub transition_length: Option<u32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct SwitchCommandRequest {
    #[prost(fixed32, optional, tag = "1")]
    pub key: Option<u32>,
    #[prost(bool, optional, tag = "2")]
    pub state: Option<bool>,
}

#[derive(Clone, PartialEq, Message)]
pub struct GetTimeRequest {}

#[derive(Clone, PartialEq, Message)]
pub struct GetTimeResponse {
    #[prost(fixed32, optional, tag = "1")]
    pub epoch_seconds: Option<u32>,
    #[prost(string, optional, tag = "2")]
    pub timezone: Option<String>,
    #[prost(message, optional, tag = "3")]
    pub parsed_timezone: Option<ParsedTimezone>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ParsedTimezone {
    #[prost(sint32, optional, tag = "1")]
    pub std_offset_seconds: Option<i32>,
    #[prost(message, optional, tag = "3")]
    pub dst_start: Option<DstRule>,
    #[prost(message, optional, tag = "4")]
    pub dst_end: Option<DstRule>,
}

#[derive(Clone, PartialEq, Message)]
pub struct DstRule {
    #[prost(uint32, optional, tag = "3")]
    pub r#type: Option<u32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ListEntitiesBinarySensorResponse {
    #[prost(string, optional, tag = "1")]
    pub object_id: Option<String>,
    #[prost(fixed32, optional, tag = "2")]
    pub key: Option<u32>,
    #[prost(string, optional, tag = "3")]
    pub name: Option<String>,
    #[prost(string, optional, tag = "5")]
    pub device_class: Option<String>,
    #[prost(string, optional, tag = "8")]
    pub icon: Option<String>,
    #[prost(uint32, optional, tag = "9")]
    pub entity_category: Option<u32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ListEntitiesLightResponse {
    #[prost(string, optional, tag = "1")]
    pub object_id: Option<String>,
    #[prost(fixed32, optional, tag = "2")]
    pub key: Option<u32>,
    #[prost(string, optional, tag = "3")]
    pub name: Option<String>,
    #[prost(string, repeated, tag = "11")]
    pub effects: Vec<String>,
    #[prost(uint32, repeated, packed = "true", tag = "12")]
    pub supported_color_modes: Vec<u32>,
    #[prost(string, optional, tag = "14")]
    pub icon: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ListEntitiesSensorResponse {
    #[prost(string, optional, tag = "1")]
    pub object_id: Option<String>,
    #[prost(fixed32, optional, tag = "2")]
    pub key: Option<u32>,
    #[prost(string, optional, tag = "3")]
    pub name: Option<String>,
    #[prost(string, optional, tag = "6")]
    pub unit_of_measurement: Option<String>,
    #[prost(int32, optional, tag = "7")]
    pub accuracy_decimals: Option<i32>,
    #[prost(string, optional, tag = "9")]
    pub device_class: Option<String>,
    #[prost(uint32, optional, tag = "10")]
    pub state_class: Option<u32>,
    #[prost(uint32, optional, tag = "13")]
    pub entity_category: Option<u32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ListEntitiesSwitchResponse {
    #[prost(string, optional, tag = "1")]
    pub object_id: Option<String>,
    #[prost(fixed32, optional, tag = "2")]
    pub key: Option<u32>,
    #[prost(string, optional, tag = "3")]
    pub name: Option<String>,
    #[prost(string, optional, tag = "5")]
    pub icon: Option<String>,
    #[prost(uint32, optional, tag = "8")]
    pub entity_category: Option<u32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ListEntitiesTextSensorResponse {
    #[prost(string, optional, tag = "1")]
    pub object_id: Option<String>,
    #[prost(fixed32, optional, tag = "2")]
    pub key: Option<u32>,
    #[prost(string, optional, tag = "3")]
    pub name: Option<String>,
    #[prost(string, optional, tag = "5")]
    pub icon: Option<String>,
    #[prost(uint32, optional, tag = "7")]
    pub entity_category: Option<u32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ListEntitiesNumberResponse {
    #[prost(string, optional, tag = "1")]
    pub object_id: Option<String>,
    #[prost(fixed32, optional, tag = "2")]
    pub key: Option<u32>,
    #[prost(string, optional, tag = "3")]
    pub name: Option<String>,
    #[prost(string, optional, tag = "5")]
    pub icon: Option<String>,
    #[prost(float, tag = "6")]
    pub min_value: f32,
    #[prost(float, tag = "7")]
    pub max_value: f32,
    #[prost(float, tag = "8")]
    pub step: f32,
    #[prost(uint32, optional, tag = "10")]
    pub entity_category: Option<u32>,
    #[prost(string, optional, tag = "11")]
    pub unit_of_measurement: Option<String>,
    #[prost(uint32, optional, tag = "12")]
    pub mode: Option<u32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct NumberStateResponse {
    #[prost(fixed32, optional, tag = "1")]
    pub key: Option<u32>,
    #[prost(float, optional, tag = "2")]
    pub state: Option<f32>,
    #[prost(bool, optional, tag = "3")]
    pub missing_state: Option<bool>,
}

#[derive(Clone, PartialEq, Message)]
pub struct NumberCommandRequest {
    #[prost(fixed32, optional, tag = "1")]
    pub key: Option<u32>,
    #[prost(float, optional, tag = "2")]
    pub state: Option<f32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ListEntitiesSelectResponse {
    #[prost(string, optional, tag = "1")]
    pub object_id: Option<String>,
    #[prost(fixed32, optional, tag = "2")]
    pub key: Option<u32>,
    #[prost(string, optional, tag = "3")]
    pub name: Option<String>,
    #[prost(string, optional, tag = "5")]
    pub icon: Option<String>,
    #[prost(string, repeated, tag = "6")]
    pub options: Vec<String>,
    #[prost(uint32, optional, tag = "8")]
    pub entity_category: Option<u32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct SelectStateResponse {
    #[prost(fixed32, optional, tag = "1")]
    pub key: Option<u32>,
    #[prost(string, optional, tag = "2")]
    pub state: Option<String>,
    #[prost(bool, optional, tag = "3")]
    pub missing_state: Option<bool>,
}

#[derive(Clone, PartialEq, Message)]
pub struct SelectCommandRequest {
    #[prost(fixed32, optional, tag = "1")]
    pub key: Option<u32>,
    #[prost(string, optional, tag = "2")]
    pub state: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ListEntitiesButtonResponse {
    #[prost(string, optional, tag = "1")]
    pub object_id: Option<String>,
    #[prost(fixed32, optional, tag = "2")]
    pub key: Option<u32>,
    #[prost(string, optional, tag = "3")]
    pub name: Option<String>,
    #[prost(string, optional, tag = "5")]
    pub icon: Option<String>,
    #[prost(uint32, optional, tag = "7")]
    pub entity_category: Option<u32>,
    #[prost(string, optional, tag = "8")]
    pub device_class: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ButtonCommandRequest {
    #[prost(fixed32, optional, tag = "1")]
    pub key: Option<u32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct MediaPlayerSupportedFormat {
    #[prost(string, optional, tag = "1")]
    pub format: Option<String>,
    #[prost(uint32, optional, tag = "2")]
    pub sample_rate: Option<u32>,
    #[prost(uint32, optional, tag = "3")]
    pub num_channels: Option<u32>,
    #[prost(uint32, tag = "4")]
    pub purpose: u32,
    #[prost(uint32, optional, tag = "5")]
    pub sample_bytes: Option<u32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ListEntitiesMediaPlayerResponse {
    #[prost(string, optional, tag = "1")]
    pub object_id: Option<String>,
    #[prost(fixed32, optional, tag = "2")]
    pub key: Option<u32>,
    #[prost(string, optional, tag = "3")]
    pub name: Option<String>,
    #[prost(bool, optional, tag = "8")]
    pub supports_pause: Option<bool>,
    #[prost(message, repeated, tag = "9")]
    pub supported_formats: Vec<MediaPlayerSupportedFormat>,
    #[prost(uint32, optional, tag = "11")]
    pub feature_flags: Option<u32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct MediaPlayerStateResponse {
    #[prost(fixed32, optional, tag = "1")]
    pub key: Option<u32>,
    #[prost(uint32, optional, tag = "2")]
    pub state: Option<u32>,
    #[prost(float, optional, tag = "3")]
    pub volume: Option<f32>,
    #[prost(bool, optional, tag = "4")]
    pub muted: Option<bool>,
}

#[derive(Clone, PartialEq, Message)]
pub struct MediaPlayerCommandRequest {
    #[prost(fixed32, optional, tag = "1")]
    pub key: Option<u32>,
    #[prost(bool, optional, tag = "2")]
    pub has_command: Option<bool>,
    #[prost(uint32, optional, tag = "3")]
    pub command: Option<u32>,
    #[prost(bool, optional, tag = "4")]
    pub has_volume: Option<bool>,
    #[prost(float, optional, tag = "5")]
    pub volume: Option<f32>,
    #[prost(bool, optional, tag = "6")]
    pub has_media_url: Option<bool>,
    #[prost(string, optional, tag = "7")]
    pub media_url: Option<String>,
    #[prost(bool, optional, tag = "8")]
    pub has_announcement: Option<bool>,
    #[prost(bool, optional, tag = "9")]
    pub announcement: Option<bool>,
}

#[derive(Clone, PartialEq, Message)]
pub struct SubscribeVoiceAssistantRequest {
    #[prost(bool, optional, tag = "1")]
    pub subscribe: Option<bool>,
    #[prost(uint32, optional, tag = "2")]
    pub flags: Option<u32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct VoiceAssistantAudioSettings {
    #[prost(uint32, optional, tag = "1")]
    pub noise_suppression_level: Option<u32>,
    #[prost(uint32, optional, tag = "2")]
    pub auto_gain: Option<u32>,
    #[prost(float, optional, tag = "3")]
    pub volume_multiplier: Option<f32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct VoiceAssistantRequest {
    #[prost(bool, optional, tag = "1")]
    pub start: Option<bool>,
    #[prost(uint32, optional, tag = "3")]
    pub flags: Option<u32>,
    #[prost(message, optional, tag = "4")]
    pub audio_settings: Option<VoiceAssistantAudioSettings>,
    #[prost(string, optional, tag = "5")]
    pub wake_word_phrase: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct VoiceAssistantResponse {
    #[prost(uint32, optional, tag = "1")]
    pub port: Option<u32>,
    #[prost(bool, optional, tag = "2")]
    pub error: Option<bool>,
}

#[derive(Clone, PartialEq, Message)]
pub struct VoiceAssistantEventData {
    #[prost(string, optional, tag = "1")]
    pub name: Option<String>,
    #[prost(string, optional, tag = "2")]
    pub value: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct VoiceAssistantEventResponse {
    #[prost(uint32, optional, tag = "1")]
    pub event_type: Option<u32>,
    #[prost(message, repeated, tag = "2")]
    pub data: Vec<VoiceAssistantEventData>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ListEntitiesTextResponse {
    #[prost(string, optional, tag = "1")]
    pub object_id: Option<String>,
    #[prost(fixed32, optional, tag = "2")]
    pub key: Option<u32>,
    #[prost(string, optional, tag = "3")]
    pub name: Option<String>,
    #[prost(string, optional, tag = "5")]
    pub icon: Option<String>,
    #[prost(uint32, optional, tag = "9")]
    pub max_length: Option<u32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct TextStateResponse {
    #[prost(fixed32, optional, tag = "1")]
    pub key: Option<u32>,
    #[prost(string, optional, tag = "2")]
    pub state: Option<String>,
    #[prost(bool, optional, tag = "3")]
    pub missing_state: Option<bool>,
}

#[derive(Clone, PartialEq, Message)]
pub struct TextCommandRequest {
    #[prost(fixed32, optional, tag = "1")]
    pub key: Option<u32>,
    /// Байты, а не `String`: C++-сервер считал длину «Clock text» по тому, что получил, и
    /// невалидный UTF-8 проходил в контроллер как lossy-текст (см. `commands::clock_text`).
    #[prost(bytes = "vec", optional, tag = "2")]
    pub state: Option<Vec<u8>>,
}

#[derive(Clone, PartialEq, Message)]
pub struct VoiceAssistantAudio {
    #[prost(bytes = "vec", optional, tag = "1")]
    pub data: Option<Vec<u8>>,
    #[prost(bool, optional, tag = "2")]
    pub end: Option<bool>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ListEntitiesEventResponse {
    #[prost(string, optional, tag = "1")]
    pub object_id: Option<String>,
    #[prost(fixed32, optional, tag = "2")]
    pub key: Option<u32>,
    #[prost(string, optional, tag = "3")]
    pub name: Option<String>,
    #[prost(string, optional, tag = "5")]
    pub icon: Option<String>,
    #[prost(string, optional, tag = "8")]
    pub device_class: Option<String>,
    #[prost(string, repeated, tag = "9")]
    pub event_types: Vec<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct EventResponse {
    #[prost(fixed32, optional, tag = "1")]
    pub key: Option<u32>,
    #[prost(string, optional, tag = "2")]
    pub event_type: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct VoiceAssistantTimerEventResponse {
    #[prost(uint32, optional, tag = "1")]
    pub event_type: Option<u32>,
    #[prost(string, optional, tag = "2")]
    pub timer_id: Option<String>,
    #[prost(string, optional, tag = "3")]
    pub name: Option<String>,
    #[prost(uint32, optional, tag = "4")]
    pub total_seconds: Option<u32>,
    #[prost(uint32, optional, tag = "5")]
    pub seconds_left: Option<u32>,
    #[prost(bool, optional, tag = "6")]
    pub is_active: Option<bool>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ListEntitiesUpdateResponse {
    #[prost(string, optional, tag = "1")]
    pub object_id: Option<String>,
    #[prost(fixed32, optional, tag = "2")]
    pub key: Option<u32>,
    #[prost(string, optional, tag = "3")]
    pub name: Option<String>,
    #[prost(uint32, optional, tag = "7")]
    pub entity_category: Option<u32>,
    #[prost(string, optional, tag = "8")]
    pub device_class: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct UpdateStateResponse {
    #[prost(fixed32, optional, tag = "1")]
    pub key: Option<u32>,
    #[prost(bool, optional, tag = "2")]
    pub missing_state: Option<bool>,
    #[prost(bool, optional, tag = "3")]
    pub in_progress: Option<bool>,
    #[prost(bool, optional, tag = "4")]
    pub has_progress: Option<bool>,
    #[prost(float, optional, tag = "5")]
    pub progress: Option<f32>,
    #[prost(string, optional, tag = "6")]
    pub current_version: Option<String>,
    #[prost(string, optional, tag = "7")]
    pub latest_version: Option<String>,
    #[prost(string, optional, tag = "8")]
    pub title: Option<String>,
    #[prost(string, optional, tag = "10")]
    pub release_url: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct UpdateCommandRequest {
    #[prost(fixed32, optional, tag = "1")]
    pub key: Option<u32>,
    #[prost(uint32, optional, tag = "2")]
    pub command: Option<u32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct VoiceAssistantAnnounceRequest {
    #[prost(string, optional, tag = "1")]
    pub media_id: Option<String>,
    #[prost(string, optional, tag = "2")]
    pub text: Option<String>,
    #[prost(string, optional, tag = "3")]
    pub preannounce_media_id: Option<String>,
    #[prost(bool, optional, tag = "4")]
    pub start_conversation: Option<bool>,
}

#[derive(Clone, PartialEq, Message)]
pub struct VoiceAssistantAnnounceFinished {
    #[prost(bool, optional, tag = "1")]
    pub success: Option<bool>,
}

#[derive(Clone, PartialEq, Message)]
pub struct VoiceAssistantConfigurationRequest {}

#[derive(Clone, PartialEq, Message)]
pub struct VoiceAssistantWakeWord {
    #[prost(string, optional, tag = "1")]
    pub id: Option<String>,
    #[prost(string, optional, tag = "2")]
    pub wake_word: Option<String>,
    #[prost(string, repeated, tag = "3")]
    pub trained_languages: Vec<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct VoiceAssistantConfigurationResponse {
    #[prost(message, repeated, tag = "1")]
    pub available_wake_words: Vec<VoiceAssistantWakeWord>,
    #[prost(string, repeated, tag = "2")]
    pub active_wake_words: Vec<String>,
    #[prost(uint32, optional, tag = "3")]
    pub max_active_wake_words: Option<u32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct VoiceAssistantSetConfiguration {
    #[prost(string, repeated, tag = "1")]
    pub active_wake_words: Vec<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct NoiseEncryptionSetKeyRequest {
    #[prost(bytes = "vec", optional, tag = "1")]
    pub key: Option<Vec<u8>>,
}

#[derive(Clone, PartialEq, Message)]
pub struct NoiseEncryptionSetKeyResponse {
    #[prost(bool, optional, tag = "1")]
    pub success: Option<bool>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ListEntitiesServicesArgument {
    #[prost(string, optional, tag = "1")]
    pub name: Option<String>,
    #[prost(uint32, optional, tag = "2")]
    pub r#type: Option<u32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ListEntitiesServicesResponse {
    #[prost(string, optional, tag = "1")]
    pub name: Option<String>,
    #[prost(fixed32, optional, tag = "2")]
    pub key: Option<u32>,
    #[prost(message, repeated, tag = "3")]
    pub args: Vec<ListEntitiesServicesArgument>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ExecuteServiceArgument {
    #[prost(bool, optional, tag = "1")]
    pub bool_: Option<bool>,
    #[prost(int32, optional, tag = "2")]
    pub legacy_int: Option<i32>,
    #[prost(float, optional, tag = "3")]
    pub float_: Option<f32>,
    #[prost(string, optional, tag = "4")]
    pub string_: Option<String>,
    #[prost(sint32, optional, tag = "5")]
    pub int_: Option<i32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ExecuteServiceRequest {
    #[prost(fixed32, optional, tag = "1")]
    pub key: Option<u32>,
    #[prost(message, repeated, tag = "2")]
    pub args: Vec<ExecuteServiceArgument>,
}
