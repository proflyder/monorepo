use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd as _;
use std::path::PathBuf;

use home_linux as sysfs;
use pulsar_core::{
    AmbientDimmer, ClockShot, DARK, MusicFrame, Timers, UiState, now_ms, perceptual,
};
pub use pulsar_model::ClockPanel as PanelLayout;
use pulsar_ui::font::DIGITS;
use pulsar_ui::{ClockFace, ClockLayers, ClockMusicStyle, Frame, MusicSource, Wall};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

#[derive(Debug, thiserror::Error)]
pub enum ClockError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("light: iio device {0} not found")]
    SensorMissing(String),
    #[error("light: cannot read {0}")]
    SensorUnreadable(String),
}

pub trait ClockPanel {
    fn draw(&mut self, frame: &Frame, level: i32) -> Result<(), ClockError>;

    fn invalidate(&mut self) {}
}

pub type Lux = f32;

pub trait WallClock {
    fn now(&self) -> Wall;
}

struct SystemClock;

impl WallClock for SystemClock {
    fn now(&self) -> Wall {
        let w = home_linux::wall_time();
        Wall {
            hour: w.hour,
            min: w.min,
            first_half: w.first_half,
        }
    }
}

const SEGS: usize = 7;
const TICK_MS: i64 = 250;
const FULL_REWRITE_TICKS: u32 = 20;
const MIN_LEVEL: i32 = 2;

pub fn face_names() -> [&'static str; ClockFace::ALL.len()] {
    ClockFace::ALL.map(ClockFace::as_str)
}

pub fn music_styles() -> [&'static str; ClockMusicStyle::ALL.len()] {
    ClockMusicStyle::ALL.map(ClockMusicStyle::as_str)
}

pub fn level(user_bri: f32, ambient: f32) -> i32 {
    if user_bri <= 0.0 {
        return 0;
    }
    let a = ambient.clamp(0.0, 1.0);
    let perceived = DARK + (1.0 - DARK) * perceptual(a);
    ((255.0 * user_bri * perceived).round() as i32).clamp(MIN_LEVEL, 255)
}

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
    on_value: i32,
}

fn write_int(f: &File, v: i32) -> bool {
    sysfs::write_int_fd(f.as_raw_fd(), v)
}

impl Leds {
    fn seg_name(panel: &PanelLayout, digit: usize, seg: usize) -> String {
        let letter = char::from_u32(panel.seg_base as u32 + seg as u32).unwrap_or('?');
        format!("{letter}{}", panel.pos_base + digit as i32)
    }

    fn open_led(panel: &PanelLayout, name: &str) -> Option<File> {
        let path = format!("{}{name}/brightness", panel.dir);
        OpenOptions::new().write(true).open(path).ok()
    }

    fn open(panel: &PanelLayout) -> Option<Self> {
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
            self.on_value = lvl;
            self.forget_written();
        }
        true
    }
}

impl ClockPanel for Leds {
    fn draw(&mut self, frame: &Frame, level: i32) -> Result<(), ClockError> {
        self.set_level(level);
        Self::draw(self, frame);
        Ok(())
    }

    fn invalidate(&mut self) {
        self.forget_written();
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
    SetColonBlink(bool),
    Play(ClockShot),
    SetUpdating(bool),
    SetMusicStyle(ClockMusicStyle),
    SetMediaPlaying(bool),
    MusicFrame(MusicFrame, MusicSource),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Timer {
    Tick,
    Anim,
}

struct Clock {
    layers: ClockLayers,
    panel: Box<dyn ClockPanel>,
    wall: Box<dyn WallClock>,
    level: i32,
    user_bri: f32,
    lux: f32,
    dimmer: AmbientDimmer,
    auto_bri: bool,
    refreshes: u32,
    timers: Timers<Timer>,
}

impl Clock {
    fn refresh(&mut self) {
        let ambient = if self.auto_bri {
            self.dimmer.factor()
        } else {
            1.0
        };
        let lvl = level(self.user_bri, ambient);
        if lvl != self.level {
            debug!(
                "clock: level={lvl} (user {:.2}, lux {:.1})",
                self.user_bri, self.lux
            );
            self.level = lvl;
        }
        let wall = self.wall.now();
        let (frame, next) = self.layers.compose(now_ms(), || wall);
        if let Err(e) = self.panel.draw(&frame, lvl) {
            warn!("clock: draw: {e}");
        }
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
                self.user_bri = if b.is_nan() { 1.0 } else { b.clamp(0.0, 1.0) };
            }
            Cmd::SetAmbientLux(lux) => {
                self.lux = lux.max(0.0);
                self.dimmer.update(self.lux);
            }
            Cmd::SetAutoBrightness(on) => {
                self.auto_bri = on;
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
            Cmd::SetColonBlink(on) => l.set_colon_blink(on),
            Cmd::Play(s) => l.play(s, now),
            Cmd::SetUpdating(on) => l.set_updating(on),
            Cmd::SetMusicStyle(style) => l.set_music_style(style),
            Cmd::SetMediaPlaying(on) => l.set_media_playing(on),
            Cmd::MusicFrame(f, source) => {
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
                            self.panel.invalidate();
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

pub fn panel_present(panel: &PanelLayout) -> bool {
    (0..DIGITS).any(|d| {
        (0..SEGS).any(|s| {
            let led = format!("{}{}/brightness", panel.dir, Leds::seg_name(panel, d, s));
            std::path::Path::new(&led).exists()
        })
    })
}

#[derive(Clone)]
pub struct ClockHandle {
    tx: Option<mpsc::UnboundedSender<Cmd>>,
}

pub fn spawn(panel: Option<&'static PanelLayout>) -> ClockHandle {
    let Some(panel) = panel else {
        return ClockHandle { tx: None };
    };
    let Some(leds) = Leds::open(panel) else {
        if !panel.optional {
            warn!("clock unavailable");
        }
        return ClockHandle { tx: None };
    };
    spawn_actor(Box::new(leds), Box::new(SystemClock))
}

fn spawn_actor(panel: Box<dyn ClockPanel>, wall: Box<dyn WallClock>) -> ClockHandle {
    let mut clock = Clock {
        layers: ClockLayers::new(),
        panel,
        wall,
        level: -1,
        user_bri: 1.0,
        lux: -1.0,
        dimmer: AmbientDimmer::new(),
        auto_bri: true,
        refreshes: 0,
        timers: Timers::new(),
    };
    let tick = TICK_MS as u64;
    clock.timers.set_repeating(Timer::Tick, tick, tick);
    clock.refresh();
    let (tx, rx) = mpsc::unbounded_channel();
    pulsar_core::supervisor().spawn_local("clock", clock.run(rx));
    ClockHandle { tx: Some(tx) }
}

#[cfg(test)]
pub fn spawn_with(panel: Box<dyn ClockPanel>, wall: Box<dyn WallClock>) -> ClockHandle {
    spawn_actor(panel, wall)
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

    pub fn set_brightness(&self, b: f32) {
        self.send(Cmd::SetBrightness(b));
    }

    pub fn set_ambient_lux(&self, lux: f32) {
        self.send(Cmd::SetAmbientLux(lux));
    }

    pub fn set_auto_brightness(&self, on: bool) {
        self.send(Cmd::SetAutoBrightness(on));
    }

    pub fn set_timer(&self, ms_left: i64, running: bool) {
        self.send(Cmd::SetTimer { ms_left, running });
    }

    pub fn flash_number(&self, value: i32) {
        self.send(Cmd::FlashNumber(value));
    }

    pub fn show_code(&self, digits: String) {
        self.send(Cmd::ShowCode(digits));
    }

    pub fn clear_code(&self) {
        self.send(Cmd::ClearCode);
    }

    pub fn set_text(&self, utf8: String) {
        self.send(Cmd::SetText(utf8));
    }

    pub fn set_face(&self, face: ClockFace) {
        self.send(Cmd::SetFace(face));
    }

    pub fn set_scroll_ms(&self, ms: i32) {
        self.send(Cmd::SetScrollMs(ms));
    }

    pub fn show_temporary_text(&self, utf8: String, seconds: i32) {
        self.send(Cmd::ShowTemporaryText(utf8, seconds));
    }

    pub fn show_temporary_face(&self, face: ClockFace, seconds: i32) {
        self.send(Cmd::ShowTemporaryFace(face, seconds));
    }

    pub fn set_ui_state(&self, s: UiState) {
        self.send(Cmd::SetUiState(s));
    }

    pub fn set_voice_indication(&self, on: bool) {
        self.send(Cmd::SetVoiceIndication(on));
    }

    pub fn set_colon_blink(&self, on: bool) {
        self.send(Cmd::SetColonBlink(on));
    }

    pub fn play(&self, s: ClockShot) {
        self.send(Cmd::Play(s));
    }

    pub fn set_updating(&self, on: bool) {
        self.send(Cmd::SetUpdating(on));
    }

    pub fn set_music_style(&self, style: ClockMusicStyle) {
        self.send(Cmd::SetMusicStyle(style));
    }

    pub fn set_media_playing(&self, playing: bool) {
        self.send(Cmd::SetMediaPlaying(playing));
    }

    pub fn music_frame(&self, f: MusicFrame, from_sendspin: bool) {
        let source = if from_sendspin {
            MusicSource::Sendspin
        } else {
            MusicSource::Speaker
        };
        self.send(Cmd::MusicFrame(f, source));
    }
}

#[cfg(test)]
mod tests;
