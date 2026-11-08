//! Слои контента часов и их приоритет: код сопряжения > громкость > голос > обновление >
//! one-shot > таймер > временный слой > текст/лицо > уровни музыки > время. Чистая часть
//! `pulsar-clock/src/clock.rs`: без tokio и sysfs.
//!
//! Content, first active wins: pairing code (even when disabled) > volume number > voice state
//! (TIMER_RING always, LISTENING / THINKING / SPEAKING with voice indication on) > update install
//! (dark) > one-shot > timer countdown > temporary layer > Text / Face > music levels > time.
//! Disabled: dark.

use pulsar_core::{ClockShot, MusicFrame, UiState};

use crate::ClockFace;
use crate::ClockMusicStyle;
use crate::MusicFrames;
use crate::MusicSource;
use crate::clock_fx::{self, Anim, Frame, clamp};
use crate::font::{self, Cell, DIGITS, Glyphs};

/// Local wall-clock time for the display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wall {
    pub hour: i32,
    pub min: i32,
    /// Within the first half of the current second.
    pub first_half: bool,
}

const FLASH_MS: i64 = 1500;
const CODE_PAGE_MS: i64 = 2000;
const TEMPORARY_MS: i64 = 5000;
const TEMPORARY_MAX_S: i64 = 3600;
/// Animation frame step (the stock patterns use 20 ms frames).
pub const FX_FRAME_MS: i64 = 20;
/// Level meter redraw while shown.
const MUSIC_FRAME_MS: i64 = 40;
/// Meters fall with this time constant.
const MUSIC_RELEASE_MS: f32 = 250.0;
/// Each digit's meter spans the recent range of its two bands: the range follows peaks and dips
/// at once and closes in on the current value with this time constant. A mixed song keeps a
/// nearly constant spectral tilt and moves by only a few dB, which on a fixed scale left every
/// song looking alike (measured on a Midi: +-0.15 of 3 segments per digit).
const RANGE_TAU_MS: f32 = 3000.0;
/// Narrowest range, in band units: ~7 dB on the speaker reference (45 dB scale), ~9 dB on
/// Sendspin (60 dB scale); keeps steady passages from blowing noise up to full height.
const MIN_RANGE: f32 = 0.15;
/// A source's ranges start over after this long without it.
const RANGE_KEEP_MS: i64 = 10_000;
/// Decay step across a gap (frames come every ~33 ms).
const RANGE_MAX_STEP_MS: i64 = 100;
/// While the meters are shown, the time takes over for the last `VIZ_TIME_MS` of every cycle.
const VIZ_CYCLE_MS: i64 = 30_000;
const VIZ_TIME_MS: i64 = 3000;

fn shot_anim(s: ClockShot) -> Anim {
    match s {
        ClockShot::Boot => Anim::Boot,
        ClockShot::Pause => Anim::Pause,
        ClockShot::Next => Anim::Next,
        ClockShot::Previous => Anim::Previous,
    }
}

pub fn glyph_frame(g: Glyphs, colon: bool) -> Frame {
    let mut f = Frame::default();
    for (seg, glyph) in f.seg.iter_mut().zip(g) {
        for (s, v) in seg.iter_mut().enumerate() {
            *v = if glyph >> s & 1 != 0 { 255 } else { 0 };
        }
    }
    f.colon = if colon { 255 } else { 0 };
    f
}

#[derive(Clone, Debug, PartialEq)]
enum Content {
    Text { cells: Vec<Cell>, since: i64 },
    Face { face: i32, since: i64 },
}

impl Content {
    fn since(&self) -> i64 {
        match self {
            Self::Text { since, .. } | Self::Face { since, .. } => *since,
        }
    }

    /// Frame step while it moves; 0 = static.
    fn step(&self, scroll_ms: i64) -> i64 {
        match self {
            Self::Text { cells, .. } if cells.len() > DIGITS => scroll_ms,
            Self::Face { face, .. } if *face == font::FACE_BLINK => font::BLINK_STEP_MS,
            Self::Face { face, .. } if *face == font::FACE_ALL => font::FACE_CYCLE_MS,
            _ => 0,
        }
    }

    fn glyphs(&self, now: i64, scroll_ms: i64) -> (Glyphs, bool) {
        let t = now - self.since();
        match self {
            Self::Text { cells, .. } => font::text_glyphs(cells, t, scroll_ms),
            Self::Face { face, .. } => (font::face_glyphs(*face, t), false),
        }
    }
}

/// Recent band range of one music source, per digit.
#[derive(Clone, Copy, Debug)]
struct Range {
    lo: [f32; DIGITS],
    hi: [f32; DIGITS],
    /// Last frame of this source.
    last_ms: i64,
}

impl Default for Range {
    fn default() -> Self {
        Self {
            lo: [0.0; DIGITS],
            hi: [0.0; DIGITS],
            last_ms: i64::MIN / 2,
        }
    }
}

/// Everything the clock shows, without the output: which layer wins and its frame at a time.
#[derive(Debug)]
pub struct ClockLayers {
    enabled: bool,
    /// Monotonic ms of a running countdown; 0 = none.
    timer_end: i64,
    /// ms left of a paused countdown; 0 = none.
    timer_paused: i64,
    flash_until: i64,
    flash_value: i32,
    /// Pairing code shown instead of anything else (empty = none).
    code: String,
    code_since: i64,
    /// Text / Select entities.
    base: Option<Content>,
    /// Temporary layer (actions).
    overlay: Option<Content>,
    overlay_until: i64,
    scroll_ms: i64,
    ui: UiState,
    voice_indication: bool,
    /// Voice state / ringing.
    voice: Option<Anim>,
    voice_t0: i64,
    /// One-shot (boot, player).
    shot: Option<Anim>,
    shot_t0: i64,
    updating: bool,
    levels_on: bool,
    media: bool,
    viz: bool,
    /// Per digit, 0..1 within its range.
    mus_in: [f32; DIGITS],
    mus_shown: [f32; DIGITS],
    /// Speaker reference, Sendspin.
    ranges: [Range; 2],
    music: MusicFrames,
    mus_drawn_ms: i64,
    viz_t0: i64,
}

impl Default for ClockLayers {
    fn default() -> Self {
        Self::new()
    }
}

impl ClockLayers {
    pub fn new() -> Self {
        Self {
            enabled: true,
            timer_end: 0,
            timer_paused: 0,
            flash_until: 0,
            flash_value: 0,
            code: String::new(),
            code_since: 0,
            base: None,
            overlay: None,
            overlay_until: 0,
            scroll_ms: 400,
            ui: UiState::Idle,
            voice_indication: true,
            voice: None,
            voice_t0: 0,
            shot: None,
            shot_t0: 0,
            updating: false,
            levels_on: false,
            media: false,
            viz: false,
            mus_in: [0.0; DIGITS],
            mus_shown: [0.0; DIGITS],
            ranges: [Range::default(); 2],
            music: MusicFrames::default(),
            mus_drawn_ms: 0,
            viz_t0: 0,
        }
    }

    pub fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
    }

    pub fn set_timer(&mut self, ms_left: i64, running: bool, now: i64) {
        self.timer_end = if ms_left > 0 && running {
            now + ms_left
        } else {
            0
        };
        self.timer_paused = if ms_left > 0 && !running { ms_left } else { 0 };
    }

    pub fn flash_number(&mut self, value: i32, now: i64) {
        self.flash_value = value.clamp(-999, 9999);
        self.flash_until = now + FLASH_MS;
    }

    pub fn show_code(&mut self, digits: String, now: i64) {
        self.code = digits;
        self.code_since = now;
    }

    pub fn clear_code(&mut self) {
        self.code.clear();
    }

    pub fn set_text(&mut self, utf8: &str, now: i64) {
        let cells = font::layout_text(utf8.as_bytes());
        if cells.is_empty() {
            // Non-empty text without cells (only ':') still replaces the face, as the
            // controller does.
            if matches!(self.base, Some(Content::Text { .. })) || !utf8.is_empty() {
                self.base = None;
            }
        } else {
            self.base = Some(Content::Text { cells, since: now });
        }
    }

    pub fn set_face(&mut self, face: ClockFace, now: i64) {
        if let Some(index) = face.index() {
            self.base = Some(Content::Face {
                face: index,
                since: now,
            });
        } else if matches!(self.base, Some(Content::Face { .. })) {
            self.base = None;
        }
    }

    pub fn set_scroll_ms(&mut self, ms: i32) {
        self.scroll_ms = i64::from(ms.clamp(100, 2000));
    }

    pub fn show_temporary_text(&mut self, utf8: &str, seconds: i32, now: i64) {
        let cells = font::layout_text(utf8.as_bytes());
        if cells.is_empty() {
            self.overlay = None;
            return;
        }
        let n = cells.len() as i64;
        let digits = DIGITS as i64;
        self.overlay_until = now
            + if seconds > 0 {
                i64::from(seconds).min(TEMPORARY_MAX_S) * 1000
            } else if n > digits {
                (n + digits) * self.scroll_ms
            } else {
                TEMPORARY_MS
            };
        self.overlay = Some(Content::Text { cells, since: now });
    }

    pub fn show_temporary_face(&mut self, face: ClockFace, seconds: i32, now: i64) {
        let Some(index) = face.index() else {
            self.overlay = None;
            return;
        };
        self.overlay_until = now
            + if seconds > 0 {
                i64::from(seconds).min(TEMPORARY_MAX_S) * 1000
            } else {
                TEMPORARY_MS
            };
        self.overlay = Some(Content::Face {
            face: index,
            since: now,
        });
    }

    fn update_voice(&mut self, now: i64) {
        let a = match self.ui {
            UiState::Listening if self.voice_indication => Some(Anim::Listening),
            UiState::Thinking if self.voice_indication => Some(Anim::Thinking),
            UiState::Speaking if self.voice_indication => Some(Anim::Speaking),
            UiState::TimerRing | UiState::Call => Some(Anim::Ringing),
            _ => None,
        };
        if a == self.voice {
            return;
        }
        self.voice = a;
        self.voice_t0 = now;
    }

    pub fn set_ui_state(&mut self, s: UiState, now: i64) {
        self.ui = s;
        self.update_voice(now);
    }

    pub fn set_voice_indication(&mut self, on: bool, now: i64) {
        self.voice_indication = on;
        self.update_voice(now);
    }

    pub fn play(&mut self, s: ClockShot, now: i64) {
        self.shot = Some(shot_anim(s));
        self.shot_t0 = now;
    }

    pub fn set_updating(&mut self, on: bool) {
        self.updating = on;
    }

    pub fn set_music_style(&mut self, style: ClockMusicStyle) {
        self.levels_on = style == ClockMusicStyle::Levels;
    }

    pub fn set_media_playing(&mut self, on: bool) {
        self.media = on;
    }

    pub fn music_frame(&mut self, f: &MusicFrame, source: MusicSource, now: i64) {
        // Frames only feed the meters; the frame timer draws them.
        if !self.music.push(*f, source, now) {
            return;
        }
        // The sources use different scales, so each keeps its own ranges. Music Assistant
        // restarts the Sendspin stream on every seek / skip (~0.7 s without frames; the speaker
        // reference fills in meanwhile): a range survives such an interlude frozen, and starts
        // over only after RANGE_KEEP_MS without frames of its source.
        let r = &mut self.ranges[usize::from(source == MusicSource::Sendspin)];
        let gap = now - r.last_ms;
        let restart = gap >= RANGE_KEEP_MS;
        let k = if restart {
            0.0
        } else {
            (-(gap.min(RANGE_MAX_STEP_MS) as f32) / RANGE_TAU_MS).exp()
        };
        for i in 0..DIGITS {
            let x = (f.bands[2 * i] + f.bands[2 * i + 1]) / 2.0;
            let (lo, hi) = (&mut r.lo[i], &mut r.hi[i]);
            if restart {
                *lo = x;
                *hi = x;
            } else {
                // Fused like the clang-built reference (see clock_fx.rs).
                *hi = if x > *hi { x } else { (*hi - x).mul_add(k, x) };
                *lo = if x < *lo { x } else { (*lo - x).mul_add(k, x) };
            }
            // A range narrower than MIN_RANGE is widened around its centre but kept inside
            // 0..1, so silence (bands at 0) stays dark instead of showing the middle of the
            // meter.
            let span = (*hi - *lo).max(MIN_RANGE);
            let base = clamp((*hi + *lo - span) / 2.0, 0.0, 1.0 - span);
            self.mus_in[i] = clamp((x - base) / span, 0.0, 1.0);
        }
        r.last_ms = now;
    }

    /// The frame to show at `now` and the ms to its next change before the tick (0 = none).
    /// `wall` is only asked when the time is shown.
    pub fn compose(&mut self, now: i64, wall: impl FnOnce() -> Wall) -> (Frame, i64) {
        if self.overlay.is_some() && now >= self.overlay_until {
            self.overlay = None;
        }
        if let Some(s) = self.shot
            && now - self.shot_t0 >= clock_fx::duration_ms(s)
        {
            self.shot = None;
        }
        let eligible = self.levels_on && self.media && self.music.fresh(now);
        if eligible && !self.viz {
            self.viz_t0 = now;
            self.mus_drawn_ms = now;
            self.mus_shown = [0.0; DIGITS];
        }
        self.viz = eligible;
        if !self.code.is_empty() {
            let g = font::code_glyphs(self.code.as_bytes(), now - self.code_since, CODE_PAGE_MS);
            return (glyph_frame(g, false), 0);
        }
        if !self.enabled {
            return (Frame::default(), 0);
        }
        let frame_step = |t0: i64| FX_FRAME_MS - (now - t0) % FX_FRAME_MS;
        if now < self.flash_until {
            let g = font::number_glyphs(self.flash_value);
            (glyph_frame(g, false), self.flash_until - now)
        } else if let Some(v) = self.voice {
            (
                clock_fx::render(v, now - self.voice_t0),
                frame_step(self.voice_t0),
            )
        } else if self.updating {
            // Dark until the new binary takes over.
            (Frame::default(), 0)
        } else if let Some(s) = self.shot {
            let end = self.shot_t0 + clock_fx::duration_ms(s);
            let next = frame_step(self.shot_t0).min(end - now).max(1);
            (clock_fx::render(s, now - self.shot_t0), next)
        } else if self.timer_end != 0 || self.timer_paused != 0 {
            let left_ms = if self.timer_end != 0 {
                self.timer_end - now
            } else {
                self.timer_paused
            };
            let left = ((left_ms + 999) / 1000).max(0);
            let mut g = font::timer_glyphs(left);
            // Blinks at 00:00.
            let blink_off = (now / 500) % 2 != 0;
            let colon = left > 0 || !blink_off;
            if left == 0 && blink_off {
                g = [0; DIGITS];
            }
            (glyph_frame(g, colon), 0)
        } else if let Some((c, is_overlay)) = self
            .overlay
            .as_ref()
            .map(|c| (c, true))
            .or(self.base.as_ref().map(|c| (c, false)))
        {
            let (g, colon) = c.glyphs(now, self.scroll_ms);
            let step = c.step(self.scroll_ms);
            let mut next = if step != 0 {
                step - (now - c.since()) % step
            } else {
                0
            };
            // Also fires when the temporary layer ends, so a single scroll pass does not show
            // the start of a second one until the next tick.
            if is_overlay {
                let left = self.overlay_until - now;
                next = if step != 0 { next.min(left) } else { left }.max(1);
            }
            (glyph_frame(g, colon), next)
        } else if self.viz && (now - self.viz_t0) % VIZ_CYCLE_MS < VIZ_CYCLE_MS - VIZ_TIME_MS {
            let k = (-((now - self.mus_drawn_ms) as f32) / MUSIC_RELEASE_MS).exp();
            for (shown, &input) in self.mus_shown.iter_mut().zip(&self.mus_in) {
                // std::max(in, shown * k): NaN in either keeps `in`, as C++ does.
                let decayed = *shown * k;
                *shown = if input < decayed { decayed } else { input };
            }
            self.mus_drawn_ms = now;
            (clock_fx::levels(&self.mus_shown), MUSIC_FRAME_MS)
        } else {
            let w = wall();
            // Colon on for the first half of every second.
            (
                glyph_frame(font::time_glyphs(w.hour, w.min), w.first_half),
                0,
            )
        }
    }
}

#[cfg(test)]
mod tests;
