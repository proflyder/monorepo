use std::fs::File;
use std::io;
use std::os::fd::AsRawFd as _;

use home_linux::{Framebuffer, I2c};
use pulsar_core::{MusicFrame, Timers, UiState};
use pulsar_model::{LedBackend, Leds, StripLayout};
use pulsar_ui::{
    CaseColor, Compositor, Effect, LedCommand, Matrix, MusicSource, MusicStyle, Rgb, VoiceColor,
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::time::Instant;
use tracing::{error, info, warn};

const FRAME_MS: u64 = 33;
const RETRY_MS: u64 = 1000;
const WARN_EVERY_MS: i64 = 10000;
const LED_DIR: &str = "/sys/class/leds/";
const CASE_DIR: &str = "/system/vendor/quasar/ledconfigs/";

pub fn effect_names() -> [&'static str; Effect::ALL.len()] {
    Effect::ALL.map(Effect::as_str)
}

pub fn music_styles() -> [&'static str; MusicStyle::ALL.len()] {
    MusicStyle::ALL.map(MusicStyle::as_str)
}

pub fn case_colors() -> Vec<String> {
    case_colors_in(CASE_DIR)
}

fn case_colors_in(dir: &str) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            (name.len() > 5 && name.ends_with(".json")).then(|| name[..name.len() - 5].to_owned())
        })
        .collect();
    out.sort();
    out
}

fn load_matrix(dir: &str, stem: &str) -> Option<Matrix> {
    parse_matrix(&std::fs::read_to_string(format!("{dir}{stem}.json")).ok()?)
}

fn parse_matrix(text: &str) -> Option<Matrix> {
    parse_matrix_json(text).or_else(|| parse_first_numbers(text))
}

fn parse_matrix_json(text: &str) -> Option<Matrix> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let rows = value.get("matrix")?.as_array()?;
    if rows.len() != 3 {
        return None;
    }
    let mut m = [[0f32; 3]; 3];
    for (r, row) in rows.iter().enumerate() {
        let row = row.as_array()?;
        if row.len() != 3 {
            return None;
        }
        for (c, v) in row.iter().enumerate() {
            let v = v.as_f64()?;
            if !v.is_finite() {
                return None;
            }
            m[r][c] = v as f32;
        }
    }
    Some(m)
}

fn parse_first_numbers(text: &str) -> Option<Matrix> {
    const KEY: &str = "\"matrix\"";
    let mut pos = 0;
    let open = loop {
        let found = pos + text[pos..].find(KEY)?;
        let after = found + KEY.len();
        let colon = text[after..]
            .find(|c: char| !c.is_ascii_whitespace())
            .map(|i| after + i);
        if let Some(c) = colon
            && text.as_bytes()[c] == b':'
        {
            break c + text[c..].find('[')?;
        }
        pos = after;
    };
    let bytes = text.as_bytes();
    let mut m = [0f32; 9];
    let mut got = 0;
    let mut p = open;
    while p < bytes.len() && bytes[p] != b'}' && got < 9 {
        if bytes[p].is_ascii_digit() || bytes[p] == b'-' || bytes[p] == b'.' || bytes[p] == b'+' {
            let len = bytes[p..]
                .iter()
                .position(|b| {
                    !(b.is_ascii_digit() || matches!(b, b'-' | b'+' | b'.' | b'e' | b'E'))
                })
                .unwrap_or(bytes.len() - p);
            let v: f64 = text[p..p + len].parse().ok()?;
            if !v.is_finite() {
                return None;
            }
            m[got] = v as f32;
            got += 1;
            p += len;
        } else {
            p += 1;
        }
    }
    if got < 9 {
        return None;
    }
    Some([[m[0], m[1], m[2]], [m[3], m[4], m[5]], [m[6], m[7], m[8]]])
}

enum Msg {
    Ui(LedCommand),
    CaseColor(CaseColor),
}

#[derive(Clone)]
pub struct LedsHandle {
    tx: UnboundedSender<Msg>,
}

impl LedsHandle {
    fn send(&self, cmd: LedCommand) {
        let _ = self.tx.send(Msg::Ui(cmd));
    }

    pub fn set_ui_state(&self, s: UiState) {
        self.send(LedCommand::UiState(s));
    }

    pub fn set_muted(&self, muted: bool) {
        self.send(LedCommand::Muted(muted));
    }

    pub fn set_user_light(
        &self,
        on: bool,
        color: Rgb,
        brightness: f32,
        effect: Effect,
        transition_ms: u32,
    ) {
        self.send(LedCommand::UserLight {
            on,
            color,
            brightness,
            effect,
            transition_ms,
        });
    }

    pub fn show_volume(&self, v: f32) {
        self.send(LedCommand::Volume(v));
    }

    pub fn set_ambient_lux(&self, lux: f32) {
        self.send(LedCommand::AmbientLux(lux));
    }

    pub fn show_timer_started(&self) {
        self.send(LedCommand::TimerStarted);
    }

    pub fn show_temporary(&self, effect: Effect, color: Rgb, seconds: i32) {
        self.send(LedCommand::ShowTemporary {
            effect,
            color,
            seconds,
        });
    }

    pub fn clear_temporary(&self) {
        self.send(LedCommand::ClearTemporary);
    }

    pub fn set_voice_indication(&self, on: bool) {
        self.send(LedCommand::VoiceIndication(on));
    }

    pub fn set_voice_color(&self, color: VoiceColor) {
        self.send(LedCommand::VoiceColor(color));
    }

    pub fn set_speed(&self, k: f32) {
        self.send(LedCommand::Speed(k));
    }

    pub fn set_music_style(&self, style: MusicStyle) {
        self.send(LedCommand::MusicStyle(style));
    }

    pub fn set_media_playing(&self, playing: bool) {
        self.send(LedCommand::MediaPlaying(playing));
    }

    pub fn music_frame(&self, f: MusicFrame, from_sendspin: bool) {
        let source = if from_sendspin {
            MusicSource::Sendspin
        } else {
            MusicSource::Speaker
        };
        self.send(LedCommand::Music(f, source));
    }

    pub fn set_case_color(&self, color: CaseColor) {
        let _ = self.tx.send(Msg::CaseColor(color));
    }
}

pub fn spawn_leds(leds: &'static Leds) -> LedsHandle {
    let n = usize::try_from(leds.ring.count).unwrap_or(0);
    let nlogo = leds.logo.as_ref().map_or(0, |l| l.count as usize);
    let (sink, opened) = open_sink(leds, n, nlogo);
    let (tx, rx) = unbounded_channel();
    let comp = Compositor::new(n, nlogo);
    let out = Output::new(sink, n, nlogo);
    pulsar_core::supervisor().spawn_local("leds", run(comp, out, rx, opened));
    LedsHandle { tx }
}

fn open_sink(leds: &Leds, n: usize, nlogo: usize) -> (Box<dyn LedSink>, bool) {
    fn sysfs(leds: &Leds, n: usize, nlogo: usize) -> (Box<dyn LedSink>, bool) {
        open_sysfs(leds, n, nlogo).map_or_else(
            || {
                warn!("LEDs unavailable");
                (Box::new(NullSink) as Box<dyn LedSink>, false)
            },
            |files| {
                info!("leds: driving {n} ring + {nlogo} logo LEDs via sysfs brightness");
                (Box::new(files), true)
            },
        )
    }
    match leds.ring.backend {
        LedBackend::I2c { dev, addr } => open_i2c(dev, addr, n, nlogo).map_or_else(
            || (Box::new(NullSink) as Box<dyn LedSink>, false),
            |sink| {
                info!("leds: driving {n} ring LEDs via i2c {dev}@0x{addr:02x}");
                (Box::new(sink), true)
            },
        ),
        LedBackend::Framebuffer { .. } => open_fb(leds, n, nlogo).map_or_else(
            || sysfs(leds, n, nlogo),
            |fb| {
                info!("leds: driving {n} ring + {nlogo} logo LEDs via framebuffer");
                (Box::new(fb), true)
            },
        ),
        LedBackend::Sysfs => sysfs(leds, n, nlogo),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TimerKey {
    Frame,
    Expiry,
    Retry,
}

async fn run(mut comp: Compositor, mut out: Output, mut rx: UnboundedReceiver<Msg>, opened: bool) {
    let epoch = Instant::now();
    let now = || i64::try_from(epoch.elapsed().as_millis()).unwrap_or(i64::MAX);
    let mut timers = Timers::new();
    if opened {
        comp.start_at(now());
        frame(&mut comp, &mut out, &mut timers, now());
    }
    loop {
        tokio::select! {
            biased;
            _ = timers.next() => frame(&mut comp, &mut out, &mut timers, now()),
            msg = rx.recv() => {
                let Some(msg) = msg else { break };
                let t = now();
                let (repaint, rewrite) = match msg {
                    Msg::Ui(cmd) => {
                        let rewrite = matches!(cmd, LedCommand::CaseMatrix(_));
                        (comp.apply(cmd, t), rewrite)
                    }
                    Msg::CaseColor(color) => {
                        let stem = color.as_str();
                        let matrix = if stem.is_empty() {
                            None
                        } else if let Some(m) = load_matrix(CASE_DIR, stem) {
                            info!("leds: case colour {stem}");
                            Some(m)
                        } else {
                            warn!("leds: case colour {stem} unreadable");
                            None
                        };
                        (comp.apply(LedCommand::CaseMatrix(matrix), t), true)
                    }
                };
                if let Some(source) = comp.take_source_change() {
                    info!("leds: music source {}", source.as_str());
                }
                if repaint {
                    if rewrite {
                        out.forget_last();
                    }
                    frame(&mut comp, &mut out, &mut timers, t);
                }
            }
        }
    }
    out.clear(now());
}

fn frame(comp: &mut Compositor, out: &mut Output, timers: &mut Timers<TimerKey>, now: i64) {
    let animated = comp.frame(now);
    let failed = out.write(comp.pixels(), now);
    if animated {
        timers.cancel(TimerKey::Expiry);
        timers.cancel(TimerKey::Retry);
        if !timers.is_set(TimerKey::Frame) {
            timers.set_repeating(TimerKey::Frame, FRAME_MS, FRAME_MS);
        }
        return;
    }
    timers.cancel(TimerKey::Frame);
    timers.cancel(TimerKey::Expiry);
    if failed {
        timers.set(TimerKey::Retry, RETRY_MS);
    } else {
        timers.cancel(TimerKey::Retry);
    }
    if let Some(e) = comp.next_expiry(now) {
        timers.set(TimerKey::Expiry, u64::try_from(e - now).unwrap_or(0));
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LedError {
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("short framebuffer write: {0} of {1} bytes")]
    ShortWrite(usize, usize),
}

pub trait LedSink {
    fn write(&mut self, px: &[Rgb]) -> Result<(), LedError>;

    fn invalidate(&mut self) {}
}

struct NullSink;

impl LedSink for NullSink {
    fn write(&mut self, _: &[Rgb]) -> Result<(), LedError> {
        Ok(())
    }
}

#[derive(Default)]
struct WriteWarn {
    last: Option<i64>,
    suppressed: u32,
}

fn log_write(w: &mut WriteWarn, what: &str, err: &LedError, now: i64) {
    if let Some(t) = w.last
        && now - t < WARN_EVERY_MS
    {
        w.suppressed += 1;
        return;
    }
    warn!(
        "leds: {what} write: {err} ({} similar suppressed)",
        w.suppressed
    );
    w.last = Some(now);
    w.suppressed = 0;
}

struct FrameBufferSink {
    ring: Framebuffer,
    logo: Option<Framebuffer>,
    n: usize,
    last: Vec<Rgb>,
    has_last: bool,
    bgr: Vec<u8>,
    warn: [WriteWarn; 2],
    epoch: Instant,
}

impl FrameBufferSink {
    fn now(&self) -> i64 {
        i64::try_from(self.epoch.elapsed().as_millis()).unwrap_or(i64::MAX)
    }

    fn write_strip(
        fb: &mut Framebuffer,
        px: &[Rgb],
        bgr: &mut Vec<u8>,
        w: &mut WriteWarn,
        what: &str,
        now: i64,
    ) -> bool {
        bgr.clear();
        for p in px {
            bgr.extend_from_slice(&[p.b, p.g, p.r]);
        }
        let r = match fb.write(bgr) {
            Ok(k) if k == bgr.len() => Ok(()),
            Ok(k) => Err(LedError::ShortWrite(k, bgr.len())),
            Err(e) => Err(LedError::Io(e)),
        };
        if let Err(e) = &r {
            log_write(w, what, e, now);
        }
        fb.pan();
        r.is_ok()
    }
}

impl LedSink for FrameBufferSink {
    fn write(&mut self, px: &[Rgb]) -> Result<(), LedError> {
        let now = self.now();
        let split = self.n;
        let mut ok = true;
        if !self.has_last || px[..split] != self.last[..split] {
            ok &= Self::write_strip(
                &mut self.ring,
                &px[..split],
                &mut self.bgr,
                &mut self.warn[0],
                "ring fb",
                now,
            );
        }
        if let Some(logo) = &mut self.logo
            && (!self.has_last || px[split..] != self.last[split..])
        {
            ok &= Self::write_strip(
                logo,
                &px[split..],
                &mut self.bgr,
                &mut self.warn[1],
                "logo fb",
                now,
            );
        }
        if !ok {
            return Err(LedError::Io(io::Error::from(io::ErrorKind::Other)));
        }
        self.last.clear();
        self.last.extend_from_slice(px);
        self.has_last = true;
        Ok(())
    }

    fn invalidate(&mut self) {
        self.has_last = false;
    }
}

struct I2cSink {
    dev: I2c,
    addr: u16,
    n: usize,
    last: Vec<Rgb>,
    has_last: bool,
    warn: WriteWarn,
    epoch: Instant,
}

impl LedSink for I2cSink {
    fn write(&mut self, px: &[Rgb]) -> Result<(), LedError> {
        let ring = &px[..self.n];
        if self.has_last && ring == &self.last[..] {
            return Ok(());
        }
        let mut bgr = [0u8; 18];
        for (i, p) in ring.iter().enumerate() {
            bgr[i * 3..i * 3 + 3].copy_from_slice(&[p.b, p.g, p.r]);
        }
        let r = self
            .dev
            .write_register(self.addr, 0x01, &bgr)
            .and_then(|()| self.dev.write_register(self.addr, 0x25, &[0xaa]));
        if let Err(e) = r {
            let now = i64::try_from(self.epoch.elapsed().as_millis()).unwrap_or(i64::MAX);
            log_write(&mut self.warn, "i2c", &LedError::Io(e), now);
            return Err(LedError::Io(io::Error::from(io::ErrorKind::Other)));
        }
        self.last.clear();
        self.last.extend_from_slice(ring);
        self.has_last = true;
        Ok(())
    }

    fn invalidate(&mut self) {
        self.has_last = false;
    }
}

struct SysfsSink {
    files: Vec<File>,
    last: Vec<u8>,
    has_last: bool,
}

impl LedSink for SysfsSink {
    fn write(&mut self, px: &[Rgb]) -> Result<(), LedError> {
        for (i, f) in self.files.iter().enumerate() {
            let p = px[i / 3];
            let v = match i % 3 {
                0 => p.r,
                1 => p.g,
                _ => p.b,
            };
            if self.has_last && self.last[i] == v {
                continue;
            }
            if !home_linux::write_int_fd(f.as_raw_fd(), i32::from(v)) {
                return Err(LedError::Io(io::Error::from(io::ErrorKind::Other)));
            }
            self.last[i] = v;
        }
        self.has_last = true;
        Ok(())
    }

    fn invalidate(&mut self) {
        self.has_last = false;
    }
}

struct Output {
    sink: Box<dyn LedSink>,
    last: Vec<u8>,
    has_last: bool,
    rgb: Vec<Rgb>,
}

impl Output {
    fn new(sink: Box<dyn LedSink>, n: usize, nlogo: usize) -> Self {
        Self {
            sink,
            last: vec![0; (n + nlogo) * 3],
            has_last: false,
            rgb: Vec::with_capacity(n + nlogo),
        }
    }

    fn forget_last(&mut self) {
        self.has_last = false;
        self.sink.invalidate();
    }

    fn clear(&mut self, now: i64) {
        let off = vec![0u8; self.last.len()];
        self.has_last = false;
        self.sink.invalidate();
        self.write(&off, now);
    }

    fn write(&mut self, px: &[u8], _now: i64) -> bool {
        if self.has_last && px == &self.last[..] {
            return false;
        }
        self.rgb.clear();
        self.rgb.extend(px.as_chunks::<3>().0.iter().map(|p| Rgb {
            r: p[0],
            g: p[1],
            b: p[2],
        }));
        if self.sink.write(&self.rgb).is_err() {
            return true;
        }
        self.last.copy_from_slice(px);
        self.has_last = true;
        false
    }
}

fn led_name(leds: &Leds, s: &StripLayout, color: &str, i: i32) -> String {
    format!(
        "{}:{color}:{}{}",
        leds.prefix,
        s.sysfs_kind,
        s.first_index as i32 + i
    )
}

fn trigger_is(led: &str, trig: &str) -> bool {
    std::fs::read_to_string(format!("{LED_DIR}{led}/trigger"))
        .is_ok_and(|t| t.contains(&format!("[{trig}]")))
}

fn open_strip_fb(leds: &Leds, s: &StripLayout) -> Option<Framebuffer> {
    let LedBackend::Framebuffer {
        dev: fb_dev,
        name: fb_name,
        trigger,
    } = s.backend
    else {
        return None;
    };
    let red0 = led_name(leds, s, "red", 0);
    if !trigger_is(&red0, trigger) {
        warn!("leds: trigger {trigger} not active on {red0}");
        return None;
    }
    let dev = fb_dev.rsplit('/').next().unwrap_or(fb_dev);
    let name = std::fs::read_to_string(format!("/sys/class/graphics/{dev}/name"))
        .ok()
        .unwrap_or_default();
    let name = home_linux::trim(&name);
    if name != fb_name {
        warn!("leds: {fb_dev} is '{name}', expected '{fb_name}'");
        return None;
    }
    match Framebuffer::open(fb_dev) {
        Ok(fb) => Some(fb),
        Err(e) => {
            warn!("leds: open {fb_dev}: {}", crate::errstr(&e));
            None
        }
    }
}

fn open_i2c(dev_name: &str, addr: u16, n: usize, nlogo: usize) -> Option<I2cSink> {
    if n != 6 || nlogo != 0 {
        warn!("leds: i2c backend expects 6 ring LEDs and no logo ({n} + {nlogo})");
        return None;
    }
    let dev = match I2c::open(dev_name) {
        Ok(dev) => dev,
        Err(e) => {
            warn!("leds: open {dev_name}: {}", crate::errstr(&e));
            return None;
        }
    };
    let imax = [3u8; 18];
    if let Err(e) = dev
        .write_register(addr, 0x00, &[1])
        .and_then(|()| dev.write_register(addr, 0x26, &imax))
        .and_then(|()| dev.write_register(addr, 0x4b, &[1]))
    {
        warn!("leds: init {dev_name}@0x{addr:02x}: {}", crate::errstr(&e));
        return None;
    }
    Some(I2cSink {
        dev,
        addr,
        n,
        last: Vec::new(),
        has_last: false,
        warn: WriteWarn::default(),
        epoch: Instant::now(),
    })
}

fn open_fb(leds: &Leds, n: usize, nlogo: usize) -> Option<FrameBufferSink> {
    let ring = open_strip_fb(leds, &leds.ring);
    let logo = if nlogo > 0 {
        leds.logo.as_ref().and_then(|l| open_strip_fb(leds, l))
    } else {
        None
    };
    let ring = ring?;
    if nlogo > 0 && logo.is_none() {
        return None;
    }
    Some(FrameBufferSink {
        ring,
        logo,
        n,
        last: Vec::new(),
        has_last: false,
        bgr: Vec::with_capacity(n.max(nlogo) * 3),
        warn: Default::default(),
        epoch: Instant::now(),
    })
}

fn open_sysfs(leds: &Leds, n: usize, nlogo: usize) -> Option<SysfsSink> {
    let mut files = Vec::with_capacity((n + nlogo) * 3);
    for i in 0..n + nlogo {
        for c in ["red", "green", "blue"] {
            let name = if i < n {
                led_name(leds, &leds.ring, c, i as i32)
            } else {
                led_name(leds, leds.logo.as_ref()?, c, (i - n) as i32)
            };
            let path = format!("{LED_DIR}{name}/brightness");
            if let Ok(f) = std::fs::OpenOptions::new().write(true).open(&path) {
                files.push(f);
            } else {
                error!("leds: cannot open {path}");
                return None;
            }
        }
    }
    Some(SysfsSink {
        last: vec![0; files.len()],
        files,
        has_last: false,
    })
}

#[cfg(test)]
mod tests;
