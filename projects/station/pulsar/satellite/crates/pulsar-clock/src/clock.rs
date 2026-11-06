//! 4-digit 7-segment clock, one sysfs LED per segment (names per `Model`), and the clock actor.
//!
//! Station Midi: two aw21036/aw21012 chips on i2c 3-0037 / 3-0021,
//! /sys/class/leds/cucumber:white:clock-<seg><digit>.
//!
//! Segment mapping (derived from the live display driven by maind: at 21:17 local time the lit
//! set was a1 b1 d1 e1 g1 | b2 c2 | b3 c3 | a4 b4 c4 plus p3 q3, i.e. "21:17"):
//! clock-<s><n>: n = digit position 1..4 left to right, s = standard 7-segment letter
//!
//! ```text
//!          aaa
//!         f   b
//!          ggg          digits: [1][2] : [3][4]
//!         e   c
//!          ddd
//! ```
//!
//! - p3 + q3: colon (upper/lower dot), the only extra LEDs maind lights for the time.
//! - deg: degree sign; h1 h2, l1 l2, m1 m2, p1 p2 p4 p5, q1 q2 q4 q5: auxiliary dots/icons whose
//!   position is not known (never lit by maind for the time); kept off here.
//! - cucumber:sync:clock-0/1 are the chips' sync pseudo-LEDs (not segments).
//!
//! Midi brightness: like maind, segments are switched 0/255 and dimming uses the chips' global
//! current attribute (/sys/bus/i2c/devices/{3-0037,3-0021}/current, 0..255; maind wrote 5 for
//! its stored brightness 0.0196 = 5/255). The brightness_scale tables are left untouched.
//!
//! Station Lite 2: /sys/class/leds/plum:white:clock<SEG><pos>, SEG = A..G (standard segments),
//! pos = 0..3 left to right, colon = DH + DL; H0..H3 and DEGREE stay dark (maind's /leds/clock
//! frameBuilder config in quasar.cfg), DM only lights in the voice animations
//! (`Model::clock_center`). No global current: maind dims through the per-segment brightness
//! value (0..255), and so do we.
//!
//! Layers and their priority live in `pulsar-ui`; this module owns the sysfs output and timers.

use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd as _;
use std::path::PathBuf;

use home_linux as sysfs;
use pulsar_core::{AmbientDimmer, ClockShot, MusicFrame, Timers, UiState, now_ms};
use pulsar_model::ClockPanel;
use pulsar_ui::font::DIGITS;
use pulsar_ui::{ClockFace, ClockLayers, ClockMusicStyle, Frame, MusicSource, Wall};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::sys;

const SEGS: usize = 7; // a..g
const TICK_MS: i64 = 250;
/// Every 20 ticks (~5 s) all segments are rewritten even if unchanged (sysfs state may be
/// changed by others); animation frames in between do not count.
const FULL_REWRITE_TICKS: u32 = 20;
/// Lowest current / brightness that still lights the segments.
const MIN_LEVEL: i32 = 2;

/// Names of `set_face` indices: the 17 static faces, then "Blink", "All faces".
pub fn face_names() -> [&'static str; ClockFace::ALL.len()] {
    ClockFace::ALL.map(ClockFace::as_str)
}

/// Clock music styles ("Clock music" select).
pub fn music_styles() -> [&'static str; ClockMusicStyle::ALL.len()] {
    ClockMusicStyle::ALL.map(ClockMusicStyle::as_str)
}

/// Output level (chip current on the Midi, value of a lit segment on the Lite 2) for the user
/// brightness 0..1 and the ambient factor 0.02..1 (1 with auto brightness off): 0 only when the
/// user turned it to 0.
fn level(user_bri: f32, ambient: f32) -> i32 {
    if user_bri <= 0.0 {
        return 0;
    }
    ((255.0 * user_bri * ambient).round() as i32).clamp(MIN_LEVEL, 255)
}

/// The open sysfs attributes and what was last written to them (-1 = unknown).
struct Leds {
    seg: [[Option<File>; SEGS]; DIGITS],
    colon: Vec<File>,
    center: Option<File>,
    current: Vec<File>,
    chip_current: bool,
    seg_last: [[i32; SEGS]; DIGITS],
    colon_last: i32,
    center_last: i32,
    level_last: i32,
    /// Value of a lit segment (the dimmed level when there is no chip current).
    on_value: i32,
}

fn write_int(f: &File, v: i32) -> bool {
    sysfs::write_int_fd(f.as_raw_fd(), v)
}

impl Leds {
    fn seg_name(panel: &ClockPanel, digit: usize, seg: usize) -> String {
        let letter = char::from_u32(panel.seg_base as u32 + seg as u32).unwrap_or('?');
        format!("{letter}{}", panel.pos_base + digit as i32)
    }

    fn open_led(panel: &ClockPanel, name: &str) -> Option<File> {
        let path = format!("{}{name}/brightness", panel.dir);
        OpenOptions::new().write(true).open(path).ok()
    }

    /// The segments, colon and centre LEDs of `panel`; aux LEDs are switched off once. None
    /// (logged) when no segment exists.
    fn open(panel: &ClockPanel) -> Option<Self> {
        let mut found = 0;
        let seg = std::array::from_fn(|d| {
            std::array::from_fn(|s| {
                let f = Self::open_led(panel, &Self::seg_name(panel, d, s));
                if f.is_some() {
                    found += 1;
                }
                f
            })
        });
        let total = DIGITS * SEGS;
        if found == 0 {
            // The whole panel is absent: one line, not one per LED.
            if panel.optional {
                info!("clock: no clock segments under {}", panel.dir);
            } else {
                error!("clock: no clock segments under {}", panel.dir);
            }
            return None;
        }
        if found < total {
            warn!("clock: {} of {total} segments missing", total - found);
        }
        let colon: Vec<File> = panel
            .colon
            .iter()
            .filter_map(|name| Self::open_led(panel, name))
            .collect();
        for aux in panel.aux {
            if let Some(f) = Self::open_led(panel, aux) {
                write_int(&f, 0);
            }
        }
        let mut center = None;
        if let Some(name) = panel.center {
            center = Self::open_led(panel, name);
            if center.is_none() {
                warn!("clock: missing centre LED {name}");
            }
        }
        let mut current = Vec::new();
        if panel.chip_current {
            // Global current of the clock driver chips, found through the LED device links of
            // the leftmost and rightmost digit (one chip each on the Midi).
            let mut opened: Vec<PathBuf> = Vec::new();
            for name in [
                Self::seg_name(panel, 0, 0),
                Self::seg_name(panel, DIGITS - 1, 0),
            ] {
                let dev = format!("{}{name}/device/current", panel.dir);
                let Ok(real) = std::fs::canonicalize(dev) else {
                    continue;
                };
                if opened.contains(&real) {
                    continue;
                }
                if let Ok(f) = OpenOptions::new().write(true).open(&real) {
                    current.push(f);
                    opened.push(real);
                }
            }
        }
        info!(
            "clock: {found} segments, {} colon LEDs, {} dimming",
            colon.len(),
            if panel.chip_current {
                "chip current"
            } else {
                "segment brightness"
            }
        );
        Some(Self {
            seg,
            colon,
            center,
            current,
            chip_current: panel.chip_current,
            seg_last: [[-1; SEGS]; DIGITS],
            colon_last: -1,
            center_last: -1,
            level_last: -1,
            on_value: 255,
        })
    }

    fn forget_written(&mut self) {
        self.seg_last = [[-1; SEGS]; DIGITS];
        self.colon_last = -1;
        self.center_last = -1;
    }

    fn scaled(&self, v: u8) -> i32 {
        (i32::from(v) * self.on_value + 127) / 255
    }

    /// Writes what changed since the last write.
    fn draw(&mut self, f: &Frame) {
        for d in 0..DIGITS {
            for s in 0..SEGS {
                let x = self.scaled(f.seg[d][s]);
                if self.seg_last[d][s] == x {
                    continue;
                }
                if let Some(file) = &self.seg[d][s]
                    && write_int(file, x)
                {
                    self.seg_last[d][s] = x;
                }
            }
        }
        let x = self.scaled(f.colon);
        if self.colon_last != x {
            for file in &self.colon {
                write_int(file, x);
            }
            self.colon_last = x;
        }
        let x = self.scaled(f.center);
        if self.center_last != x
            && let Some(file) = &self.center
            && write_int(file, x)
        {
            self.center_last = x;
        }
    }

    /// Applies an output level; true if it changed.
    fn set_level(&mut self, lvl: i32) -> bool {
        if lvl == self.level_last {
            return false;
        }
        self.level_last = lvl;
        if self.chip_current {
            for file in &self.current {
                write_int(file, lvl);
            }
        } else {
            // Lit segments are rewritten with the new value on the next refresh.
            self.on_value = lvl;
            self.forget_written();
        }
        true
    }
}

enum Cmd {
    SetEnabled(bool),
    SetBrightness(f32),
    SetAmbientLux(f32),
    SetAutoBrightness(bool),
    SetTimer { ms_left: i64, running: bool },
    FlashNumber(i32),
    ShowCode(String),
    ClearCode,
    SetText(String),
    SetFace(ClockFace),
    SetScrollMs(i32),
    ShowTemporaryText(String, i32),
    ShowTemporaryFace(ClockFace, i32),
    SetUiState(UiState),
    SetVoiceIndication(bool),
    Play(ClockShot),
    SetUpdating(bool),
    SetMusicStyle(ClockMusicStyle),
    SetMediaPlaying(bool),
    MusicFrame(MusicFrame, MusicSource),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Timer {
    Tick,
    /// One-shot frame timer of moving content, re-armed by every refresh.
    Anim,
}

struct Clock {
    layers: ClockLayers,
    leds: Leds,
    user_bri: f32,
    /// < 0: unknown, no auto-dimming (only logged).
    lux: f32,
    dimmer: AmbientDimmer,
    /// false: the level is the user brightness alone; readings still feed `dimmer`, so turning
    /// it back on applies the current light at once.
    auto_bri: bool,
    refreshes: u32,
    timers: Timers<Timer>,
}

impl Clock {
    fn apply_level(&mut self) {
        let ambient = if self.auto_bri {
            self.dimmer.factor()
        } else {
            1.0
        };
        let lvl = level(self.user_bri, ambient);
        if self.leds.set_level(lvl) {
            debug!(
                "clock: level={lvl} (user {:.2}, lux {:.1})",
                self.user_bri, self.lux
            );
        }
    }

    fn refresh(&mut self) {
        let (frame, next) = self.layers.compose(now_ms(), wall);
        self.leds.draw(&frame);
        self.timers.cancel(Timer::Anim);
        if next > 0 {
            self.timers.set(Timer::Anim, next as u64);
        }
    }

    fn handle(&mut self, cmd: Cmd) {
        let now = now_ms();
        let l = &mut self.layers;
        match cmd {
            Cmd::SetEnabled(on) => l.set_enabled(on),
            Cmd::SetBrightness(b) => {
                // NaN counts as full brightness, as std::max(0, std::min(1, b)) did.
                self.user_bri = if b.is_nan() { 1.0 } else { b.clamp(0.0, 1.0) };
                self.apply_level();
            }
            Cmd::SetAmbientLux(lux) => {
                self.lux = lux.max(0.0);
                self.dimmer.update(self.lux);
                self.apply_level();
            }
            Cmd::SetAutoBrightness(on) => {
                self.auto_bri = on;
                self.apply_level();
            }
            Cmd::SetTimer { ms_left, running } => l.set_timer(ms_left, running, now),
            Cmd::FlashNumber(v) => l.flash_number(v, now),
            Cmd::ShowCode(digits) => l.show_code(digits, now),
            Cmd::ClearCode => l.clear_code(),
            Cmd::SetText(s) => l.set_text(&s, now),
            Cmd::SetFace(face) => l.set_face(face, now),
            Cmd::SetScrollMs(ms) => l.set_scroll_ms(ms),
            Cmd::ShowTemporaryText(s, secs) => l.show_temporary_text(&s, secs, now),
            Cmd::ShowTemporaryFace(face, secs) => l.show_temporary_face(face, secs, now),
            Cmd::SetUiState(s) => l.set_ui_state(s, now),
            Cmd::SetVoiceIndication(on) => l.set_voice_indication(on, now),
            Cmd::Play(s) => l.play(s, now),
            Cmd::SetUpdating(on) => l.set_updating(on),
            Cmd::SetMusicStyle(style) => l.set_music_style(style),
            Cmd::SetMediaPlaying(on) => l.set_media_playing(on),
            Cmd::MusicFrame(f, source) => {
                // Frames only feed the meters; the frame timer draws them.
                l.music_frame(&f, source, now);
                return;
            }
        }
        self.refresh();
    }

    async fn run(mut self, mut rx: mpsc::UnboundedReceiver<Cmd>) {
        loop {
            tokio::select! {
                biased;
                t = self.timers.next() => {
                    if t == Timer::Tick {
                        self.refreshes = self.refreshes.wrapping_add(1);
                        if self.refreshes.is_multiple_of(FULL_REWRITE_TICKS) {
                            self.leds.forget_written();
                        }
                    }
                    self.refresh();
                }
                cmd = rx.recv() => match cmd {
                    Some(cmd) => self.handle(cmd),
                    None => break,
                },
            }
        }
    }
}

/// Whether any clock segment of `panel` exists in sysfs (decides the clock entities of a model
/// whose panel may be missing, `ClockPanel::optional`).
pub fn panel_present(panel: &ClockPanel) -> bool {
    (0..DIGITS).any(|d| {
        (0..SEGS).any(|s| {
            let led = format!("{}{}/brightness", panel.dir, Leds::seg_name(panel, d, s));
            std::path::Path::new(&led).exists()
        })
    })
}

/// Clock control; every method queues a command for the clock actor and returns at once. The
/// handle of a clock that could not be opened does nothing.
#[derive(Clone)]
pub struct ClockHandle {
    tx: Option<mpsc::UnboundedSender<Cmd>>,
}

/// Opens the clock LEDs of `panel` (sysfs `<dir><segment><position>`) and starts the clock
/// actor, which stops when every handle is dropped. Without a panel: no-op handle.
pub fn spawn(panel: Option<&'static ClockPanel>) -> ClockHandle {
    // None means the model has no display at all (the Mini): no sysfs probe.
    let Some(panel) = panel else {
        return ClockHandle { tx: None };
    };
    let Some(leds) = Leds::open(panel) else {
        if !panel.optional {
            warn!("clock unavailable");
        }
        return ClockHandle { tx: None };
    };
    let mut clock = Clock {
        layers: ClockLayers::new(),
        leds,
        user_bri: 1.0,
        lux: -1.0,
        dimmer: AmbientDimmer::new(),
        auto_bri: true,
        refreshes: 0,
        timers: Timers::new(),
    };
    let tick = TICK_MS as u64;
    clock.timers.set_repeating(Timer::Tick, tick, tick);
    clock.apply_level();
    clock.refresh();
    let (tx, rx) = mpsc::unbounded_channel();
    pulsar_core::supervisor().spawn_local("clock", clock.run(rx));
    ClockHandle { tx: Some(tx) }
}

impl ClockHandle {
    fn send(&self, cmd: Cmd) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(cmd);
        }
    }

    pub fn set_enabled(&self, on: bool) {
        self.send(Cmd::SetEnabled(on));
    }

    /// 0..1 user scale; with auto brightness on, the ceiling of the ambient auto-dimming.
    pub fn set_brightness(&self, b: f32) {
        self.send(Cmd::SetBrightness(b));
    }

    pub fn set_ambient_lux(&self, lux: f32) {
        self.send(Cmd::SetAmbientLux(lux));
    }

    /// On: the level follows the light sensor (the 0..1 scale is the ceiling). Off: the level
    /// is the 0..1 scale alone, whatever the light.
    pub fn set_auto_brightness(&self, on: bool) {
        self.send(Cmd::SetAutoBrightness(on));
    }

    /// Shows a countdown mm:ss (or h:mm) instead of the time while `ms_left > 0`; <= 0 clears.
    /// `running`: counts down on its own from `ms_left`; otherwise `ms_left` stays shown
    /// (paused timer).
    pub fn set_timer(&self, ms_left: i64, running: bool) {
        self.send(Cmd::SetTimer { ms_left, running });
    }

    /// Shows a number (volume 0..100) for 1.5 s.
    pub fn flash_number(&self, value: i32) {
        self.send(Cmd::FlashNumber(value));
    }

    /// Shows a pairing code (digits) until `clear_code`, over everything else, even when the
    /// clock is disabled. Codes longer than 4 digits page through 3-digit groups.
    pub fn show_code(&self, digits: String) {
        self.send(Cmd::ShowCode(digits));
    }

    pub fn clear_code(&self) {
        self.send(Cmd::ClearCode);
    }

    /// Custom text (Text entity), UTF-8: one cell per character, ':' lights the colon after the
    /// preceding character instead of taking a cell; up to 4 cells are shown static and
    /// centred, longer text scrolls right-to-left in a loop. `set_text` and `set_face` replace
    /// each other; `set_text("")` / `set_face(ClockFace::None)` clear only their own kind.
    pub fn set_text(&self, utf8: String) {
        self.send(Cmd::SetText(utf8));
    }

    /// Face of `face_names()`; `ClockFace::None` = no face.
    pub fn set_face(&self, face: ClockFace) {
        self.send(Cmd::SetFace(face));
    }

    /// Scroll step, clamped to 100..2000 ms.
    pub fn set_scroll_ms(&self, ms: i32) {
        self.send(Cmd::SetScrollMs(ms));
    }

    /// Temporary layer above the text / face. `seconds <= 0`: one pass of a scrolling text, 5 s
    /// otherwise; capped at 3600 s. Replaces the previous layer; `ClockFace::None` or "" removes
    /// it.
    pub fn show_temporary_text(&self, utf8: String, seconds: i32) {
        self.send(Cmd::ShowTemporaryText(utf8, seconds));
    }

    /// As `show_temporary_text` with a face; `seconds <= 0`: 5 s; `ClockFace::None` removes the
    /// layer.
    pub fn show_temporary_face(&self, face: ClockFace, seconds: i32) {
        self.send(Cmd::ShowTemporaryFace(face, seconds));
    }

    pub fn set_ui_state(&self, s: UiState) {
        self.send(Cmd::SetUiState(s));
    }

    pub fn set_voice_indication(&self, on: bool) {
        self.send(Cmd::SetVoiceIndication(on));
    }

    /// Restarts / replaces a running one-shot.
    pub fn play(&self, s: ClockShot) {
        self.send(Cmd::Play(s));
    }

    /// Dark while an update installs.
    pub fn set_updating(&self, on: bool) {
        self.send(Cmd::SetUpdating(on));
    }

    pub fn set_music_style(&self, style: ClockMusicStyle) {
        self.send(Cmd::SetMusicStyle(style));
    }

    pub fn set_media_playing(&self, playing: bool) {
        self.send(Cmd::SetMediaPlaying(playing));
    }

    /// Music features, ~30 per second while playing; Sendspin frames take precedence over the
    /// speaker reference while they keep coming. Shown only while media plays and the style is
    /// not "Off".
    pub fn music_frame(&self, f: MusicFrame, from_sendspin: bool) {
        let source = if from_sendspin {
            MusicSource::Sendspin
        } else {
            MusicSource::Speaker
        };
        self.send(Cmd::MusicFrame(f, source));
    }
}

fn wall() -> Wall {
    sys::wall()
}

#[cfg(test)]
mod tests;
