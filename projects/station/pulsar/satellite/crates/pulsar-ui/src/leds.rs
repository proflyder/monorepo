use pulsar_core::{AmbientDimmer, MusicFrame, UiState, perceptual};

use crate::Effect;
use crate::MusicFrames;
use crate::MusicSource;
use crate::MusicStyle;
use crate::VoiceColor;
use crate::led_fx::{self as fx, Col};

pub const FADE_MS: i64 = 250;
pub const VOLUME_SHOW_MS: i64 = 1500;
const MAX_STEP_MS: i64 = 100;
const CYCLE_STYLE_MS: i64 = 30000;
const CYCLE_FADE_MS: i64 = 1000;
const MAX_SCALE: f32 = 0.8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

pub type Matrix = [[f32; 3]; 3];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LedCommand {
    UiState(UiState),
    Muted(bool),
    UserLight {
        on: bool,
        color: Rgb,
        brightness: f32,
        effect: Effect,
        transition_ms: u32,
    },
    Volume(f32),
    AmbientLux(f32),
    TimerStarted,
    ShowTemporary {
        effect: Effect,
        color: Rgb,
        seconds: i32,
    },
    ClearTemporary,
    VoiceIndication(bool),
    VoiceColor(VoiceColor),
    Speed(f32),
    MusicStyle(MusicStyle),
    MediaPlaying(bool),
    Music(MusicFrame, MusicSource),
    CaseMatrix(Option<Matrix>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    None,
    Volume,
    Voice,
    OneShot,
    Show,
    Mute,
    User,
    Music,
    Off,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sweep {
    None,
    MuteOn,
    MuteOff,
}

fn to_voice(s: UiState) -> fx::Voice {
    match s {
        UiState::Listening => fx::Voice::Listening,
        UiState::Thinking => fx::Voice::Thinking,
        UiState::Speaking => fx::Voice::Speaking,
        UiState::Error => fx::Voice::Error,
        UiState::Call => fx::Voice::Call,
        UiState::TimerRing | UiState::Idle => fx::Voice::TimerRing,
    }
}

fn to_col(c: Rgb) -> Col {
    fx::rgb(c.r, c.g, c.b)
}

pub fn to_px(c: Col, d: f32, matrix: Option<&Matrix>) -> [u8; 3] {
    let mut p = [
        fx::clamp01(c.r) * d,
        fx::clamp01(c.g) * d,
        fx::clamp01(c.b) * d,
    ];
    for v in &mut p {
        *v = perceptual(*v);
    }
    if let Some(m) = matrix {
        let mut q = [0f32; 3];
        for (r, q) in q.iter_mut().enumerate() {
            *q = m[r][2]
                .mul_add(p[2], m[r][0].mul_add(p[0], m[r][1] * p[1]))
                .max(0.0);
        }
        let mx = q[0].max(q[1]).max(q[2]);
        for (r, q) in q.iter().enumerate() {
            p[r] = if mx > 1.0 { *q / mx } else { *q };
        }
    }
    p.map(|v| (v.min(1.0) * MAX_SCALE * 255.0).round() as u8)
}

pub struct Compositor {
    n: usize,
    nlogo: usize,
    short_ring: bool,
    matrix: Option<Matrix>,

    state: UiState,
    state_t0: i64,
    voice_indication: bool,
    voice_color: VoiceColor,
    sweep: Sweep,
    sweep_t0: i64,
    timer_shot: bool,
    timer_t0: i64,
    muted: bool,
    mute_known: bool,
    show_on: bool,
    show_effect: Effect,
    show_col: Col,
    show_until: i64,
    show_fx: fx::EffectState,
    user_on: bool,
    user_col: Col,
    user_bri: f32,
    user_effect: Effect,
    user_fx: fx::EffectState,
    user_changed: bool,
    user_transition: u32,
    style: MusicStyle,
    media: bool,
    music: MusicFrames,
    source: Option<MusicSource>,
    source_changed: Option<MusicSource>,
    mus_shown: fx::MusicInput,
    mus_st: fx::MusicState,
    cycle_t0: i64,
    volume: f32,
    volume_until: i64,
    speed: f32,
    phase: f64,
    last_ms: Option<i64>,
    layer: Layer,
    shown: Vec<Col>,
    from: Vec<Col>,
    target: Vec<Col>,
    alt: Vec<Col>,
    fade_t0: i64,
    fade_ms: i64,
    dimmer: AmbientDimmer,
    px: Vec<u8>,
}

impl Compositor {
    pub fn new(n: usize, nlogo: usize) -> Self {
        let all = n + nlogo;
        Self {
            n,
            nlogo,
            short_ring: !fx::wide(n),
            matrix: None,
            state: UiState::Idle,
            state_t0: 0,
            voice_indication: true,
            voice_color: VoiceColor::Violet,
            sweep: Sweep::None,
            sweep_t0: 0,
            timer_shot: false,
            timer_t0: 0,
            muted: false,
            mute_known: false,
            show_on: false,
            show_effect: Effect::Solid,
            show_col: Col::default(),
            show_until: 0,
            show_fx: fx::EffectState::default(),
            user_on: false,
            user_col: fx::rgb(255, 255, 255),
            user_bri: 1.0,
            user_effect: Effect::Solid,
            user_fx: fx::EffectState::default(),
            user_changed: false,
            user_transition: 0,
            style: MusicStyle::Off,
            media: false,
            music: MusicFrames::default(),
            source: None,
            source_changed: None,
            mus_shown: fx::MusicInput::default(),
            mus_st: fx::MusicState::default(),
            cycle_t0: 0,
            volume: 0.0,
            volume_until: 0,
            speed: 1.0,
            phase: 0.0,
            last_ms: None,
            layer: Layer::None,
            shown: vec![Col::default(); all],
            from: vec![Col::default(); all],
            target: vec![Col::default(); all],
            alt: vec![Col::default(); n],
            fade_t0: 0,
            fade_ms: 0,
            dimmer: AmbientDimmer::new(),
            px: vec![0; all * 3],
        }
    }

    pub fn start_at(&mut self, now: i64) {
        self.state_t0 = now;
    }

    pub fn pixels(&self) -> &[u8] {
        &self.px
    }

    pub fn take_source_change(&mut self) -> Option<MusicSource> {
        self.source_changed.take()
    }

    pub fn apply(&mut self, cmd: LedCommand, now: i64) -> bool {
        match cmd {
            LedCommand::UiState(s) => {
                if s == self.state {
                    return false;
                }
                if self.state == UiState::Error
                    && s == UiState::Idle
                    && now - self.state_t0 < fx::ERROR_MS
                {
                    return false;
                }
                self.set_state(s, now);
            }
            LedCommand::Muted(muted) => {
                if self.mute_known && muted == self.muted {
                    return false;
                }
                let first = !self.mute_known;
                self.mute_known = true;
                self.muted = muted;
                if !first && (muted || !self.short_ring) {
                    self.sweep = if muted { Sweep::MuteOn } else { Sweep::MuteOff };
                    self.sweep_t0 = now;
                } else if !first && self.sweep == Sweep::MuteOn {
                    self.sweep = Sweep::None;
                }
            }
            LedCommand::UserLight {
                on,
                color,
                brightness,
                effect,
                transition_ms,
            } => {
                self.user_on = on;
                self.user_col = to_col(color);
                self.user_bri = fx::clamp01(brightness);
                self.user_effect = effect;
                self.user_changed = true;
                self.user_transition = transition_ms;
            }
            LedCommand::Volume(v) => {
                self.volume = fx::clamp01(v);
                self.volume_until = now + VOLUME_SHOW_MS;
            }
            LedCommand::AmbientLux(lux) => return self.dimmer.update(lux),
            LedCommand::TimerStarted => {
                self.timer_shot = true;
                self.timer_t0 = now;
            }
            LedCommand::ShowTemporary {
                effect,
                color,
                seconds,
            } => {
                self.show_on = true;
                self.show_effect = effect;
                self.show_col = to_col(color);
                let seconds = if seconds <= 0 { 5 } else { seconds.min(3600) };
                self.show_until = now + i64::from(seconds) * 1000;
                self.show_fx = fx::EffectState::default();
            }
            LedCommand::ClearTemporary => {
                if !self.show_on {
                    return false;
                }
                self.show_on = false;
            }
            LedCommand::VoiceIndication(on) => {
                if on == self.voice_indication {
                    return false;
                }
                self.voice_indication = on;
            }
            LedCommand::VoiceColor(color) => {
                if color == self.voice_color {
                    return false;
                }
                self.voice_color = color;
            }
            LedCommand::Speed(k) => {
                self.speed = if k.is_finite() {
                    k.clamp(0.25, 4.0)
                } else {
                    1.0
                };
                return false;
            }
            LedCommand::MusicStyle(style) => {
                if style == self.style {
                    return false;
                }
                self.style = style;
                self.cycle_t0 = now;
            }
            LedCommand::MediaPlaying(playing) => {
                if playing == self.media {
                    return false;
                }
                self.media = playing;
                if playing {
                    self.cycle_t0 = now;
                    self.mus_shown = fx::MusicInput::default();
                } else {
                    self.source = None;
                }
            }
            LedCommand::Music(f, src) => {
                if !self.music.push(f, src, now) {
                    return false;
                }
                if self.media && self.source != Some(src) {
                    self.source = Some(src);
                    self.source_changed = Some(src);
                }
                return false;
            }
            LedCommand::CaseMatrix(m) => self.matrix = m,
        }
        true
    }

    fn set_state(&mut self, s: UiState, now: i64) {
        self.state = s;
        self.state_t0 = now;
    }

    pub fn dim(&self) -> f32 {
        0.3 + 0.7 * (self.dimmer.factor() - 0.02) / 0.98
    }

    fn voice_shown(&self) -> bool {
        match self.state {
            UiState::Idle => false,
            UiState::Error | UiState::TimerRing | UiState::Call => true,
            _ => self.voice_indication,
        }
    }

    fn music_shown(&self) -> bool {
        self.media && self.style != MusicStyle::Off
    }

    fn update_music(&mut self, now: i64, dt_s: f64, beat: bool) {
        let fresh = self.music.fresh(now);
        let frame = self.music.frame();
        let rel = (-dt_s / 0.25).exp() as f32;
        let shown = &mut self.mus_shown;
        for (s, v) in shown.bands.iter_mut().zip(frame.bands) {
            *s = (if fresh { v } else { 0.0 }).max(*s * rel);
        }
        shown.loudness = (if fresh { frame.loudness } else { 0.0 }).max(shown.loudness * rel);
        shown.flash *= (-dt_s / 0.15).exp() as f32;
        if beat {
            shown.flash = 1.0;
        }
    }

    fn render_style(&self, s: fx::Style, out: &mut [Col]) {
        fx::music(s, &self.mus_st, &self.mus_shown, self.phase, out);
    }

    fn render_music(&mut self, now: i64, out: &mut [Col]) {
        if let Some(style) = self.style.style() {
            return self.render_style(style, out);
        }
        let t = now - self.cycle_t0;
        let i = (t / CYCLE_STYLE_MS % 4) as usize;
        self.render_style(fx::Style::ALL[i], out);
        let into = t % CYCLE_STYLE_MS;
        if into < CYCLE_STYLE_MS - CYCLE_FADE_MS {
            return;
        }
        let mut alt = std::mem::take(&mut self.alt);
        self.render_style(fx::Style::ALL[(i + 1) % 4], &mut alt);
        let f = fx::smoothstep(
            0.0,
            1.0,
            (into - (CYCLE_STYLE_MS - CYCLE_FADE_MS)) as f32 / CYCLE_FADE_MS as f32,
        );
        for (o, a) in out.iter_mut().zip(&alt) {
            *o = fx::lerp(*o, *a, f);
        }
        self.alt = alt;
    }

    fn render(&mut self, now: i64, dt: f64, lay: Layer, target: &mut [Col]) -> bool {
        target.fill(Col::default());
        let (ring, logo) = target.split_at_mut(self.n);
        let mut animated = true;
        match lay {
            Layer::Volume => {
                fx::volume(self.volume, ring);
                animated = false;
            }
            Layer::Voice => fx::voice(
                to_voice(self.state),
                now - self.state_t0,
                self.voice_color,
                ring,
            ),
            Layer::OneShot => {
                if self.sweep == Sweep::None {
                    fx::timer_started(now - self.timer_t0, ring);
                } else {
                    fx::mute_sweep(self.sweep == Sweep::MuteOn, now - self.sweep_t0, ring);
                }
            }
            Layer::Show => {
                animated = fx::effect(
                    self.show_effect,
                    self.show_col,
                    1.0,
                    self.phase,
                    dt,
                    &mut self.show_fx,
                    ring,
                );
            }
            Layer::Mute => {
                ring.fill(fx::mute_steady());
                animated = false;
            }
            Layer::User => {
                animated = fx::effect(
                    self.user_effect,
                    self.user_col,
                    self.user_bri,
                    self.phase,
                    dt,
                    &mut self.user_fx,
                    ring,
                );
            }
            Layer::Music => self.render_music(now, ring),
            Layer::Off | Layer::None => animated = false,
        }
        if self.nlogo > 0 {
            if self.voice_shown() {
                fx::voice_logo(
                    to_voice(self.state),
                    now - self.state_t0,
                    self.voice_color,
                    logo,
                );
                animated = true;
            } else if self.muted {
                logo.fill(fx::mute_logo());
            } else {
                let k = 1.0 / self.n as f32;
                let avg = ring.iter().fold(Col::default(), |a, c| a + *c * k);
                logo.fill(avg);
            }
        }
        animated
    }

    fn active_layer(&self, now: i64) -> Layer {
        if now < self.volume_until {
            Layer::Volume
        } else if self.voice_shown() {
            Layer::Voice
        } else if self.sweep != Sweep::None || self.timer_shot {
            Layer::OneShot
        } else if self.show_on {
            Layer::Show
        } else if self.short_ring && self.muted {
            Layer::Mute
        } else if self.user_on && self.user_bri > 0.0 {
            Layer::User
        } else if self.music_shown() {
            Layer::Music
        } else {
            Layer::Off
        }
    }

    fn expire(&mut self, now: i64) {
        if self.state == UiState::Error && now - self.state_t0 >= fx::ERROR_MS {
            self.set_state(UiState::Idle, now);
        }
        if self.sweep != Sweep::None && now - self.sweep_t0 >= fx::MUTE_SWEEP_MS {
            self.sweep = Sweep::None;
        }
        if self.timer_shot && now - self.timer_t0 >= fx::TIMER_STARTED_MS {
            self.timer_shot = false;
        }
        if self.show_on && now >= self.show_until {
            self.show_on = false;
        }
    }

    pub fn next_expiry(&self, now: i64) -> Option<i64> {
        let mut t: Option<i64> = None;
        let mut take = |e: i64| {
            if e > now && t.is_none_or(|t| e < t) {
                t = Some(e);
            }
        };
        take(self.volume_until);
        if self.state == UiState::Error {
            take(self.state_t0 + fx::ERROR_MS);
        }
        if self.sweep != Sweep::None {
            take(self.sweep_t0 + fx::MUTE_SWEEP_MS);
        }
        if self.timer_shot {
            take(self.timer_t0 + fx::TIMER_STARTED_MS);
        }
        if self.show_on {
            take(self.show_until);
        }
        t
    }

    pub fn frame(&mut self, now: i64) -> bool {
        let step = self.last_ms.map_or(0, |l| (now - l).clamp(0, MAX_STEP_MS));
        self.last_ms = Some(now);
        let dt = step as f64 / 1000.0 * f64::from(self.speed);
        self.phase += dt;
        self.expire(now);

        let lay = self.active_layer(now);
        let beat = self.music.take_beat();
        if lay == Layer::Music {
            self.update_music(now, step as f64 / 1000.0, beat);
            fx::music_step(&mut self.mus_st, &self.mus_shown, beat, self.phase, dt);
        }
        let mut target = std::mem::take(&mut self.target);
        let mut animated = self.render(now, dt, lay, &mut target);
        self.target = target;

        let fade = if self.user_changed
            && (lay == Layer::User || (self.layer == Layer::User && lay != self.layer))
        {
            Some(i64::from(self.user_transition))
        } else {
            (lay != self.layer).then(|| if lay == Layer::Volume { 0 } else { FADE_MS })
        };
        self.user_changed = false;
        self.layer = lay;
        if let Some(f) = fade {
            self.from.copy_from_slice(&self.shown);
            self.fade_t0 = now;
            self.fade_ms = f;
        }
        if self.fade_ms > 0 && now - self.fade_t0 < self.fade_ms {
            let f = fx::smoothstep(0.0, 1.0, (now - self.fade_t0) as f32 / self.fade_ms as f32);
            for ((s, a), b) in self.shown.iter_mut().zip(&self.from).zip(&self.target) {
                *s = fx::lerp(*a, *b, f);
            }
            animated = true;
        } else {
            self.fade_ms = 0;
            self.shown.copy_from_slice(&self.target);
        }

        let d = self.dim();
        for (c, px) in self.shown.iter().zip(self.px.as_chunks_mut::<3>().0) {
            *px = to_px(*c, d, self.matrix.as_ref());
        }
        animated
    }
}

#[cfg(test)]
mod tests;
