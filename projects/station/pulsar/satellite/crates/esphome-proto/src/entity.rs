//! Устойчивая таблица сущностей: одна [`EntityDef`] на сущность, её ключ, `object_id` и
//! метаданные.
//!
//! Home Assistant хранит реестр сущностей по fixed32-ключу, поэтому ключи, `object_id`, имена
//! и порядок ответов ListEntities — контракт провода. Какие сущности есть у модели, решают
//! [`Capabilities`]; [`listed`] — единственное место этого решения.

use prost::Message;

use crate::pb::msg;
use crate::pb::*;

/// Счётчик «Follow-up duration», с: с запасом меньше 15 с, после которых HA сам заканчивает
/// ожидание речи.
// см. docs/voice.md, «Голосовой ассистент HA (ESPHome)»
pub const FOLLOWUP_DURATION_MAX_S: u32 = 10;

/// Fixed32-ключ сущности; не меняется между перезапусками.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityKey(pub u32);

impl EntityKey {
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Ключи сущностей C++-референса и этого форка (1..=40).
pub mod key {
    use super::EntityKey;

    pub const MEDIA: EntityKey = EntityKey(1);
    pub const SW_MICROPHONES: EntityKey = EntityKey(2);
    pub const SW_CLOCK: EntityKey = EntityKey(3);
    pub const BS_HW_MIC_MUTE: EntityKey = EntityKey(4);
    pub const SENSOR_LUX: EntityKey = EntityKey(5);
    pub const NUMBER_CLOCK_BRI: EntityKey = EntityKey(6);
    pub const LIGHT_RING: EntityKey = EntityKey(7);
    pub const EVENT_BUTTON: EntityKey = EntityKey(8);
    pub const BUTTON_RESTART: EntityKey = EntityKey(9);
    pub const SENSOR_RSSI: EntityKey = EntityKey(10);
    pub const SENSOR_UPTIME: EntityKey = EntityKey(11);
    pub const TEXT_IP: EntityKey = EntityKey(12);
    pub const TEXT_VERSION: EntityKey = EntityKey(13);
    pub const TEXT_CLOCK: EntityKey = EntityKey(14);
    pub const SELECT_CLOCK_FACE: EntityKey = EntityKey(15);
    pub const NUMBER_CLOCK_SCROLL: EntityKey = EntityKey(16);
    pub const ACTION_CLOCK_TEXT: EntityKey = EntityKey(17);
    pub const ACTION_CLOCK_FACE: EntityKey = EntityKey(18);
    pub const UPDATE: EntityKey = EntityKey(19);
    pub const SW_AUTO_UPDATE: EntityKey = EntityKey(20);
    pub const SELECT_ZIGBEE_MODE: EntityKey = EntityKey(21);
    pub const BUTTON_ZIGBEE_JOIN: EntityKey = EntityKey(22);
    pub const BUTTON_ZIGBEE_LEAVE: EntityKey = EntityKey(23);
    pub const TEXT_ZIGBEE: EntityKey = EntityKey(24);
    pub const SW_NEAREST_ANSWERS: EntityKey = EntityKey(25);
    pub const SELECT_MUSIC_VIZ: EntityKey = EntityKey(26);
    pub const NUMBER_LED_SPEED: EntityKey = EntityKey(27);
    pub const SW_VOICE_INDICATION: EntityKey = EntityKey(28);
    pub const SELECT_CASE_COLOR: EntityKey = EntityKey(29);
    pub const ACTION_LED_SHOW: EntityKey = EntityKey(30);
    pub const SELECT_CLOCK_MUSIC: EntityKey = EntityKey(31);
    pub const SW_WAKE_PREROLL: EntityKey = EntityKey(32);
    pub const SW_CLOCK_AUTO_BRI: EntityKey = EntityKey(33);
    pub const BUTTON_WAKE_FALSE_ALARM: EntityKey = EntityKey(34);
    pub const BUTTON_WAKE_MISSED: EntityKey = EntityKey(35);
    pub const SW_WAKE_EVENT_RECORDING: EntityKey = EntityKey(36);
    pub const SW_WAKE_DRY_RUN: EntityKey = EntityKey(37);
    pub const SELECT_VOICE_COLOR: EntityKey = EntityKey(38);
    pub const NUMBER_FOLLOWUP_DURATION: EntityKey = EntityKey(39);
    pub const NUMBER_ANNOUNCE_VOLUME: EntityKey = EntityKey(40);
}

/// aioesphomeapi VoiceAssistantFeature: голосовой ассистент, API-аудио, таймеры, анонсы,
/// старт разговора.
pub const VOICE_FEATURES: u32 = 1 | 4 | 8 | 16 | 32;
/// HA MediaPlayerEntityFeature: пауза, громкость / mute / шаг, play media, стоп, play,
/// media announce.
pub const MEDIA_FEATURES: u32 = 1 | 4 | 8 | 512 | 1024 | 4096 | 16384 | 1_048_576;
pub const ENTITY_CATEGORY_CONFIG: u32 = 1;
pub const ENTITY_CATEGORY_DIAGNOSTIC: u32 = 2;
pub const COLOR_MODE_RGB: u32 = 35;
pub const STATE_CLASS_MEASUREMENT: u32 = 1;
pub const STATE_CLASS_TOTAL_INCREASING: u32 = 2;
pub const NUMBER_MODE_SLIDER: u32 = 2;
pub const SERVICE_ARG_INT: u32 = 1;
pub const SERVICE_ARG_STRING: u32 = 3;
/// Максимальная длина текста «Clock text» в символах UTF-8.
pub const CLOCK_TEXT_MAX: u32 = 64;
/// Границы числа «LED speed».
pub const LED_SPEED_MIN: f32 = 0.25;
pub const LED_SPEED_MAX: f32 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    None,
    Config,
    Diagnostic,
}

impl Category {
    const fn code(self) -> Option<u32> {
        match self {
            Self::None => None,
            Self::Config => Some(ENTITY_CATEGORY_CONFIG),
            Self::Diagnostic => Some(ENTITY_CATEGORY_DIAGNOSTIC),
        }
    }
}

/// Какой список опций из [`EntityOptions`] заполняет select (или эффекты света).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionSet {
    ClockFaces,
    MusicStyles,
    CaseColors,
    ClockMusic,
    VoiceColors,
    ZigbeeModes,
}

impl OptionSet {
    fn get<'a>(self, opts: &EntityOptions<'a>) -> &'a [String] {
        match self {
            Self::ClockFaces => opts.clock_faces,
            Self::MusicStyles => opts.music_styles,
            Self::CaseColors => opts.case_colors,
            Self::ClockMusic => opts.clock_music_styles,
            Self::VoiceColors => opts.voice_colors,
            Self::ZigbeeModes => opts.zigbee_modes,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EntityKind {
    MediaPlayer,
    Switch {
        category: Category,
    },
    BinarySensor {
        device_class: &'static str,
        category: Category,
    },
    Sensor {
        unit: &'static str,
        device_class: &'static str,
        state_class: u32,
        category: Category,
    },
    Number {
        min: f32,
        max: f32,
        step: f32,
        unit: &'static str,
    },
    Light,
    Event {
        event_types: &'static [&'static str],
    },
    Button {
        category: Category,
        device_class: Option<&'static str>,
    },
    TextSensor {
        category: Category,
    },
    Text {
        max_length: u32,
    },
    Select {
        options: OptionSet,
        category: Category,
    },
    Update,
    Action {
        args: &'static [(&'static str, u32)],
    },
}

/// Тип сущности без данных: для проверок «id → правильная запись таблицы» и диспетчеризации
/// состояний.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityKindTag {
    MediaPlayer,
    Switch,
    BinarySensor,
    Sensor,
    Number,
    Light,
    Event,
    Button,
    TextSensor,
    Text,
    Select,
    Update,
    Action,
}

impl EntityKind {
    pub fn tag(self) -> EntityKindTag {
        match self {
            Self::MediaPlayer => EntityKindTag::MediaPlayer,
            Self::Switch { .. } => EntityKindTag::Switch,
            Self::BinarySensor { .. } => EntityKindTag::BinarySensor,
            Self::Sensor { .. } => EntityKindTag::Sensor,
            Self::Number { .. } => EntityKindTag::Number,
            Self::Light => EntityKindTag::Light,
            Self::Event { .. } => EntityKindTag::Event,
            Self::Button { .. } => EntityKindTag::Button,
            Self::TextSensor { .. } => EntityKindTag::TextSensor,
            Self::Text { .. } => EntityKindTag::Text,
            Self::Select { .. } => EntityKindTag::Select,
            Self::Update => EntityKindTag::Update,
            Self::Action { .. } => EntityKindTag::Action,
        }
    }
}

/// Что должно быть у модели, чтобы сущность существовала.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capability {
    Always,
    Clock,
    ClockAutoBrightness,
    Lux,
    WakePreroll,
    HwMicMute,
    Buttons,
    CaseColors,
    VoiceColors,
    Zigbee,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityDef {
    pub key: EntityKey,
    pub kind: EntityKind,
    pub object_id: &'static str,
    pub name: &'static str,
    pub icon: Option<&'static str>,
    pub requires: Capability,
}

/// Какие сущности есть у модели. Заполняется контроллером один раз при сборке сервисов.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub clock: bool,
    pub lux: bool,
    pub auto_brightness: bool,
    pub wake_preroll: bool,
    pub hw_mic_mute: bool,
    pub buttons: bool,
    /// Больше одной опции: select с единственной опцией не публикуется.
    pub case_colors: bool,
    pub voice_colors: bool,
    pub zigbee: bool,
}

impl Capabilities {
    fn has(self, c: Capability) -> bool {
        match c {
            Capability::Always => true,
            Capability::Clock => self.clock,
            Capability::ClockAutoBrightness => self.clock && self.auto_brightness,
            Capability::Lux => self.lux,
            Capability::WakePreroll => self.wake_preroll,
            Capability::HwMicMute => self.hw_mic_mute,
            Capability::Buttons => self.buttons,
            Capability::CaseColors => self.case_colors,
            Capability::VoiceColors => self.voice_colors,
            Capability::Zigbee => self.zigbee,
        }
    }
}

/// Единственный гейт: перечисляет ли модель эту сущность.
pub fn listed(def: &EntityDef, caps: &Capabilities) -> bool {
    caps.has(def.requires)
}

/// Списки опций, которые нужны ListEntities. Только они, без остального конфига сервера.
#[derive(Clone, Copy, Debug, Default)]
pub struct EntityOptions<'a> {
    pub light_effects: &'a [String],
    pub clock_faces: &'a [String],
    pub music_styles: &'a [String],
    pub case_colors: &'a [String],
    pub clock_music_styles: &'a [String],
    pub voice_colors: &'a [String],
    pub zigbee_modes: &'a [String],
}

// Типизированные id сущностей: единственное отображение на ключ таблицы.

macro_rules! id_enum {
    ($name:ident { $($variant:ident = $value:literal => $key:ident),* $(,)? }) => {
        #[repr(u32)]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum $name {
            $($variant = $value),*
        }

        impl $name {
            pub const fn key(self) -> EntityKey {
                match self {
                    $(Self::$variant => key::$key),*
                }
            }
        }
    };
}

id_enum!(SwitchId {
    Microphones = 1 => SW_MICROPHONES,
    Clock = 2 => SW_CLOCK,
    AutoUpdate = 3 => SW_AUTO_UPDATE,
    NearestAnswers = 4 => SW_NEAREST_ANSWERS,
    VoiceIndication = 5 => SW_VOICE_INDICATION,
    WakePreroll = 6 => SW_WAKE_PREROLL,
    ClockAutoBrightness = 7 => SW_CLOCK_AUTO_BRI,
    WakeEventRecording = 8 => SW_WAKE_EVENT_RECORDING,
    WakeDryRun = 9 => SW_WAKE_DRY_RUN,
});

id_enum!(SensorId {
    Illuminance = 1 => SENSOR_LUX,
});

id_enum!(BinarySensorId {
    HwMicMute = 1 => BS_HW_MIC_MUTE,
});

id_enum!(NumberId {
    ClockBrightness = 1 => NUMBER_CLOCK_BRI,
    ClockScroll = 2 => NUMBER_CLOCK_SCROLL,
    LedSpeed = 3 => NUMBER_LED_SPEED,
    FollowupDuration = 4 => NUMBER_FOLLOWUP_DURATION,
    AnnounceVolume = 5 => NUMBER_ANNOUNCE_VOLUME,
});

id_enum!(TextId {
    ClockText = 1 => TEXT_CLOCK,
});

id_enum!(SelectId {
    ClockFace = 1 => SELECT_CLOCK_FACE,
    ZigbeeMode = 2 => SELECT_ZIGBEE_MODE,
    MusicViz = 3 => SELECT_MUSIC_VIZ,
    CaseColor = 4 => SELECT_CASE_COLOR,
    ClockMusic = 5 => SELECT_CLOCK_MUSIC,
    VoiceColor = 6 => SELECT_VOICE_COLOR,
});

id_enum!(ButtonId {
    ZigbeeJoin = 1 => BUTTON_ZIGBEE_JOIN,
    ZigbeeLeave = 2 => BUTTON_ZIGBEE_LEAVE,
    WakeFalseAlarm = 3 => BUTTON_WAKE_FALSE_ALARM,
    WakeMissed = 4 => BUTTON_WAKE_MISSED,
});

id_enum!(TextSensorId {
    Zigbee = 1 => TEXT_ZIGBEE,
});

id_enum!(EventId {
    Button = 1 => EVENT_BUTTON,
});

id_enum!(ActionId {
    ClockShowText = 1 => ACTION_CLOCK_TEXT,
    ClockShowFace = 2 => ACTION_CLOCK_FACE,
    LedShow = 3 => ACTION_LED_SHOW,
});

const EVENT_TYPES: &[&str] = &["action", "action_long", "volume_up", "volume_down"];
const ACTION_CLOCK_TEXT_ARGS: &[(&str, u32)] =
    &[("text", SERVICE_ARG_STRING), ("seconds", SERVICE_ARG_INT)];
const ACTION_CLOCK_FACE_ARGS: &[(&str, u32)] =
    &[("face", SERVICE_ARG_STRING), ("seconds", SERVICE_ARG_INT)];
const ACTION_LED_SHOW_ARGS: &[(&str, u32)] = &[
    ("effect", SERVICE_ARG_STRING),
    ("color", SERVICE_ARG_STRING),
    ("seconds", SERVICE_ARG_INT),
];

const fn ent(
    key: EntityKey,
    kind: EntityKind,
    object_id: &'static str,
    name: &'static str,
    icon: Option<&'static str>,
    requires: Capability,
) -> EntityDef {
    EntityDef {
        key,
        kind,
        object_id,
        name,
        icon,
        requires,
    }
}

/// Список сущностей в порядке C++ (HA хранит его по устройству; ключи и `object_id` не должны
/// двигаться). Добавленные форком сущности отмечены в golden-захвате.
pub const ENTITIES: &[EntityDef] = &[
    ent(
        key::MEDIA,
        EntityKind::MediaPlayer,
        "media_player",
        "Media Player",
        None,
        Capability::Always,
    ),
    ent(
        key::SW_MICROPHONES,
        EntityKind::Switch {
            category: Category::None,
        },
        "microphones",
        "Микрофоны",
        Some("mdi:microphone"),
        Capability::Always,
    ),
    ent(
        key::SW_CLOCK,
        EntityKind::Switch {
            category: Category::None,
        },
        "clock_display",
        "Часы на дисплее",
        Some("mdi:clock-digital"),
        Capability::Clock,
    ),
    ent(
        key::SW_CLOCK_AUTO_BRI,
        EntityKind::Switch {
            category: Category::Config,
        },
        "clock_auto_brightness",
        "Clock auto brightness",
        Some("mdi:brightness-auto"),
        Capability::ClockAutoBrightness,
    ),
    ent(
        key::SW_AUTO_UPDATE,
        EntityKind::Switch {
            category: Category::Config,
        },
        "auto_update",
        "Auto update",
        Some("mdi:update"),
        Capability::Always,
    ),
    ent(
        key::SW_NEAREST_ANSWERS,
        EntityKind::Switch {
            category: Category::Config,
        },
        "nearest_station_answers",
        "Nearest station answers",
        Some("mdi:speaker-multiple"),
        Capability::Always,
    ),
    ent(
        key::SW_VOICE_INDICATION,
        EntityKind::Switch {
            category: Category::Config,
        },
        "voice_indication",
        "Voice indication",
        Some("mdi:led-on"),
        Capability::Always,
    ),
    ent(
        key::SW_WAKE_PREROLL,
        EntityKind::Switch {
            category: Category::Config,
        },
        "command_right_after_wake_word",
        "Command right after wake word",
        Some("mdi:skip-backward"),
        Capability::WakePreroll,
    ),
    ent(
        key::SW_WAKE_EVENT_RECORDING,
        EntityKind::Switch {
            category: Category::Config,
        },
        "wake_event_recording",
        "Wake: запись событий",
        Some("mdi:record-circle-outline"),
        Capability::Always,
    ),
    ent(
        key::SW_WAKE_DRY_RUN,
        EntityKind::Switch {
            category: Category::Config,
        },
        "wake_dry_run",
        "Wake: тестовый режим (без ответа)",
        Some("mdi:robot-off-outline"),
        Capability::Always,
    ),
    ent(
        key::NUMBER_LED_SPEED,
        EntityKind::Number {
            min: LED_SPEED_MIN,
            max: LED_SPEED_MAX,
            step: 0.05,
            unit: "×",
        },
        "led_speed",
        "LED speed",
        Some("mdi:speedometer"),
        Capability::Always,
    ),
    ent(
        key::NUMBER_FOLLOWUP_DURATION,
        EntityKind::Number {
            min: 0.0,
            max: FOLLOWUP_DURATION_MAX_S as f32,
            step: 1.0,
            unit: "s",
        },
        "followup_duration",
        "Follow-up duration",
        Some("mdi:microphone-message"),
        Capability::Always,
    ),
    ent(
        key::NUMBER_ANNOUNCE_VOLUME,
        EntityKind::Number {
            min: 0.0,
            max: 100.0,
            step: 5.0,
            unit: "%",
        },
        "announce_volume",
        "Громкость объявлений",
        Some("mdi:bullhorn"),
        Capability::Always,
    ),
    ent(
        key::SELECT_MUSIC_VIZ,
        EntityKind::Select {
            options: OptionSet::MusicStyles,
            category: Category::Config,
        },
        "music_visualization",
        "Music visualization",
        Some("mdi:equalizer"),
        Capability::Always,
    ),
    ent(
        key::SELECT_CASE_COLOR,
        EntityKind::Select {
            options: OptionSet::CaseColors,
            category: Category::Config,
        },
        "case_color",
        "Case color",
        Some("mdi:palette"),
        Capability::CaseColors,
    ),
    ent(
        key::SELECT_VOICE_COLOR,
        EntityKind::Select {
            options: OptionSet::VoiceColors,
            category: Category::Config,
        },
        "voice_color",
        "Voice color",
        Some("mdi:circle"),
        Capability::VoiceColors,
    ),
    ent(
        key::SELECT_CLOCK_MUSIC,
        EntityKind::Select {
            options: OptionSet::ClockMusic,
            category: Category::Config,
        },
        "clock_music",
        "Clock music",
        Some("mdi:equalizer-outline"),
        Capability::Clock,
    ),
    ent(
        key::UPDATE,
        EntityKind::Update,
        "firmware",
        "Firmware",
        None,
        Capability::Always,
    ),
    ent(
        key::BS_HW_MIC_MUTE,
        EntityKind::BinarySensor {
            device_class: "",
            category: Category::Diagnostic,
        },
        "hardware_mic_mute",
        "Hardware mic mute",
        Some("mdi:microphone-off"),
        Capability::HwMicMute,
    ),
    ent(
        key::SENSOR_LUX,
        EntityKind::Sensor {
            unit: "lx",
            device_class: "illuminance",
            state_class: STATE_CLASS_MEASUREMENT,
            category: Category::None,
        },
        "illuminance",
        "Illuminance",
        None,
        Capability::Lux,
    ),
    ent(
        key::NUMBER_CLOCK_BRI,
        EntityKind::Number {
            min: 0.0,
            max: 100.0,
            step: 1.0,
            unit: "%",
        },
        "clock_brightness",
        "Clock brightness",
        Some("mdi:brightness-6"),
        Capability::Clock,
    ),
    ent(
        key::LIGHT_RING,
        EntityKind::Light,
        "ring",
        "Подсветка",
        Some("mdi:led-strip-variant"),
        Capability::Always,
    ),
    ent(
        key::EVENT_BUTTON,
        EntityKind::Event {
            event_types: EVENT_TYPES,
        },
        "button",
        "Button",
        Some("mdi:gesture-tap-button"),
        Capability::Buttons,
    ),
    ent(
        key::BUTTON_RESTART,
        EntityKind::Button {
            category: Category::Config,
            device_class: Some("restart"),
        },
        "restart",
        "Restart",
        None,
        Capability::Always,
    ),
    ent(
        key::BUTTON_WAKE_FALSE_ALARM,
        EntityKind::Button {
            category: Category::Config,
            device_class: None,
        },
        "wake_false_alarm",
        "Wake: ложное срабатывание",
        Some("mdi:alert-circle-outline"),
        Capability::Always,
    ),
    ent(
        key::BUTTON_WAKE_MISSED,
        EntityKind::Button {
            category: Category::Config,
            device_class: None,
        },
        "wake_missed",
        "Wake: не услышала",
        Some("mdi:ear-hearing-off"),
        Capability::Always,
    ),
    ent(
        key::SENSOR_RSSI,
        EntityKind::Sensor {
            unit: "dBm",
            device_class: "signal_strength",
            state_class: STATE_CLASS_MEASUREMENT,
            category: Category::Diagnostic,
        },
        "wifi_signal",
        "Wi-Fi signal",
        None,
        Capability::Always,
    ),
    ent(
        key::SENSOR_UPTIME,
        EntityKind::Sensor {
            unit: "s",
            device_class: "duration",
            state_class: STATE_CLASS_TOTAL_INCREASING,
            category: Category::Diagnostic,
        },
        "uptime",
        "Uptime",
        None,
        Capability::Always,
    ),
    ent(
        key::TEXT_IP,
        EntityKind::TextSensor {
            category: Category::Diagnostic,
        },
        "ip_address",
        "IP address",
        Some("mdi:ip-network"),
        Capability::Always,
    ),
    ent(
        key::TEXT_VERSION,
        EntityKind::TextSensor {
            category: Category::Diagnostic,
        },
        "version",
        "Version",
        Some("mdi:new-box"),
        Capability::Always,
    ),
    ent(
        key::NUMBER_CLOCK_SCROLL,
        EntityKind::Number {
            min: 100.0,
            max: 2000.0,
            step: 50.0,
            unit: "ms",
        },
        "clock_text_speed",
        "Clock text speed",
        Some("mdi:speedometer"),
        Capability::Clock,
    ),
    ent(
        key::TEXT_CLOCK,
        EntityKind::Text {
            max_length: CLOCK_TEXT_MAX,
        },
        "clock_text",
        "Clock text",
        Some("mdi:form-textbox"),
        Capability::Clock,
    ),
    ent(
        key::SELECT_CLOCK_FACE,
        EntityKind::Select {
            options: OptionSet::ClockFaces,
            category: Category::None,
        },
        "clock_face",
        "Clock face",
        Some("mdi:emoticon-outline"),
        Capability::Clock,
    ),
    ent(
        key::SELECT_ZIGBEE_MODE,
        EntityKind::Select {
            options: OptionSet::ZigbeeModes,
            category: Category::Config,
        },
        "zigbee_role",
        "Zigbee role",
        Some("mdi:zigbee"),
        Capability::Zigbee,
    ),
    ent(
        key::BUTTON_ZIGBEE_JOIN,
        EntityKind::Button {
            category: Category::Config,
            device_class: None,
        },
        "zigbee_join",
        "Zigbee join network",
        Some("mdi:link-variant"),
        Capability::Zigbee,
    ),
    ent(
        key::BUTTON_ZIGBEE_LEAVE,
        EntityKind::Button {
            category: Category::Config,
            device_class: None,
        },
        "zigbee_leave",
        "Zigbee leave network",
        Some("mdi:link-variant-off"),
        Capability::Zigbee,
    ),
    ent(
        key::TEXT_ZIGBEE,
        EntityKind::TextSensor {
            category: Category::Diagnostic,
        },
        "zigbee",
        "Zigbee",
        Some("mdi:zigbee"),
        Capability::Zigbee,
    ),
    ent(
        key::ACTION_CLOCK_TEXT,
        EntityKind::Action {
            args: ACTION_CLOCK_TEXT_ARGS,
        },
        "clock_show_text",
        "clock_show_text",
        None,
        Capability::Clock,
    ),
    ent(
        key::ACTION_CLOCK_FACE,
        EntityKind::Action {
            args: ACTION_CLOCK_FACE_ARGS,
        },
        "clock_show_face",
        "clock_show_face",
        None,
        Capability::Clock,
    ),
    ent(
        key::ACTION_LED_SHOW,
        EntityKind::Action {
            args: ACTION_LED_SHOW_ARGS,
        },
        "led_show",
        "led_show",
        None,
        Capability::Always,
    ),
];

/// Порядок replay состояний (исторический порядок C++ `send_all_states`, не порядок списка).
/// Golden-захват сравнивает состояния по порядку, поэтому таблица списка здесь не подходит.
pub const REPLAY_ORDER: &[EntityKey] = &[
    key::MEDIA,
    key::SW_MICROPHONES,
    key::SW_CLOCK,
    key::SW_CLOCK_AUTO_BRI,
    key::SW_AUTO_UPDATE,
    key::SW_NEAREST_ANSWERS,
    key::SW_VOICE_INDICATION,
    key::SW_WAKE_PREROLL,
    key::SW_WAKE_EVENT_RECORDING,
    key::SW_WAKE_DRY_RUN,
    key::UPDATE,
    key::BS_HW_MIC_MUTE,
    key::SENSOR_LUX,
    key::NUMBER_CLOCK_BRI,
    key::LIGHT_RING,
    key::SENSOR_RSSI,
    key::SENSOR_UPTIME,
    key::TEXT_IP,
    key::TEXT_VERSION,
    key::NUMBER_CLOCK_SCROLL,
    key::TEXT_CLOCK,
    key::SELECT_CLOCK_FACE,
    key::NUMBER_LED_SPEED,
    key::NUMBER_FOLLOWUP_DURATION,
    key::SELECT_MUSIC_VIZ,
    key::SELECT_CASE_COLOR,
    key::SELECT_VOICE_COLOR,
    key::SELECT_CLOCK_MUSIC,
    key::SELECT_ZIGBEE_MODE,
    key::TEXT_ZIGBEE,
    key::NUMBER_ANNOUNCE_VOLUME,
];

/// Запись таблицы по ключу.
pub fn find(key: EntityKey) -> Option<&'static EntityDef> {
    ENTITIES.iter().find(|def| def.key == key)
}

/// Одно сообщение ListEntities, готовое для фрейминга.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntityMessage {
    pub msg_type: u32,
    pub payload: Vec<u8>,
}

trait Head {
    fn head(def: &EntityDef) -> Self;
}

macro_rules! head_impl {
    ($($ty:ty),* $(,)?) => {
        $(impl Head for $ty {
            fn head(def: &EntityDef) -> Self {
                Self {
                    object_id: Some(def.object_id.to_owned()),
                    key: Some(def.key.get()),
                    name: Some(def.name.to_owned()),
                    ..Self::default()
                }
            }
        })*
    };
}

head_impl!(
    ListEntitiesBinarySensorResponse,
    ListEntitiesLightResponse,
    ListEntitiesSensorResponse,
    ListEntitiesSwitchResponse,
    ListEntitiesTextSensorResponse,
    ListEntitiesNumberResponse,
    ListEntitiesSelectResponse,
    ListEntitiesButtonResponse,
    ListEntitiesMediaPlayerResponse,
    ListEntitiesTextResponse,
    ListEntitiesEventResponse,
    ListEntitiesUpdateResponse,
);

fn icon(def: &EntityDef) -> Option<String> {
    def.icon.map(str::to_owned)
}

fn switch_message(def: &EntityDef, category: Category) -> EntityMessage {
    let mut m: ListEntitiesSwitchResponse = Head::head(def);
    m.icon = icon(def);
    m.entity_category = category.code();
    EntityMessage {
        msg_type: msg::LIST_SWITCH,
        payload: m.encode_to_vec(),
    }
}

fn button_message(
    def: &EntityDef,
    category: Category,
    device_class: Option<&str>,
) -> EntityMessage {
    let mut m: ListEntitiesButtonResponse = Head::head(def);
    m.icon = icon(def);
    m.entity_category = category.code();
    m.device_class = device_class.map(str::to_owned);
    EntityMessage {
        msg_type: msg::LIST_BUTTON,
        payload: m.encode_to_vec(),
    }
}

fn number_message(def: &EntityDef, min: f32, max: f32, step: f32, unit: &str) -> EntityMessage {
    let mut m: ListEntitiesNumberResponse = Head::head(def);
    m.icon = icon(def);
    m.min_value = min;
    m.max_value = max;
    m.step = step;
    m.entity_category = Some(ENTITY_CATEGORY_CONFIG);
    m.unit_of_measurement = Some(unit.to_owned());
    m.mode = Some(NUMBER_MODE_SLIDER);
    EntityMessage {
        msg_type: msg::LIST_NUMBER,
        payload: m.encode_to_vec(),
    }
}

fn select_message(
    def: &EntityDef,
    opts: &EntityOptions<'_>,
    options: OptionSet,
    category: Category,
) -> EntityMessage {
    let mut m: ListEntitiesSelectResponse = Head::head(def);
    m.icon = icon(def);
    m.options = options.get(opts).to_vec();
    m.entity_category = category.code();
    EntityMessage {
        msg_type: msg::LIST_SELECT,
        payload: m.encode_to_vec(),
    }
}

fn sensor_message(
    def: &EntityDef,
    unit: &str,
    device_class: &str,
    state_class: u32,
    category: Category,
) -> EntityMessage {
    let mut m: ListEntitiesSensorResponse = Head::head(def);
    m.unit_of_measurement = Some(unit.to_owned());
    m.device_class = Some(device_class.to_owned());
    m.state_class = Some(state_class);
    m.entity_category = category.code();
    EntityMessage {
        msg_type: msg::LIST_SENSOR,
        payload: m.encode_to_vec(),
    }
}

fn text_sensor_message(def: &EntityDef, category: Category) -> EntityMessage {
    let mut m: ListEntitiesTextSensorResponse = Head::head(def);
    m.icon = icon(def);
    m.entity_category = category.code();
    EntityMessage {
        msg_type: msg::LIST_TEXT_SENSOR,
        payload: m.encode_to_vec(),
    }
}

fn action_message(def: &EntityDef, args: &[(&str, u32)]) -> EntityMessage {
    let m = ListEntitiesServicesResponse {
        name: Some(def.name.to_owned()),
        key: Some(def.key.get()),
        args: args
            .iter()
            .map(|&(name, ty)| ListEntitiesServicesArgument {
                name: Some(name.to_owned()),
                r#type: Some(ty),
            })
            .collect(),
    };
    EntityMessage {
        msg_type: msg::LIST_SERVICES,
        payload: m.encode_to_vec(),
    }
}

fn media_player_message(def: &EntityDef) -> EntityMessage {
    let format = |channels, purpose| MediaPlayerSupportedFormat {
        format: Some("flac".to_owned()),
        sample_rate: Some(48_000),
        num_channels: Some(channels),
        purpose,
        sample_bytes: Some(2),
    };
    let m = ListEntitiesMediaPlayerResponse {
        object_id: Some(def.object_id.to_owned()),
        key: Some(def.key.get()),
        name: Some(def.name.to_owned()),
        supports_pause: Some(true),
        supported_formats: vec![format(2, 0), format(1, 1)],
        feature_flags: Some(MEDIA_FEATURES),
    };
    EntityMessage {
        msg_type: msg::LIST_MEDIA_PLAYER,
        payload: m.encode_to_vec(),
    }
}

fn light_message(def: &EntityDef, opts: &EntityOptions<'_>) -> EntityMessage {
    let mut effects = Vec::with_capacity(opts.light_effects.len() + 1);
    effects.push("None".to_owned());
    effects.extend(opts.light_effects.iter().cloned());
    let m = ListEntitiesLightResponse {
        object_id: Some(def.object_id.to_owned()),
        key: Some(def.key.get()),
        name: Some(def.name.to_owned()),
        effects,
        supported_color_modes: vec![COLOR_MODE_RGB],
        icon: icon(def),
    };
    EntityMessage {
        msg_type: msg::LIST_LIGHT,
        payload: m.encode_to_vec(),
    }
}

/// Все ответы ListEntities этой конфигурации по порядку, затем ListEntitiesDone.
pub fn list_entities(caps: &Capabilities, opts: &EntityOptions<'_>) -> Vec<EntityMessage> {
    let mut out = Vec::with_capacity(ENTITIES.len() + 1);
    for def in ENTITIES.iter().filter(|d| listed(d, caps)) {
        out.push(match def.kind {
            EntityKind::MediaPlayer => media_player_message(def),
            EntityKind::Switch { category } => switch_message(def, category),
            EntityKind::BinarySensor {
                category,
                device_class,
            } => {
                let mut m: ListEntitiesBinarySensorResponse = Head::head(def);
                m.device_class = (!device_class.is_empty()).then(|| device_class.to_owned());
                m.icon = icon(def);
                m.entity_category = category.code();
                EntityMessage {
                    msg_type: msg::LIST_BINARY_SENSOR,
                    payload: m.encode_to_vec(),
                }
            }
            EntityKind::Sensor {
                unit,
                device_class,
                state_class,
                category,
            } => sensor_message(def, unit, device_class, state_class, category),
            EntityKind::Number {
                min,
                max,
                step,
                unit,
            } => number_message(def, min, max, step, unit),
            EntityKind::Light => light_message(def, opts),
            EntityKind::Event { event_types } => {
                let mut m: ListEntitiesEventResponse = Head::head(def);
                m.icon = icon(def);
                m.device_class = Some("button".to_owned());
                m.event_types = event_types.iter().map(|s| (*s).to_owned()).collect();
                EntityMessage {
                    msg_type: msg::LIST_EVENT,
                    payload: m.encode_to_vec(),
                }
            }
            EntityKind::Button {
                category,
                device_class,
            } => button_message(def, category, device_class),
            EntityKind::TextSensor { category } => text_sensor_message(def, category),
            EntityKind::Text { max_length } => {
                let mut m: ListEntitiesTextResponse = Head::head(def);
                m.icon = icon(def);
                m.max_length = Some(max_length);
                EntityMessage {
                    msg_type: msg::LIST_TEXT,
                    payload: m.encode_to_vec(),
                }
            }
            EntityKind::Select { options, category } => {
                select_message(def, opts, options, category)
            }
            EntityKind::Update => {
                let mut m: ListEntitiesUpdateResponse = Head::head(def);
                m.entity_category = Some(ENTITY_CATEGORY_CONFIG);
                m.device_class = Some("firmware".to_owned());
                EntityMessage {
                    msg_type: msg::LIST_UPDATE,
                    payload: m.encode_to_vec(),
                }
            }
            EntityKind::Action { args } => action_message(def, args),
        });
    }
    out.push(EntityMessage {
        msg_type: msg::LIST_DONE,
        payload: Vec::new(),
    });
    out
}
