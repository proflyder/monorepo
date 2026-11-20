use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt as _;

use pulsar_esphome::{Capabilities, DeviceIdentity, MediaState, VaEvent};
use pulsar_hw::Rgb;
use pulsar_media::PlayState;
use pulsar_model::{DeviceProfile, Identity};

pub const COMMAND_AFTER_WAKE_MS: i64 = 2500;

pub const PAUSED_MUSIC_MS: i64 = 2 * 60 * 60 * 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Listening,
    Processing,
    Speaking,
    Call,
}

pub fn cxx_min(a: f32, b: f32) -> f32 {
    if b < a { b } else { a }
}

pub fn cxx_max(a: f32, b: f32) -> f32 {
    if a < b { b } else { a }
}

pub fn cxx_clamp(v: f32, lo: f32, hi: f32) -> f32 {
    if v < lo {
        lo
    } else if hi < v {
        hi
    } else {
        v
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ports {
    pub offset: u16,
    pub api: u16,
    pub sendspin: u16,
    pub deconz: u16,
    pub arbiter: u16,
    pub metrics: u16,
}

pub const API_PORT: u16 = 6053;
pub const SENDSPIN_PORT: u16 = 8928;
pub const DECONZ_PORT: u16 = 6638;
pub const ARBITER_PORT: u16 = 6054;
pub const METRICS_PORT: u16 = pulsar_metrics::DEFAULT_PORT;

pub fn ports(offset: Option<&OsStr>) -> Option<Ports> {
    let offset = match offset {
        None => 0,
        Some(s) => s.to_str()?.parse::<u16>().ok()?,
    };
    Some(Ports {
        offset,
        api: API_PORT.checked_add(offset)?,
        sendspin: SENDSPIN_PORT.checked_add(offset)?,
        deconz: DECONZ_PORT.checked_add(offset)?,
        arbiter: ARBITER_PORT.checked_add(offset)?,
        metrics: METRICS_PORT.checked_add(offset)?,
    })
}

pub fn state_dir(state_path: &OsStr) -> std::path::PathBuf {
    let b = state_path.as_bytes();
    b.iter()
        .rposition(|&c| c == b'/')
        .map_or_else(|| ".".into(), |i| OsStr::from_bytes(&b[..i]).into())
}

pub fn in_dir(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let mut s = dir.as_os_str().to_owned();
    s.push("/");
    s.push(name);
    s.into()
}

pub fn identity(
    identity: &Identity,
    mac_line: &str,
    version: &str,
    compilation_time: &str,
) -> DeviceIdentity {
    let mac = mac_line.to_ascii_uppercase();
    let plain: String = mac
        .chars()
        .filter(|&c| c != ':')
        .map(|c| c.to_ascii_lowercase())
        .collect();
    let suffix = if plain.len() >= 6 {
        &plain[plain.len() - 6..]
    } else {
        &plain[..]
    };
    DeviceIdentity {
        name: format!("{}{suffix}", identity.host_prefix),
        friendly_name: identity.friendly_name.into(),
        mac,
        model: identity.model_name.into(),
        board: identity.codename.into(),
        manufacturer: "Yandex".into(),
        project_version: version.into(),
        compilation_time: compilation_time.into(),
    }
}

pub fn capabilities(
    profile: &DeviceProfile,
    panel_present: bool,
    lux_present: bool,
    case_colors: bool,
    voice_colors: bool,
    zigbee: bool,
) -> Capabilities {
    Capabilities {
        clock: profile
            .clock
            .as_ref()
            .is_some_and(|panel| !panel.optional || panel_present),
        lux: profile
            .lux
            .as_ref()
            .is_some_and(|sensor| !sensor.optional || lux_present),
        auto_brightness: lux_present,
        wake_preroll: profile.wake_preroll.is_some(),
        hw_mic_mute: profile.input.hardware_mute(),
        buttons: profile.input.has_button_events(),
        case_colors,
        voice_colors,
        zigbee,
    }
}

pub fn initial_volume(
    has_volume: bool,
    volume: f32,
    volume_legacy: f32,
    legacy_on_stock: bool,
    mixer_volume: f32,
    from_legacy: impl Fn(f32) -> f32,
) -> (f32, bool) {
    if has_volume {
        (volume, false)
    } else if volume_legacy >= 0.0 {
        let v = if legacy_on_stock {
            cxx_min(1.0, volume_legacy)
        } else {
            from_legacy(volume_legacy)
        };
        (v, true)
    } else {
        (cxx_min(mixer_volume, 0.3), false)
    }
}

pub fn parse_led_color(color: &str) -> Option<Rgb> {
    if color.is_empty() {
        return Some(Rgb {
            r: 255,
            g: 255,
            b: 255,
        });
    }
    let hex = color.strip_prefix('#').unwrap_or(color);
    if hex.len() != 6 || !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(hex, 16).ok()?;
    Some(Rgb {
        r: (v >> 16) as u8,
        g: (v >> 8) as u8,
        b: v as u8,
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HaTimer {
    pub active: bool,
    pub end_ms: i64,
    pub seconds_left: u32,
}

pub fn pick_timer(timers: &BTreeMap<String, HaTimer>, now: i64) -> Option<(&str, HaTimer, i64)> {
    let mut best: Option<(&str, HaTimer, i64)> = None;
    for (id, tm) in timers {
        let left = if tm.active {
            tm.end_ms - now
        } else {
            i64::from(tm.seconds_left) * 1000
        };
        if left <= 0 {
            continue;
        }
        let better = match &best {
            None => true,
            Some((_, b, bl)) => (tm.active && !b.active) || (tm.active == b.active && left < *bl),
        };
        if better {
            best = Some((id, *tm, left));
        }
    }
    best
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceCommand {
    Stop,
    Pause,
    Resume,
    Louder,
    Quieter,
    Next,
    Prev,
}

pub fn command_of(word: &str) -> Option<VoiceCommand> {
    Some(match word {
        "стоп" | "хватит" => VoiceCommand::Stop,
        "пауза" => VoiceCommand::Pause,
        "продолжи" | "играй" => VoiceCommand::Resume,
        "громче" => VoiceCommand::Louder,
        "тише" => VoiceCommand::Quieter,
        "дальше" | "следующий" => VoiceCommand::Next,
        "назад" => VoiceCommand::Prev,
        _ => return None,
    })
}

pub fn stt_stop(text: &str) -> bool {
    let text = text.to_lowercase();
    let sep = |c: char| {
        c.is_whitespace()
            || c.is_ascii_punctuation()
            || matches!(c, '«' | '»' | '\u{2010}'..='\u{2027}' | '‹' | '›')
    };
    let mut words = text.split(sep).filter(|w| !w.is_empty()).peekable();
    words.next_if(|w| matches!(*w, "алиса" | "яндекс"));
    words.peek().is_some() && words.all(|w| command_of(w) == Some(VoiceCommand::Stop))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SttStopAction {
    None,
    Cancel,
}

pub fn stt_stop_action(text: &str, wake_dialog: bool) -> SttStopAction {
    if wake_dialog || !stt_stop(text) {
        SttStopAction::None
    } else {
        SttStopAction::Cancel
    }
}

pub fn after_wake(phase: Phase, wake_dialog: bool, now: i64, listen_started_ms: i64) -> bool {
    wake_dialog
        && matches!(phase, Phase::Listening | Phase::Processing)
        && now - listen_started_ms < COMMAND_AFTER_WAKE_MS
}

pub fn owns_music(
    now: i64,
    music: PlayState,
    music_paused_since: Option<i64>,
    sendspin_streaming: bool,
    sendspin_connected: bool,
    sendspin_ended_since: Option<i64>,
) -> bool {
    let recent = |t: Option<i64>| t.is_some_and(|t| now - t < PAUSED_MUSIC_MS);
    matches!(music, PlayState::Playing | PlayState::Loading)
        || (music == PlayState::Paused && recent(music_paused_since))
        || sendspin_streaming
        || (sendspin_connected && recent(sendspin_ended_since))
}

pub fn command_allowed(
    c: VoiceCommand,
    wake: bool,
    in_dialog: bool,
    sounding: bool,
    owns_music: bool,
) -> bool {
    if wake {
        return true;
    }
    if in_dialog {
        return c == VoiceCommand::Stop;
    }
    match c {
        VoiceCommand::Stop => true,
        VoiceCommand::Louder | VoiceCommand::Quieter => sounding || owns_music,
        VoiceCommand::Pause | VoiceCommand::Resume | VoiceCommand::Next | VoiceCommand::Prev => {
            owns_music
        }
    }
}

pub fn media_state(announcing: bool, music: PlayState, sendspin_streaming: bool) -> MediaState {
    if announcing {
        return MediaState::Announcing;
    }
    match music {
        PlayState::Playing | PlayState::Loading => MediaState::Playing,
        PlayState::Paused if sendspin_streaming => MediaState::Playing,
        PlayState::Paused => MediaState::Paused,
        _ if sendspin_streaming => MediaState::Playing,
        _ => MediaState::Idle,
    }
}

pub fn want_duck(
    phase: Phase,
    announcing: bool,
    timer_ringing: bool,
    closing_earcon: bool,
) -> bool {
    phase != Phase::Idle || announcing || timer_ringing || closing_earcon
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WakeCue {
    Keep,
    Sound,
    Cut,
}

pub fn wake_cue(earcon: bool, wake_sound: bool) -> WakeCue {
    match (earcon, wake_sound) {
        (false, _) => WakeCue::Keep,
        (true, true) => WakeCue::Sound,
        (true, false) => WakeCue::Cut,
    }
}

pub fn want_volume_click(
    before: (f32, bool),
    after: (f32, bool),
    phase: Phase,
    arbitrating: bool,
    announcing: bool,
    timer_ringing: bool,
    closing_earcon: bool,
) -> bool {
    let percent = |v: f32| (v * 100.0).round() as i32;
    (percent(before.0), before.1) != (percent(after.0), after.1)
        && matches!(phase, Phase::Idle | Phase::Processing)
        && !arbitrating
        && !announcing
        && !timer_ringing
        && !closing_earcon
}

pub fn sw_mute_after_switch(on: bool, sw_muted: bool, hw_muted: bool) -> bool {
    if on && hw_muted { sw_muted } else { !on }
}

pub fn sw_mute_after_button(sw_muted: bool, hw_muted: bool) -> bool {
    sw_muted && hw_muted
}

pub fn idle_for_update(
    phase: Phase,
    announcing: bool,
    timer_ringing: bool,
    music: PlayState,
    sendspin_streaming: bool,
    zigbee_flashing: bool,
) -> bool {
    phase == Phase::Idle
        && !announcing
        && !timer_ringing
        && music != PlayState::Playing
        && !sendspin_streaming
        && !zigbee_flashing
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoUpdate {
    No,
    WaitIdle,
    Install,
}

pub fn auto_update(
    enabled: bool,
    release: &str,
    installing: bool,
    current: &str,
    update_attempt: &str,
    idle: bool,
) -> AutoUpdate {
    if !enabled || release.is_empty() || installing {
        return AutoUpdate::No;
    }
    let (newer, current_known) = pulsar_update::release_newer(release, current);
    if !newer || !current_known || release == update_attempt {
        AutoUpdate::No
    } else if !idle {
        AutoUpdate::WaitIdle
    } else {
        AutoUpdate::Install
    }
}

pub fn strip_deleted(exe: &OsStr) -> &OsStr {
    const DELETED: &[u8] = b" (deleted)";
    let b = exe.as_bytes();
    if b.len() > DELETED.len() && b.ends_with(DELETED) {
        OsStr::from_bytes(&b[..b.len() - DELETED.len()])
    } else {
        exe
    }
}

pub const FOLLOWUP_VAD_GRACE_MS: u64 = 1000;

#[derive(Clone, Copy, Debug, Default)]
pub struct Followup {
    requested: bool,
    failed: bool,
    vad_seen: bool,
    awaiting_speech: bool,
    timed: bool,
}

impl Followup {
    pub fn on_event(&mut self, event: VaEvent, continue_conversation: bool) {
        match event {
            VaEvent::RunStart => {
                self.requested = false;
                self.failed = false;
                self.vad_seen = false;
            }
            VaEvent::IntentEnd => self.requested = continue_conversation && !self.failed,
            VaEvent::Error => {
                self.failed = true;
                self.requested = false;
            }
            VaEvent::SttVadStart => {
                self.vad_seen = true;
                self.awaiting_speech = false;
            }
            _ => {}
        }
    }

    pub fn should_continue(&self, ok: bool, seconds: u32, blocked: bool) -> bool {
        ok && self.requested && !blocked && seconds > 0
    }

    pub fn next_turn(self) -> Self {
        Self {
            awaiting_speech: true,
            timed: self.vad_seen,
            ..Self::default()
        }
    }

    pub fn window_ms(&self, seconds: u32) -> Option<u64> {
        (self.awaiting_speech && self.timed && seconds > 0)
            .then(|| u64::from(seconds) * 1000 + FOLLOWUP_VAD_GRACE_MS)
    }

    pub fn silent(&self, code: &str) -> bool {
        self.awaiting_speech && code == "stt-no-text-recognized"
    }

    pub fn awaiting_speech(&self) -> bool {
        self.awaiting_speech
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RunBinding {
    pub awaiting_run_start: bool,
    pub run_active: bool,
    pub stale_run_end: bool,
}

impl RunBinding {
    pub fn start(&mut self) {
        self.stale_run_end = self.run_active;
        self.awaiting_run_start = true;
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn accept(&mut self, ev: VaEvent) -> bool {
        if ev == VaEvent::RunStart {
            self.run_active = true;
            self.awaiting_run_start = false;
        } else if self.awaiting_run_start {
            if ev != VaEvent::RunEnd || self.stale_run_end {
                if ev == VaEvent::RunEnd {
                    self.stale_run_end = false;
                    self.run_active = false;
                }
                return false;
            }
            self.awaiting_run_start = false;
        } else if ev == VaEvent::RunEnd {
            self.run_active = false;
        }
        true
    }
}

pub fn publish_lux(lux: f32, last: f32, last_ms: i64, now: i64) -> bool {
    let diff = (lux - last).abs();
    last < 0.0 || diff > cxx_max(1.0, last * 0.1) || now - last_ms > 60_000
}

#[cfg(test)]
mod tests;
