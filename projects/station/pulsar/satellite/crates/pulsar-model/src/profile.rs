//! Capability-структуры профиля и пять известных станций.

/// Всё, что станция сообщает о себе в Home Assistant / Music Assistant и в логе.
#[derive(Debug, PartialEq, Eq)]
pub struct Identity {
    /// Devicetree codename: "cucumber", "plum", "station-mini2", "bergamot", "yandexmini".
    pub codename: &'static str,
    /// ESPHome node name prefix; the MAC suffix is appended.
    pub host_prefix: &'static str,
    /// Shown in Home Assistant / Music Assistant.
    pub friendly_name: &'static str,
    pub model_name: &'static str,
}

/// Звук станции: захват (микрофоны и speaker reference) и воспроизведение.
#[derive(Debug, PartialEq, Eq)]
pub struct Audio {
    pub capture: Capture,
    pub playback: Playback,
}

/// Захват: interleaved samples from capture_pcm, capture_format per channel (S16_LE or S32_LE);
/// VQE gets mic_channels / spk_channels.
#[derive(Debug, PartialEq, Eq)]
pub struct Capture {
    pub pcm: &'static str,
    pub rate: u32,
    pub channels: u32,
    pub format: CaptureFormat,
    pub mic_channels: &'static [u32],
    pub spk_channels: &'static [u32],
    pub vqe_preset: &'static str,
}

/// Воспроизведение и его ALSA-настройки. `alsa`/`spk_mute_ctl`/`amp_enable_paths` —
/// необязательные части конкретной модели.
#[derive(Debug, PartialEq, Eq)]
pub struct Playback {
    /// ALSA playback PCM (dmix + the softvol "Master Playback Volume"): GStreamer's alsasink,
    /// the Sendspin sink, and the PCM the mixer opens to create the control.
    pub pcm: &'static str,
    /// Процессные ALSA-настройки модели; None — штатный /etc/asound.conf и ленивый поиск
    /// libasound по SONAME.
    pub alsa: Option<Alsa>,
    /// ALSA playback switch (on hw:0) that must be 1 (on) for the amplifier to pass audio.
    pub spk_mute_ctl: Option<&'static str>,
    /// Sysfs switches that gate the speaker amplifier; written to "1" before a playback PCM is
    /// opened.
    pub amp_enable_paths: &'static [&'static str],
}

/// Нестандартные пути libasound и asound.conf (Mini: firmware keeps both outside the loader
/// path and /etc).
#[derive(Debug, PartialEq, Eq)]
pub struct Alsa {
    /// Selected through ALSA_CONFIG_PATH before any thread starts.
    pub config: &'static str,
    /// Absolute libasound path for `pulsar-alsa`.
    pub lib: &'static str,
}

/// Sample format of the capture PCM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureFormat {
    S16,
    /// S32_LE with right-justified 24 significant bits; `shift` low bits are discarded to get
    /// the VQE's 16-bit input (8 on the Mini). A full 32-bit sample would use 16.
    S32 {
        shift: u32,
    },
}

impl CaptureFormat {
    pub fn bits(self) -> u32 {
        match self {
            Self::S16 => 16,
            Self::S32 { .. } => 32,
        }
    }
}

/// Stock volume curve: platform level -> raw value (0..255) of the softvol "Master Playback
/// Volume" on the playback PCM, piecewise linear between the points; the first point is level 0
/// (raw 0), the last one the top level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VolumeCurve {
    pub map: &'static [VolumePoint],
    /// true: any audible volume is at least the map's second point (the stock firmware never
    /// sets the raw values below it).
    pub min_step: bool,
    /// true: no hardware master control; the controller applies the volume to the player channels.
    pub software_volume: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VolumePoint {
    pub level: i32,
    pub raw: i32,
}

/// Светодиоды: кольцо и необязательный логотип.
#[derive(Debug, PartialEq, Eq)]
pub struct Leds {
    /// Prefix of the per-channel sysfs LEDs "<prefix>:<color>:<kind><index>".
    pub prefix: &'static str,
    pub ring: StripLayout,
    pub logo: Option<StripLayout>,
}

/// Полоса светодиодов и способ, которым её ведёт драйвер.
#[derive(Debug, PartialEq, Eq)]
pub struct StripLayout {
    pub count: u32,
    /// sysfs numbering starts at 0 (Midi, Mini 3) or 1 (Lite 2, Mini 2).
    pub first_index: u32,
    /// "ring" / "logo" in the sysfs LED names.
    pub sysfs_kind: &'static str,
    pub backend: LedBackend,
}

/// Как выводится полоса: framebuffer за ledfb-триггером, только sysfs brightness или
/// IS31FL323x по I²C.
#[derive(Debug, PartialEq, Eq)]
pub enum LedBackend {
    /// An Nx1 BGR framebuffer behind a ledfb trigger; per-channel sysfs LEDs as fallback.
    Framebuffer {
        dev: &'static str,
        /// Expected /sys/class/graphics/fbN/name.
        name: &'static str,
        /// LED trigger that routes the framebuffer to the LEDs.
        trigger: &'static str,
    },
    /// Per-channel sysfs brightness only.
    Sysfs,
    /// IS31FL323x I2C controller (18 channels = 6 RGB pixels).
    I2c { dev: &'static str, addr: u16 },
}

/// Часы: 4-digit 7-segment panel behind "<dir><segment><position>" sysfs LEDs.
#[derive(Debug, PartialEq, Eq)]
pub struct ClockPanel {
    /// e.g. "/sys/class/leds/cucumber:white:clock-"
    pub dir: &'static str,
    /// Name of segment a ('a' or 'A').
    pub seg_base: char,
    /// Name of the leftmost digit position.
    pub pos_base: i32,
    /// Colon LEDs.
    pub colon: &'static [&'static str],
    /// LEDs kept dark.
    pub aux: &'static [&'static str],
    /// LED at g height between digits 1 and 2.
    pub center: Option<&'static str>,
    /// true: segments switch 0/255, dimming via the driver chips' "current" attribute;
    /// false: dimming via the per-segment brightness value itself.
    pub chip_current: bool,
    /// true: the station may lack the panel; its HA entities then exist only when a segment is
    /// found at start, and a missing panel is logged at info.
    pub optional: bool,
}

/// Кнопки и переключатель микрофона.
#[derive(Debug, PartialEq, Eq)]
pub struct Input {
    /// evdev device name of the touch keys.
    pub keys_device: &'static str,
    /// Key code of the action/TALK button (KEY_VOICECOMMAND 582); None: no action button.
    pub action_key: Option<u16>,
    /// Аппаратный или программный мьют микрофона.
    pub mute: Option<MuteSwitch>,
}

/// Как устроен мьют микрофона.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MuteSwitch {
    /// Отдельное evdev-устройство: защёлкивающийся переключатель, SW_MUTE_DEVICE.
    Device(&'static str),
    /// Sysfs GPIO, low = muted, опрашивается (Mini 2).
    Gpio(u32),
    /// Моментальная кнопка KEY_MUTE на keys_device, переключает программный мьют (Mini).
    Key(u16),
}

impl Input {
    /// Whether the keys device feeds the «Button» event entity (action, action_long, volume_up,
    /// volume_down): every model with the action button also has the volume keys. The Mini's
    /// keypad has only the mute button, which toggles the Microphones switch instead.
    pub fn has_button_events(&self) -> bool {
        self.action_key.is_some()
    }

    /// true: аппаратный мьют микрофона (переключатель или GPIO), а не программный.
    pub fn hardware_mute(&self) -> bool {
        matches!(self.mute, Some(MuteSwitch::Device(_) | MuteSwitch::Gpio(_)))
    }
}

/// Ambient light sensor. `optional`: станция может быть без датчика, тогда пропуск пишется в
/// info, а сущность Illuminance существует только при успешном открытии.
#[derive(Debug, PartialEq, Eq)]
pub struct LightSensor {
    pub optional: bool,
}

/// Zigbee radio: Telink co-processor on a UART; its reset and boot lines are GPIO value files
/// (active low, 1 = asserted) exported by the kernel's zb_control driver.
#[derive(Debug, PartialEq, Eq)]
pub struct ZigbeeRadio {
    pub tty: &'static str,
    pub reset: &'static str,
    pub boot: &'static str,
}

/// Earcons, relative to /system/vendor/quasar/sounds/.
#[derive(Debug, PartialEq, Eq)]
pub struct Earcons {
    pub error: &'static str,
    pub wake: &'static str,
    pub cancel: &'static str,
    pub timer: &'static str,
    /// maind plays it (sound 45, SimpleVolumeTonePlayer) for every change except those from its
    /// muted sources "other" (app / remote) and "smart_tts" (smart volume); keys and voice words
    /// click.
    pub volume: &'static str,
}

/// Pre-roll per wake word model: audio between the end of the wake word and its confirmation,
/// sent ahead of the request. Models without measurements have no pre-roll.
#[derive(Debug, PartialEq, Eq)]
pub struct Preroll {
    pub rules: &'static [WakePreroll],
}

/// Pre-roll of one wake word model (`spotter_models/activation/<id>`), in VQE chunks (32 ms;
/// 16 ms on the Mini 3 and Mini) ending with the confirming chunk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WakePreroll {
    pub id: &'static str,
    pub rule: PrerollRule,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrerollRule {
    /// (chunks since the 1st-stage hit) + `offset`, at least 0; `fallback` chunks when no
    /// 1st-stage hit preceded the confirmation.
    FirstStage { offset: i32, fallback: u32 },
    /// Always this many chunks.
    Fixed(u32),
}

/// Полное описание станции.
#[derive(Debug, PartialEq, Eq)]
pub struct DeviceProfile {
    pub identity: Identity,
    pub audio: Audio,
    pub volume: VolumeCurve,
    pub leds: Leds,
    pub clock: Option<ClockPanel>,
    pub input: Input,
    pub lux: Option<LightSensor>,
    pub zigbee: Option<ZigbeeRadio>,
    pub earcons: Earcons,
    pub wake_preroll: Option<Preroll>,
}

// Midi, frontend_probe 2026-10-28, one talker at 1-2 m: "alisa" confirms with event 5 and the
// 1st-stage hit (event 3) always comes in the same chunk, so it anchors nothing; "yandex"
// confirms with event 2 and never reports event 3. Both get a fixed length. Words reach
// VQE_OMNI at -54..-63 dB, near the noise floor, so the end of an isolated word is often not
// found: confirmation 0..5 chunks after it, mostly 2..3 ("alisa", 10 words), 0..3 ("yandex",
// 11 words). Readings this close to the noise floor are unreliable, so the typical reading is
// used instead of the shortest; checked through HA (see the PR).
const CUCUMBER_PREROLL: Preroll = Preroll {
    rules: &[
        WakePreroll {
            id: "alisa",
            rule: PrerollRule::Fixed(2),
        },
        WakePreroll {
            id: "yandex",
            rule: PrerollRule::Fixed(2),
        },
        // The fork's own «Рикардо» microWakeWord detector (no stock model directory, so the
        // front-end looks it up under this id). 8 chunks = 256 ms: the smallest relative front cut
        // that removed the wake remnant while all three commands transcribed exactly in the
        // 2026-10-30 v27/Midi capture sweep (n=3, empirical; not a guarantee for other commands).
        WakePreroll {
            id: "ricardo",
            rule: PrerollRule::Fixed(8),
        },
    ],
};
// Lite 2, frontend_probe 2026-10-28, one talker at 1-2 m: both models confirm with event 2
// (1st stage) and never report a 1st-stage hit (event 3), so the length is fixed. Chunks from
// the end of an isolated word (VQE_OMNI 20 dB below its peak) to the confirmation, over words
// louder than -55 dB (quieter ones have their noise floor within 20 dB): "alisa" 3..8,
// "yandex" 4..9. The shortest one is used, so the word does not reach STT. Through HA,
// "yandex" with no pre-roll lost the start of the command ("который час" -> "час").
// The Mini 2 ships the Lite 2's spotter models and uses these too (not measured on it).
const PLUM_PREROLL: Preroll = Preroll {
    rules: &[
        WakePreroll {
            id: "alisa",
            rule: PrerollRule::Fixed(3),
        },
        WakePreroll {
            id: "yandex",
            rule: PrerollRule::Fixed(4),
        },
    ],
};
// Мини 3: «alisa» — замер из #30 у шумового пола, поэтому берётся типичное значение, а не самое
// короткое (см. docs/mini3.md, «Звук», и docs/voice.md, «Пре-ролл: методика измерения»); приёмка
// через HA не проводилась. «yandex» не замерялся: длина Лайт 2 во времени (128 мс при чанке VQE
// 16 мс).
const BERGAMOT_PREROLL: Preroll = Preroll {
    rules: &[
        WakePreroll {
            id: "alisa",
            rule: PrerollRule::Fixed(3),
        },
        WakePreroll {
            id: "yandex",
            rule: PrerollRule::Fixed(8),
        },
    ],
};

// { "0": 0, "1": 49, "20": 255 }: softvol min_dB -60, so 2.6 dB per level, level 1 at -48.5 dB.
// The Lite 2's quasar.cfg has the same map.
const STOCK_VOLUME: VolumeCurve = VolumeCurve {
    map: &[
        VolumePoint { level: 0, raw: 0 },
        VolumePoint { level: 1, raw: 49 },
        VolumePoint {
            level: 20,
            raw: 255,
        },
    ],
    min_step: true,
    software_volume: false,
};

// set_volume.sh on the Mini 2 is `amixer set 'Master' <percent>%`: raw = percent of 255, no dB
// offset and no lowest step.
const MINI2_VOLUME: VolumeCurve = VolumeCurve {
    map: &[
        VolumePoint { level: 0, raw: 0 },
        VolumePoint {
            level: 20,
            raw: 255,
        },
    ],
    min_step: false,
    software_volume: false,
};

// The Mini 3's quasar.cfg platformToAlsaMap: { "0": 0, "1": 33, "20": 255 }.
const MINI3_VOLUME: VolumeCurve = VolumeCurve {
    map: &[
        VolumePoint { level: 0, raw: 0 },
        VolumePoint { level: 1, raw: 33 },
        VolumePoint {
            level: 20,
            raw: 255,
        },
    ],
    min_step: true,
    software_volume: false,
};

// The Mini's stock maind curve; the raw values are applied as gain to the player channels
// because the hardware "Master" control does not attenuate the output.
const MINI_VOLUME: VolumeCurve = VolumeCurve {
    map: STOCK_VOLUME.map,
    min_step: true,
    software_volume: true,
};

/// Станция Миди: см. docs/midi.md.
pub static MIDI: DeviceProfile = DeviceProfile {
    identity: Identity {
        codename: "cucumber",
        host_prefix: "yandex-midi-",
        friendly_name: "Яндекс Станция Миди",
        model_name: "Yandex Station Midi",
    },
    audio: Audio {
        capture: Capture {
            pcm: "hw:0,2",
            rate: 96000,
            channels: 8,
            format: CaptureFormat::S16,
            mic_channels: &[0, 1, 2, 3],
            spk_channels: &[4, 5, 6, 7],
            vqe_preset: "cucumber",
        },
        playback: Playback {
            pcm: "speakers",
            alsa: None,
            spk_mute_ctl: None,
            amp_enable_paths: &[],
        },
    },
    volume: STOCK_VOLUME,
    leds: Leds {
        prefix: "cucumber",
        ring: StripLayout {
            count: 24,
            first_index: 0,
            sysfs_kind: "ring",
            backend: LedBackend::Framebuffer {
                dev: "/dev/fb0",
                name: "ring",
                trigger: "ledfb-ring",
            },
        },
        logo: Some(StripLayout {
            count: 6,
            first_index: 0,
            sysfs_kind: "logo",
            backend: LedBackend::Framebuffer {
                dev: "/dev/fb1",
                name: "logo",
                trigger: "ledfb-logo",
            },
        }),
    },
    clock: Some(ClockPanel {
        dir: "/sys/class/leds/cucumber:white:clock-",
        seg_base: 'a',
        pos_base: 1,
        // p3 + q3 form the colon; the rest are auxiliary dots/icons maind never lights for the
        // time.
        colon: &["p3", "q3"],
        aux: &[
            "deg", "h1", "h2", "l1", "l2", "m1", "m2", "p1", "p2", "p4", "p5", "q1", "q2", "q4",
            "q5",
        ],
        center: None,
        chip_current: true,
        optional: false,
    }),
    input: Input {
        keys_device: "awinic_sar",
        action_key: Some(582),
        mute: Some(MuteSwitch::Device("mute")),
    },
    lux: Some(LightSensor { optional: false }),
    // Telink B91 (quasar.cfg zigbee.tty.port). Reset and boot are GPIOZ_11 / GPIOZ_9
    // (devicetree node "zigbee", driver zb_control); maind drives them through these files when
    // quasar.cfg zigbee.zbControl is set, as it is on this firmware.
    zigbee: Some(ZigbeeRadio {
        tty: "/dev/ttyS2",
        reset: "/sys/devices/platform/zigbee/zb-reset/value",
        boot: "/sys/devices/platform/zigbee/zb-boot/value",
    }),
    earcons: Earcons {
        error: "ru-RU/vins_error.wav",
        wake: "common/spotter_start.mp3",
        cancel: "common/spotter_cancel.mp3",
        timer: "common/timer.wav",
        volume: "common/volume_sound.mp3",
    },
    wake_preroll: Some(CUCUMBER_PREROLL),
};

/// Станция Лайт 2: см. docs/lite2.md.
pub static LITE2: DeviceProfile = DeviceProfile {
    identity: Identity {
        codename: "plum",
        host_prefix: "yandex-lite2-",
        friendly_name: "Яндекс Станция Лайт 2",
        model_name: "Yandex Station Lite 2",
    },
    audio: Audio {
        capture: Capture {
            pcm: "hw:0,1",
            rate: 48000,
            channels: 8,
            format: CaptureFormat::S16,
            mic_channels: &[0, 2],
            spk_channels: &[6],
            vqe_preset: "plum",
        },
        playback: Playback {
            pcm: "speakers",
            alsa: None,
            spk_mute_ctl: None,
            amp_enable_paths: &[],
        },
    },
    volume: STOCK_VOLUME,
    leds: Leds {
        prefix: "plum",
        ring: StripLayout {
            count: 3,
            first_index: 1,
            sysfs_kind: "ring",
            backend: LedBackend::Framebuffer {
                dev: "/dev/fb0",
                name: "YNDX LED ring",
                trigger: "led_ring_fb",
            },
        },
        logo: None,
    },
    clock: Some(ClockPanel {
        dir: "/sys/class/leds/plum:white:clock",
        seg_base: 'A',
        pos_base: 0,
        colon: &["DH", "DL"],
        aux: &["H0", "H1", "H2", "H3", "DEGREE"],
        // Centre dot of the stock voice patterns (their extra LED 35).
        center: Some("DM"),
        chip_current: false,
        optional: false,
    }),
    input: Input {
        keys_device: "aw9610x",
        action_key: Some(582),
        mute: Some(MuteSwitch::Device("gpio-keys")),
    },
    lux: Some(LightSensor { optional: false }),
    zigbee: None,
    earcons: Earcons {
        error: "ru-RU/playback_error.mp3",
        wake: "common/spotter_start.mp3",
        cancel: "common/spotter_cancel.mp3",
        timer: "common/timer.wav",
        volume: "common/volume_sound.mp3",
    },
    wake_preroll: Some(PLUM_PREROLL),
};

/// Станция Мини 2: см. docs/mini2.md.
pub static MINI2: DeviceProfile = DeviceProfile {
    identity: Identity {
        codename: "station-mini2",
        host_prefix: "yandex-mini2-",
        friendly_name: "Яндекс Станция Мини 2",
        model_name: "Yandex Station Mini 2",
    },
    audio: Audio {
        capture: Capture {
            pcm: "default",
            rate: 48000,
            channels: 6,
            format: CaptureFormat::S16,
            mic_channels: &[0, 1, 2, 3],
            spk_channels: &[4, 5],
            vqe_preset: "yandexmini_2",
        },
        playback: Playback {
            pcm: "default",
            alsa: None,
            spk_mute_ctl: None,
            amp_enable_paths: &[],
        },
    },
    volume: MINI2_VOLUME,
    leds: Leds {
        prefix: "mini2",
        ring: StripLayout {
            count: 6,
            first_index: 1,
            sysfs_kind: "ring",
            backend: LedBackend::Sysfs,
        },
        logo: None,
    },
    clock: Some(ClockPanel {
        dir: "/sys/class/leds/mini2:white:clock",
        seg_base: 'A',
        pos_base: 1,
        colon: &["PLUS1", "PLUS2"],
        aux: &["DEGREE", "MINUS"],
        center: None,
        chip_current: false,
        optional: true,
    }),
    input: Input {
        keys_device: "sar0_pad0",
        action_key: Some(582),
        // GPIOZ_3: см. docs/mini2.md, «Кнопки и микрофоны».
        mute: Some(MuteSwitch::Gpio(414)),
    },
    // Датчик есть не на всех станциях: см. docs/mini2.md, «Светодиоды и часы».
    lux: Some(LightSensor { optional: true }),
    zigbee: None,
    // Earcons в .opus: см. docs/mini2.md, «Звук».
    earcons: Earcons {
        error: "ru-RU/playback_error.opus",
        wake: "common/spotter_start.opus",
        cancel: "common/spotter_cancel.opus",
        timer: "common/timer.opus",
        volume: "common/volume_sound.opus",
    },
    wake_preroll: Some(PLUM_PREROLL),
};

/// Станция Мини (первое поколение): см. docs/mini.md.
pub static MINI: DeviceProfile = DeviceProfile {
    identity: Identity {
        codename: "yandexmini",
        host_prefix: "yandex-mini-",
        friendly_name: "Яндекс Станция Мини",
        model_name: "Yandex Station Mini",
    },
    audio: Audio {
        capture: Capture {
            pcm: "hw:0,2",
            rate: 48000,
            channels: 8,
            // Right-justified 24-bit samples: keep the top 16 bits as int16.
            format: CaptureFormat::S32 { shift: 8 },
            mic_channels: &[0, 1, 2, 3],
            spk_channels: &[6, 7],
            vqe_preset: "yandexmini",
        },
        // dmix over hw:0,0; defined in /data/pulsar/asound.conf and selected via ALSA_CONFIG_PATH.
        playback: Playback {
            pcm: "mini",
            alsa: Some(Alsa {
                config: "/data/pulsar/asound.conf",
                lib: "/system/vendor/quasar/libs/libasound.so.2",
            }),
            spk_mute_ctl: Some("SPK mute"),
            // The stock maind opens the tdm_state amplifier gates; without them the ALSA sink
            // runs but the speaker stays silent (SPK mute cannot be cleared while amp_table is
            // 0). The driver clears amp_table when the playback PCM closes (observed idle
            // state), and both sinks close it when playback ends, so only the next start has to
            // set the gates again.
            amp_enable_paths: &[
                "/sys/class/tdm_state/amp_table",
                "/sys/class/tdm_state/aux_amp",
            ],
        },
    },
    volume: MINI_VOLUME,
    leds: Leds {
        prefix: "",
        ring: StripLayout {
            count: 6,
            first_index: 0,
            sysfs_kind: "",
            backend: LedBackend::I2c {
                dev: "/dev/i2c-0",
                addr: 0x3c,
            },
        },
        logo: None,
    },
    clock: None,
    input: Input {
        keys_device: "adc_keypad",
        // The retail Mini has no TALK pad and no volume keys: adc_keypad sends only KEY_MUTE.
        action_key: None,
        mute: Some(MuteSwitch::Key(113)),
    },
    lux: Some(LightSensor { optional: true }),
    zigbee: None,
    earcons: Earcons {
        error: "ru-RU/playback_error.mp3",
        wake: "common/spotter_start.mp3",
        cancel: "common/spotter_cancel.mp3",
        timer: "common/timer.wav",
        volume: "common/volume_sound.mp3",
    },
    // No pre-roll measurements on the Mini: an id without a rule gives an empty pre-roll.
    wake_preroll: None,
};

/// Станция Мини 3: см. docs/mini3.md.
pub static MINI3: DeviceProfile = DeviceProfile {
    identity: Identity {
        codename: "bergamot",
        host_prefix: "yandex-mini3-",
        friendly_name: "Яндекс Станция Мини 3",
        model_name: "Yandex Station Mini 3",
    },
    audio: Audio {
        capture: Capture {
            pcm: "hw:0,1",
            rate: 48000,
            channels: 8,
            format: CaptureFormat::S16,
            mic_channels: &[0, 1, 2],
            spk_channels: &[6],
            vqe_preset: "bergamot",
        },
        playback: Playback {
            pcm: "speakers",
            alsa: None,
            spk_mute_ctl: None,
            amp_enable_paths: &[],
        },
    },
    volume: MINI3_VOLUME,
    leds: Leds {
        prefix: "bergamot",
        // The driver calls the 6-zone ring "logo" (/dev/fb0 6×1, AW21018); there is no separate
        // logo.
        ring: StripLayout {
            count: 6,
            first_index: 0,
            sysfs_kind: "logo",
            backend: LedBackend::Framebuffer {
                dev: "/dev/fb0",
                name: "logo",
                trigger: "ledfb-logo",
            },
        },
        logo: None,
    },
    clock: Some(ClockPanel {
        dir: "/sys/class/leds/bergamot:white:clock",
        seg_base: 'A',
        pos_base: 0,
        colon: &["DH", "DL"],
        aux: &["DEGREE"],
        center: Some("DM"),
        chip_current: false,
        // Sold with and without the LED clock (AW21036): segments are looked up at start.
        optional: true,
    }),
    input: Input {
        keys_device: "aw933xx",
        action_key: Some(582),
        mute: Some(MuteSwitch::Device("gpio-keys")),
    },
    lux: Some(LightSensor { optional: false }),
    zigbee: None,
    earcons: Earcons {
        error: "ru-RU/playback_error.mp3",
        wake: "common/spotter_start.mp3",
        cancel: "common/spotter_cancel.mp3",
        timer: "common/timer.wav",
        volume: "common/volume_sound.mp3",
    },
    wake_preroll: Some(BERGAMOT_PREROLL),
};

/// Все профили: для табличных тестов и проверок полноты.
pub const ALL: [&DeviceProfile; 5] = [&MIDI, &LITE2, &MINI2, &MINI, &MINI3];

#[cfg(test)]
mod tests {
    use super::*;

    /// Снимок раскладок и решений по каждой модели: то, что старый плоский `Model` задавал
    /// полями, проверяется здесь один раз.
    #[test]
    fn audio_capture_layouts_per_model() {
        let want = [
            (
                &MIDI,
                "hw:0,2",
                96000,
                8,
                CaptureFormat::S16,
                &[0, 1, 2, 3][..],
                &[4, 5, 6, 7][..],
                "cucumber",
            ),
            (
                &LITE2,
                "hw:0,1",
                48000,
                8,
                CaptureFormat::S16,
                &[0, 2],
                &[6],
                "plum",
            ),
            (
                &MINI2,
                "default",
                48000,
                6,
                CaptureFormat::S16,
                &[0, 1, 2, 3],
                &[4, 5],
                "yandexmini_2",
            ),
            (
                &MINI,
                "hw:0,2",
                48000,
                8,
                CaptureFormat::S32 { shift: 8 },
                &[0, 1, 2, 3],
                &[6, 7],
                "yandexmini",
            ),
            (
                &MINI3,
                "hw:0,1",
                48000,
                8,
                CaptureFormat::S16,
                &[0, 1, 2],
                &[6],
                "bergamot",
            ),
        ];
        for (p, pcm, rate, channels, format, mic, spk, preset) in want {
            let c = &p.audio.capture;
            assert_eq!(
                (
                    c.pcm,
                    c.rate,
                    c.channels,
                    c.format,
                    c.mic_channels,
                    c.spk_channels,
                    c.vqe_preset
                ),
                (pcm, rate, channels, format, mic, spk, preset),
                "{}",
                p.identity.codename
            );
        }
    }

    #[test]
    fn playback_layouts_per_model() {
        let want = [
            (&MIDI, "speakers", None, None, 0usize),
            (&LITE2, "speakers", None, None, 0),
            (&MINI2, "default", None, None, 0),
            (
                &MINI,
                "mini",
                Some("/data/pulsar/asound.conf"),
                Some("SPK mute"),
                2,
            ),
            (&MINI3, "speakers", None, None, 0),
        ];
        for (p, pcm, config, mute, amps) in want {
            let pb = &p.audio.playback;
            assert_eq!(
                (
                    pb.pcm,
                    pb.alsa.as_ref().map(|a| a.config),
                    pb.spk_mute_ctl,
                    pb.amp_enable_paths.len()
                ),
                (pcm, config, mute, amps),
                "{}",
                p.identity.codename
            );
        }
        let mini = &MINI.audio.playback;
        assert_eq!(
            mini.alsa.as_ref().unwrap().lib,
            "/system/vendor/quasar/libs/libasound.so.2"
        );
    }

    #[test]
    fn volume_curves_per_model() {
        let map = |p: &DeviceProfile| {
            p.volume
                .map
                .iter()
                .map(|v| (v.level, v.raw))
                .collect::<Vec<_>>()
        };
        assert_eq!(map(&MIDI), [(0, 0), (1, 49), (20, 255)]);
        assert_eq!(map(&LITE2), [(0, 0), (1, 49), (20, 255)]);
        assert_eq!(map(&MINI2), [(0, 0), (20, 255)]);
        assert_eq!(map(&MINI), [(0, 0), (1, 49), (20, 255)]);
        assert_eq!(map(&MINI3), [(0, 0), (1, 33), (20, 255)]);
        for p in ALL {
            assert_eq!(p.volume.min_step, p.identity.codename != "station-mini2");
            assert_eq!(
                p.volume.software_volume,
                p.identity.codename == "yandexmini"
            );
        }
    }

    #[test]
    fn led_layouts_per_model() {
        // prefix, ring (count, first, kind), backend, logo count
        let want = [
            (
                &MIDI,
                "cucumber",
                (24, 0, "ring"),
                "fb /dev/fb0 ring ledfb-ring",
                Some((6, 0, "logo", "/dev/fb1", "logo", "ledfb-logo")),
            ),
            (
                &LITE2,
                "plum",
                (3, 1, "ring"),
                "fb /dev/fb0 YNDX LED ring led_ring_fb",
                None,
            ),
            (&MINI2, "mini2", (6, 1, "ring"), "sysfs", None),
            (&MINI, "", (6, 0, ""), "i2c /dev/i2c-0 0x3c", None),
            (
                &MINI3,
                "bergamot",
                (6, 0, "logo"),
                "fb /dev/fb0 logo ledfb-logo",
                None,
            ),
        ];
        for (p, prefix, ring, backend, logo) in want {
            let leds = &p.leds;
            assert_eq!(leds.prefix, prefix, "{}", p.identity.codename);
            assert_eq!(
                (leds.ring.count, leds.ring.first_index, leds.ring.sysfs_kind),
                ring
            );
            let got = match leds.ring.backend {
                LedBackend::Framebuffer { dev, name, trigger } => {
                    format!("fb {dev} {name} {trigger}")
                }
                LedBackend::Sysfs => "sysfs".to_owned(),
                LedBackend::I2c { dev, addr } => format!("i2c {dev} 0x{addr:02x}"),
            };
            assert_eq!(got, backend, "{}", p.identity.codename);
            let logo = logo.map(|(count, first, kind, dev, name, trigger)| {
                let strip = leds.logo.as_ref().unwrap();
                assert_eq!(
                    (strip.count, strip.first_index, strip.sysfs_kind),
                    (count, first, kind)
                );
                assert_eq!(
                    strip.backend,
                    LedBackend::Framebuffer { dev, name, trigger }
                );
            });
            assert_eq!(logo.is_some(), leds.logo.is_some());
        }
    }

    #[test]
    fn clock_layouts_per_model() {
        let midi = MIDI.clock.as_ref().unwrap();
        assert_eq!(
            (
                midi.dir,
                midi.seg_base,
                midi.pos_base,
                midi.colon,
                midi.aux.len(),
                midi.center,
                midi.chip_current,
                midi.optional
            ),
            (
                "/sys/class/leds/cucumber:white:clock-",
                'a',
                1,
                &["p3", "q3"][..],
                15,
                None,
                true,
                false
            )
        );
        let lite2 = LITE2.clock.as_ref().unwrap();
        assert_eq!(
            (
                lite2.dir,
                lite2.seg_base,
                lite2.pos_base,
                lite2.colon,
                lite2.center
            ),
            (
                "/sys/class/leds/plum:white:clock",
                'A',
                0,
                &["DH", "DL"][..],
                Some("DM")
            )
        );
        assert!(!lite2.chip_current && !lite2.optional);

        let mini2 = MINI2.clock.as_ref().unwrap();
        assert_eq!(
            (mini2.dir, mini2.pos_base, mini2.colon, mini2.center),
            (
                "/sys/class/leds/mini2:white:clock",
                1,
                &["PLUS1", "PLUS2"][..],
                None
            )
        );
        assert!(mini2.optional);

        let mini3 = MINI3.clock.as_ref().unwrap();
        assert_eq!(
            (mini3.dir, mini3.pos_base, mini3.colon, mini3.center),
            (
                "/sys/class/leds/bergamot:white:clock",
                0,
                &["DH", "DL"][..],
                Some("DM")
            )
        );
        assert!(mini3.optional);

        assert!(MINI.clock.is_none());
    }

    #[test]
    fn input_zigbee_lux_and_preroll_per_model() {
        let want = [
            (
                &MIDI,
                Some(582),
                Some(MuteSwitch::Device("mute")),
                true,
                false,
                true,
            ),
            (
                &LITE2,
                Some(582),
                Some(MuteSwitch::Device("gpio-keys")),
                false,
                false,
                true,
            ),
            (
                &MINI2,
                Some(582),
                Some(MuteSwitch::Gpio(414)),
                false,
                true,
                true,
            ),
            (&MINI, None, Some(MuteSwitch::Key(113)), false, true, false),
            (
                &MINI3,
                Some(582),
                Some(MuteSwitch::Device("gpio-keys")),
                false,
                false,
                true,
            ),
        ];
        for (p, action, mute, zigbee, lux_optional, preroll) in want {
            assert_eq!(p.input.action_key, action, "{}", p.identity.codename);
            assert_eq!(p.input.mute, mute, "{}", p.identity.codename);
            assert_eq!(p.zigbee.is_some(), zigbee, "{}", p.identity.codename);
            assert_eq!(
                p.lux.as_ref().unwrap().optional,
                lux_optional,
                "{}",
                p.identity.codename
            );
            assert_eq!(p.wake_preroll.is_some(), preroll, "{}", p.identity.codename);
        }
        assert_eq!(MIDI.zigbee.as_ref().unwrap().tty, "/dev/ttyS2");
    }
}
