//! Доменные типы сервера: устройство, конфигурация, события для контроллера и значения
//! сущностей. Числовые значения — протокольные (api.proto / aioesphomeapi), как в C++
//! `api_server.h`.

use std::collections::HashMap;
use std::path::PathBuf;

use esphome_proto::entity::{Capabilities, EntityOptions};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeviceIdentity {
    /// Похоже на hostname, например "yandex-midi-nq3g" (не длиннее 31 символа).
    pub name: String,
    /// Например, "Яндекс Станция Миди".
    pub friendly_name: String,
    /// wlan0, "AA:BB:CC:DD:EE:FF".
    pub mac: String,
    /// "Yandex Station Midi".
    pub model: String,
    /// Codename из devicetree, например "cucumber".
    pub board: String,
    /// "Yandex".
    pub manufacturer: String,
    pub project_version: String,
    /// DeviceInfo compilation_time в формате C++ `__DATE__ ", " __TIME__`.
    pub compilation_time: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WakeWordInfo {
    /// Например, "alisa".
    pub id: String,
    /// Например, "Алиса".
    pub phrase: String,
    /// Например, ["ru"].
    pub languages: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiConfig {
    /// 6053.
    pub port: u16,
    /// Какие сущности есть у модели. Заполняется контроллером один раз.
    pub capabilities: Capabilities,
    /// Доступные на устройстве wake word.
    pub wake_words: Vec<WakeWordInfo>,
    /// Активные id на старте.
    pub active_wake_words: Vec<String>,
    /// Эффекты кольца, кроме "None".
    pub light_effects: Vec<String>,
    /// Опции select «Clock face», "None" первой.
    pub clock_faces: Vec<String>,
    /// Опции select «Music visualization».
    pub music_styles: Vec<String>,
    /// Опции select «Case color», "None" первой.
    pub case_colors: Vec<String>,
    /// Опции select «Clock music».
    pub clock_music_styles: Vec<String>,
    /// Опции select «Voice color» (Violet / Blue); пусто — сущности нет.
    pub voice_colors: Vec<String>,
    /// Опции select «Zigbee role»; пусто — Zigbee-сущностей нет (нет радио).
    pub zigbee_modes: Vec<String>,
    /// Файл ключа шифрования Noise (base64 из 32 байт, как ESPHome `api: encryption: key`).
    /// Нет файла или он пуст — plaintext (и провижининг нулевым ключом). `None` — шифрование
    /// не поддерживается.
    pub key_path: Option<PathBuf>,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            port: 6053,
            capabilities: Capabilities::default(),
            wake_words: Vec::new(),
            active_wake_words: Vec::new(),
            light_effects: Vec::new(),
            clock_faces: Vec::new(),
            music_styles: Vec::new(),
            case_colors: Vec::new(),
            clock_music_styles: Vec::new(),
            voice_colors: Vec::new(),
            zigbee_modes: Vec::new(),
            key_path: None,
        }
    }
}

impl ApiConfig {
    /// Только списки опций, которые нужны для ListEntities.
    pub fn entity_options(&self) -> EntityOptions<'_> {
        EntityOptions {
            light_effects: &self.light_effects,
            clock_faces: &self.clock_faces,
            music_styles: &self.music_styles,
            case_colors: &self.case_colors,
            clock_music_styles: &self.clock_music_styles,
            voice_colors: &self.voice_colors,
            zigbee_modes: &self.zigbee_modes,
        }
    }
}

/// aioesphomeapi VoiceAssistantEventType; `code()` — значение на проводе.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VaEvent {
    Error,
    RunStart,
    RunEnd,
    SttStart,
    SttEnd,
    IntentStart,
    IntentEnd,
    TtsStart,
    TtsEnd,
    WakeWordStart,
    WakeWordEnd,
    SttVadStart,
    SttVadEnd,
    TtsStreamStart,
    TtsStreamEnd,
    IntentProgress,
    /// Тип новее этой сборки. Всё равно событие пайплайна: C++-контроллер перезапускал
    /// watchdog LISTENING / PROCESSING на любом событии.
    Unknown(u32),
}

impl VaEvent {
    const CODES: [(u32, Self); 16] = [
        (0, Self::Error),
        (1, Self::RunStart),
        (2, Self::RunEnd),
        (3, Self::SttStart),
        (4, Self::SttEnd),
        (5, Self::IntentStart),
        (6, Self::IntentEnd),
        (7, Self::TtsStart),
        (8, Self::TtsEnd),
        (9, Self::WakeWordStart),
        (10, Self::WakeWordEnd),
        (11, Self::SttVadStart),
        (12, Self::SttVadEnd),
        (98, Self::TtsStreamStart),
        (99, Self::TtsStreamEnd),
        (100, Self::IntentProgress),
    ];

    pub fn from_code(code: u32) -> Self {
        Self::CODES
            .iter()
            .find(|(c, _)| *c == code)
            .map_or(Self::Unknown(code), |&(_, e)| e)
    }

    pub fn code(self) -> u32 {
        match self {
            Self::Unknown(c) => c,
            e => Self::CODES
                .iter()
                .find(|(_, x)| *x == e)
                .map_or(u32::MAX, |&(c, _)| c),
        }
    }
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TimerEvent {
    #[default]
    Started = 0,
    Updated = 1,
    Cancelled = 2,
    Finished = 3,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TimerInfo {
    pub event: TimerEvent,
    pub id: String,
    pub name: String,
    pub total_seconds: u32,
    pub seconds_left: u32,
    pub is_active: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AnnounceRequest {
    pub media_id: String,
    pub text: String,
    pub preannounce_media_id: String,
    pub start_conversation: bool,
}

/// api.proto MediaPlayerState.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MediaState {
    None = 0,
    Idle = 1,
    Playing = 2,
    Paused = 3,
    Announcing = 4,
    Off = 5,
    On = 6,
}

impl MediaState {
    /// The value sent in `MediaPlayerStateResponse`. Home Assistant 2026.10's ESPHome media
    /// player knows only NONE/IDLE/PLAYING/PAUSED/OFF/ON: an unknown code raises
    /// `KeyError: MediaPlayerState.ANNOUNCING` in its state subscription and the update is lost,
    /// so an announcement goes on the wire as PLAYING. The internal state, ducking and the
    /// announcement callbacks are unchanged.
    pub fn wire(self) -> u32 {
        match self {
            Self::Announcing => Self::Playing as u32,
            other => other as u32,
        }
    }
}

/// api.proto MediaPlayerCommand.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MediaCommand {
    Play = 0,
    Pause = 1,
    Stop = 2,
    Mute = 3,
    Unmute = 4,
    Toggle = 5,
    VolumeUp = 6,
    VolumeDown = 7,
    Enqueue = 8,
    RepeatOne = 9,
    RepeatOff = 10,
    ClearPlaylist = 11,
    TurnOn = 12,
    TurnOff = 13,
}

/// MediaPlayerCommandRequest: каждое поле только если HA его задала.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MediaCommandRequest {
    pub command: Option<MediaCommand>,
    pub volume: Option<f32>,
    pub media_url: Option<String>,
    pub announcement: Option<bool>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LightState {
    pub on: bool,
    /// 0..1.
    pub brightness: f32,
    /// 0..1 каждое.
    pub r: f32,
    pub g: f32,
    pub b: f32,
    /// "" или "None" — без эффекта.
    pub effect: String,
}

impl Default for LightState {
    fn default() -> Self {
        Self {
            on: false,
            brightness: 1.0,
            r: 1.0,
            g: 1.0,
            b: 1.0,
            effect: String::new(),
        }
    }
}

/// "Firmware" update entity command (api.proto UpdateCommand).
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UpdateCommand {
    None = 0,
    Install = 1,
    Check = 2,
}

/// UpdateStateResponse сущности "Firmware".
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UpdateState {
    pub in_progress: bool,
    pub has_progress: bool,
    /// 0..100.
    pub progress: f32,
    pub current_version: String,
    /// Равна `current_version`, когда новее ничего нет (или ещё не проверяли).
    pub latest_version: String,
    pub title: String,
    pub release_url: String,
}

/// Верхняя граница «Follow-up duration», с: с запасом меньше 15 с, после которых HA сам
/// заканчивает ожидание речи.
// см. docs/voice.md, «Голосовой ассистент HA (ESPHome)»
pub const FOLLOWUP_DURATION_MAX_S: u32 = esphome_proto::entity::FOLLOWUP_DURATION_MAX_S;

/// «Follow-up duration» из команды HA или `state.conf` — целые секунды 0..=MAX; `None` для NaN и
/// бесконечности (значение не меняется).
pub fn followup_duration_s(v: f32) -> Option<u32> {
    v.is_finite()
        .then(|| v.round().clamp(0.0, FOLLOWUP_DURATION_MAX_S as f32) as u32)
}

/// Что Home Assistant (или другой клиент API) попросила или сообщила; C++ `ApiHooks`.
#[derive(Clone, Debug, PartialEq)]
pub enum ApiEvent {
    /// Клиент завершил hello (событие на каждого клиента).
    ClientConnected {
        client_info: String,
    },
    /// Последний клиент после hello отключился.
    ClientDisconnected,
    /// Владелец голосового пайплайна появился / исчез. Владелец — самый свежий клиент,
    /// всё ещё подписанный на голосового ассистента; при передаче владения между клиентами
    /// false предшествует true.
    VoiceSubscribed(bool),
    VoiceEvent {
        event: VaEvent,
        data: HashMap<String, String>,
    },
    Announce(AnnounceRequest),
    Timer(TimerInfo),
    /// HA изменила активные wake word (id известны, непусты). Сообщаемый набор меняется только
    /// через `ApiHandle::set_active_wake_words`.
    ActiveWakeWords(Vec<String>),
    MediaCommand(MediaCommandRequest),
    Switch(SwitchId, bool),
    Button(ButtonId),
    /// `transition_ms`: длина перехода команды, по умолчанию ESPHome (1 с) без неё.
    Light {
        state: LightState,
        transition_ms: u32,
    },
    Number(NumberId, f32),
    /// Не длиннее 64 символов UTF-8.
    Text(TextId, String),
    /// Одна из опций select.
    Select(SelectId, String),
    /// Пользовательское действие: `value` — текст (не длиннее 64 символов) или лицо ("" или
    /// одно из `ApiConfig::clock_faces`).
    Action {
        id: ActionId,
        value: String,
        seconds: i32,
    },
    /// Action led_show: эффект "" (сплошной цвет) или один из `ApiConfig::light_effects`.
    LedShow {
        effect: String,
        color: String,
        seconds: i32,
    },
    /// Время от HA (GetTimeResponse); `posix_tz` — POSIX TZ-строка (например, "MSK-3"), может
    /// быть пустой.
    Time {
        epoch: u32,
        posix_tz: String,
    },
    /// Кнопка "Restart": демон должен выйти с кодом 0 (supervise.sh перезапустит).
    Restart,
    /// HA ответила ошибкой на VoiceAssistantRequest(start): пайплайн не запущен.
    VoiceStartFailed {
        run: u64,
    },
    /// Связь с HA отстала слишком сильно для микрофонного аудио; сервер уже отправил
    /// VoiceAssistantRequest(start=false), контроллер должен завершить run локально.
    VoiceAudioOverflow {
        run: u64,
    },
    /// Update entity: установить предложенный релиз или проверить сейчас.
    Update(UpdateCommand),
    /// Ключ шифрования изменился: TXT-записи `_esphomelib._tcp`, которые нужно объявить.
    MdnsTxtChanged(Vec<String>),
}

pub use esphome_proto::entity::{
    ActionId, BinarySensorId, ButtonId, EventId, NumberId, SelectId, SensorId, SwitchId, TextId,
    TextSensorId,
};

#[cfg(test)]
mod tests {
    use super::MediaState;

    #[test]
    fn announcing_is_published_as_playing() {
        // HA 2026.10 has no ANNOUNCING (4) in its media player state enum: the wire value must be
        // one it knows, while the internal state keeps its own code.
        assert_eq!(MediaState::Announcing.wire(), 2);
        assert_eq!(MediaState::Announcing as u32, 4);
        for state in [
            MediaState::None,
            MediaState::Idle,
            MediaState::Playing,
            MediaState::Paused,
            MediaState::Off,
            MediaState::On,
        ] {
            assert_eq!(state.wire(), state as u32, "{state:?}");
        }
    }
}
