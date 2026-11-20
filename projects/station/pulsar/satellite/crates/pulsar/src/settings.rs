use std::ffi::OsString;
use std::fs::File;
use std::io::Write as _;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use pulsar_core::Timers;
use pulsar_core::fmt::{g6, parse_f32_prefix};
use tracing::warn;

pub const SAVE_DELAY_MS: u64 = 1000;

const MUSIC_STYLES: [&str; 6] = ["Off", "All styles", "Spectrum", "Pulse", "Disco", "Polar"];
const MUSIC_STYLE_DEFAULT: &str = "All styles";
const CLOCK_MUSIC_STYLES: [&str; 2] = ["Off", "Levels"];
const VOICE_COLORS: [&str; 2] = ["Violet", "Blue"];
const ZIGBEE_ROLES: [&str; 3] = ["off", "pulsar-coordinator", "router"];

#[derive(Clone, Debug, PartialEq)]
pub struct Light {
    pub on: bool,
    pub brightness: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub effect: String,
}

impl Default for Light {
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

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub volume: f32,
    pub has_volume: bool,
    pub volume_legacy: f32,
    pub legacy_on_stock: bool,
    pub muted: bool,
    pub mic_muted: bool,
    pub clock_on: bool,
    pub clock_brightness: f32,
    pub clock_auto_brightness: bool,
    pub clock_colon_blink: bool,
    pub clock_scroll_ms: i32,
    pub light: Light,
    pub wake_word: String,
    pub tz: String,
    pub auto_update: bool,
    pub nearest_answers: bool,
    pub music_viz: String,
    pub clock_music: String,
    pub led_speed: f32,
    pub voice_indication: bool,
    pub wake_sound: bool,
    pub wake_preroll: bool,
    pub followup_duration_s: u32,
    pub wake_recording: bool,
    pub wake_dry_run: bool,
    pub announce_volume: f32,
    pub mic_gain_db: u32,
    pub led_case_color: String,
    pub voice_color: String,
    pub update_attempt: String,
    pub zigbee_role: String,
    pub zigbee_flash_failed: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            volume: 0.5,
            has_volume: false,
            volume_legacy: -1.0,
            legacy_on_stock: false,
            muted: false,
            mic_muted: false,
            clock_on: true,
            clock_brightness: 1.0,
            clock_auto_brightness: true,
            clock_colon_blink: true,
            clock_scroll_ms: 400,
            light: Light::default(),
            wake_word: "ricardo".into(),
            tz: "MSK-3".into(),
            auto_update: false,
            nearest_answers: false,
            music_viz: MUSIC_STYLE_DEFAULT.into(),
            clock_music: "Off".into(),
            led_speed: 1.0,
            voice_indication: true,
            wake_sound: true,
            wake_preroll: true,
            followup_duration_s: 10,
            wake_recording: true,
            wake_dry_run: false,
            announce_volume: 1.0,
            mic_gain_db: 34,
            led_case_color: String::new(),
            voice_color: "Violet".into(),
            update_attempt: String::new(),
            zigbee_role: "off".into(),
            zigbee_flash_failed: 0,
        }
    }
}

fn canonical(options: &[&str], value: &str, default: &str) -> String {
    if options.contains(&value) {
        value.to_owned()
    } else {
        default.to_owned()
    }
}

impl Settings {
    pub fn load(&mut self, path: &Path) -> bool {
        std::fs::read(path).is_ok_and(|text| {
            self.parse(&text);
            true
        })
    }

    pub fn parse(&mut self, text: &[u8]) {
        for line in text.split(|&b| b == b'\n') {
            let Some(eq) = line.iter().position(|&b| b == b'=') else {
                continue;
            };
            let k = &line[..eq];
            let v = String::from_utf8_lossy(&line[eq + 1..]).into_owned();
            let fv = parse_f32_prefix(&v);
            let on = v == "1";
            match k {
                b"volume2" => {
                    self.volume = fv;
                    self.has_volume = true;
                }
                b"volume" => self.volume_legacy = fv,
                b"volume_curve" => self.legacy_on_stock = v == "2",
                b"muted" => self.muted = on,
                b"mic_muted" => self.mic_muted = on,
                b"clock_on" => self.clock_on = on,
                b"clock_brightness" => self.clock_brightness = fv,
                b"clock_auto_brightness" => self.clock_auto_brightness = on,
                b"clock_colon_blink" => self.clock_colon_blink = on,
                b"clock_scroll_ms" => self.clock_scroll_ms = (fv as i32).clamp(100, 2000),
                b"light_on" => self.light.on = on,
                b"light_brightness" => self.light.brightness = fv,
                b"light_r" => self.light.r = fv,
                b"light_g" => self.light.g = fv,
                b"light_b" => self.light.b = fv,
                b"light_effect" => self.light.effect = v,
                b"wake_word" => self.wake_word = v,
                b"tz" => self.tz = v,
                b"auto_update" => self.auto_update = on,
                b"nearest_answers" => self.nearest_answers = on,
                b"update_attempt" => self.update_attempt = v,
                b"zigbee_role" => self.zigbee_role = canonical(&ZIGBEE_ROLES, &v, "off"),
                b"zigbee_flash_failed" => self.zigbee_flash_failed = strtoul_hex(&v) as u32,
                b"music_viz" => {
                    self.music_viz = canonical(&MUSIC_STYLES, &v, MUSIC_STYLE_DEFAULT);
                }
                b"clock_music" => self.clock_music = canonical(&CLOCK_MUSIC_STYLES, &v, "Off"),
                b"led_speed" => {
                    self.led_speed = if fv.is_finite() {
                        fv.clamp(0.25, 4.0)
                    } else {
                        1.0
                    };
                }
                b"voice_indication" => self.voice_indication = on,
                b"wake_sound" => self.wake_sound = on,
                b"wake_preroll" => self.wake_preroll = on,
                b"followup_duration_s" => {
                    if let Some(s) = pulsar_esphome::followup_duration_s(fv) {
                        self.followup_duration_s = s;
                    }
                }
                b"wake_event_recording" => self.wake_recording = on,
                b"wake_dry_run" => self.wake_dry_run = on,
                b"announce_volume" => {
                    self.announce_volume = if fv.is_finite() {
                        fv.clamp(0.0, 1.0)
                    } else {
                        1.0
                    };
                }
                b"mic_gain_db" => {
                    if let Some(db) = pulsar_esphome::mic_gain_db(fv) {
                        self.mic_gain_db = db;
                    }
                }
                b"led_case_color" => self.led_case_color = v,
                b"voice_color" => self.voice_color = canonical(&VOICE_COLORS, &v, "Violet"),
                _ => {}
            }
        }
    }

    pub fn serialize(&self) -> String {
        let f = |v: f32| g6(f64::from(v));
        let b = |v: bool| u8::from(v);
        format!(
            "volume2={}\nvolume={}\nmuted={}\nmic_muted={}\nclock_on={}\nclock_brightness={}\n\
             clock_scroll_ms={}\nlight_on={}\nlight_brightness={}\nlight_r={}\nlight_g={}\n\
             light_b={}\nlight_effect={}\nwake_word={}\ntz={}\nauto_update={}\nupdate_attempt={}\n\
             zigbee_role={}\nzigbee_flash_failed={:x}\nnearest_answers={}\nmusic_viz={}\n\
             led_speed={}\nvoice_indication={}\nled_case_color={}\nvoice_color={}\nclock_music={}\n\
             wake_preroll={}\nclock_auto_brightness={}\nfollowup_duration_s={}\n\
             wake_event_recording={}\nwake_dry_run={}\nannounce_volume={}\nmic_gain_db={}\n\
             wake_sound={}\nclock_colon_blink={}\n",
            f(self.volume),
            f(self.volume_legacy),
            b(self.muted),
            b(self.mic_muted),
            b(self.clock_on),
            f(self.clock_brightness),
            self.clock_scroll_ms,
            b(self.light.on),
            f(self.light.brightness),
            f(self.light.r),
            f(self.light.g),
            f(self.light.b),
            self.light.effect,
            self.wake_word,
            self.tz,
            b(self.auto_update),
            self.update_attempt,
            self.zigbee_role,
            self.zigbee_flash_failed,
            b(self.nearest_answers),
            self.music_viz,
            f(self.led_speed),
            b(self.voice_indication),
            self.led_case_color,
            self.voice_color,
            self.clock_music,
            b(self.wake_preroll),
            b(self.clock_auto_brightness),
            self.followup_duration_s,
            b(self.wake_recording),
            b(self.wake_dry_run),
            f(self.announce_volume),
            self.mic_gain_db,
            b(self.wake_sound),
            b(self.clock_colon_blink),
        )
    }

    pub fn save(&self, path: &Path) -> bool {
        let mut tmp = OsString::from(path.as_os_str());
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        let mut f = match File::create(&tmp) {
            Ok(f) => f,
            Err(e) => {
                warn!("cannot write {}: {e}", tmp.display());
                return false;
            }
        };
        let written = f
            .write_all(self.serialize().as_bytes())
            .and_then(|()| f.sync_all());
        drop(f);
        if let Err(e) = written.and_then(|()| std::fs::rename(&tmp, path)) {
            warn!("cannot write {}: {e}", path.display());
            let _ = std::fs::remove_file(&tmp);
            return false;
        }
        let dir = match path.parent() {
            Some(p) if !p.as_os_str().is_empty() => p,
            _ => Path::new("."),
        };
        if let Err(e) = File::open(dir).and_then(|d| d.sync_all()) {
            warn!("cannot sync {}: {e}", dir.display());
        }
        true
    }
}

pub struct Store {
    path: PathBuf,
    values: Settings,
    dirty: bool,
    timer: Timers<()>,
}

impl Store {
    pub fn load(path: PathBuf) -> Self {
        let mut values = Settings::default();
        values.load(&path);
        Self {
            path,
            values,
            dirty: false,
            timer: Timers::new(),
        }
    }

    #[cfg(test)]
    pub fn with_values(path: PathBuf, values: Settings) -> Self {
        Self {
            path,
            values,
            dirty: false,
            timer: Timers::new(),
        }
    }

    pub fn changed(&mut self) {
        self.dirty = true;
        self.timer.set((), SAVE_DELAY_MS);
    }

    #[must_use = "a write that must be on disk has to check it"]
    pub fn write_now(&mut self) -> bool {
        self.timer.cancel(());
        self.dirty = !self.values.save(&self.path);
        !self.dirty
    }

    pub fn flush(&mut self) {
        if self.dirty {
            let _ = self.write_now();
        }
    }

    pub async fn due(&mut self) {
        self.timer.next().await;
    }
}

impl Deref for Store {
    type Target = Settings;

    fn deref(&self) -> &Settings {
        &self.values
    }
}

impl DerefMut for Store {
    fn deref_mut(&mut self) -> &mut Settings {
        &mut self.values
    }
}

fn strtoul_hex(s: &str) -> std::ffi::c_ulong {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r') {
        i += 1;
    }
    let neg = match b.get(i) {
        Some(b'-') => {
            i += 1;
            true
        }
        Some(b'+') => {
            i += 1;
            false
        }
        _ => false,
    };
    if b.get(i) == Some(&b'0')
        && matches!(b.get(i + 1), Some(b'x' | b'X'))
        && b.get(i + 2).is_some_and(u8::is_ascii_hexdigit)
    {
        i += 2;
    }
    let mut v: std::ffi::c_ulong = 0;
    let mut overflow = false;
    while let Some(d) = b.get(i).and_then(|&c| (c as char).to_digit(16)) {
        match v
            .checked_mul(16)
            .and_then(|x| x.checked_add(std::ffi::c_ulong::from(d)))
        {
            Some(x) => v = x,
            None => overflow = true,
        }
        i += 1;
    }
    if overflow {
        std::ffi::c_ulong::MAX
    } else if neg {
        v.wrapping_neg()
    } else {
        v
    }
}

#[cfg(test)]
mod tests;
