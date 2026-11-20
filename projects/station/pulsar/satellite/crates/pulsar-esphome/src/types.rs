use std::collections::HashMap;
use std::path::PathBuf;

use esphome_proto::entity::{Capabilities, EntityOptions};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeviceIdentity {
    pub name: String,
    pub friendly_name: String,
    pub mac: String,
    pub model: String,
    pub board: String,
    pub manufacturer: String,
    pub project_version: String,
    pub compilation_time: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WakeWordInfo {
    pub id: String,
    pub phrase: String,
    pub languages: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiConfig {
    pub port: u16,
    pub capabilities: Capabilities,
    pub wake_words: Vec<WakeWordInfo>,
    pub active_wake_words: Vec<String>,
    pub light_effects: Vec<String>,
    pub clock_faces: Vec<String>,
    pub music_styles: Vec<String>,
    pub case_colors: Vec<String>,
    pub clock_music_styles: Vec<String>,
    pub voice_colors: Vec<String>,
    pub zigbee_modes: Vec<String>,
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
    pub fn wire(self) -> u32 {
        match self {
            Self::Announcing => Self::Playing as u32,
            other => other as u32,
        }
    }
}

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
    pub brightness: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
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

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UpdateCommand {
    None = 0,
    Install = 1,
    Check = 2,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct UpdateState {
    pub in_progress: bool,
    pub has_progress: bool,
    pub progress: f32,
    pub current_version: String,
    pub latest_version: String,
    pub title: String,
    pub release_url: String,
}

pub const FOLLOWUP_DURATION_MAX_S: u32 = esphome_proto::entity::FOLLOWUP_DURATION_MAX_S;

pub fn followup_duration_s(v: f32) -> Option<u32> {
    v.is_finite()
        .then(|| v.round().clamp(0.0, FOLLOWUP_DURATION_MAX_S as f32) as u32)
}

pub const MIC_GAIN_MAX_DB: u32 = 40;

pub fn mic_gain_db(v: f32) -> Option<u32> {
    v.is_finite()
        .then(|| v.round().clamp(0.0, MIC_GAIN_MAX_DB as f32) as u32)
}

#[derive(Clone, Debug, PartialEq)]
pub enum ApiEvent {
    ClientConnected {
        client_info: String,
    },
    ClientDisconnected,
    VoiceSubscribed(bool),
    VoiceEvent {
        event: VaEvent,
        data: HashMap<String, String>,
    },
    Announce(AnnounceRequest),
    Timer(TimerInfo),
    ActiveWakeWords(Vec<String>),
    MediaCommand(MediaCommandRequest),
    Switch(SwitchId, bool),
    Button(ButtonId),
    Light {
        state: LightState,
        transition_ms: u32,
    },
    Number(NumberId, f32),
    Text(TextId, String),
    Select(SelectId, String),
    Action {
        id: ActionId,
        value: String,
        seconds: i32,
    },
    LedShow {
        effect: String,
        color: String,
        seconds: i32,
    },
    Time {
        epoch: u32,
        posix_tz: String,
    },
    Restart,
    VoiceStartFailed {
        run: u64,
    },
    VoiceAudioOverflow {
        run: u64,
    },
    Update(UpdateCommand),
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
