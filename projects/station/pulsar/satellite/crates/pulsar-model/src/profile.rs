#[derive(Debug, PartialEq, Eq)]
pub struct Identity {
    pub codename: &'static str,
    pub host_prefix: &'static str,
    pub friendly_name: &'static str,
    pub model_name: &'static str,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Audio {
    pub capture: Capture,
    pub playback: Playback,
}

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

#[derive(Debug, PartialEq, Eq)]
pub struct Playback {
    pub pcm: &'static str,
    pub alsa: Option<Alsa>,
    pub spk_mute_ctl: Option<&'static str>,
    pub amp_enable_paths: &'static [&'static str],
}

#[derive(Debug, PartialEq, Eq)]
pub struct Alsa {
    pub config: &'static str,
    pub lib: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureFormat {
    S16,
    S32 { shift: u32 },
}

impl CaptureFormat {
    pub fn bits(self) -> u32 {
        match self {
            Self::S16 => 16,
            Self::S32 { .. } => 32,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VolumeCurve {
    pub map: &'static [VolumePoint],
    pub min_step: bool,
    pub software_volume: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VolumePoint {
    pub level: i32,
    pub raw: i32,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Leds {
    pub prefix: &'static str,
    pub ring: StripLayout,
    pub logo: Option<StripLayout>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct StripLayout {
    pub count: u32,
    pub first_index: u32,
    pub sysfs_kind: &'static str,
    pub backend: LedBackend,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LedBackend {
    Framebuffer {
        dev: &'static str,
        name: &'static str,
        trigger: &'static str,
    },
    Sysfs,
    I2c {
        dev: &'static str,
        addr: u16,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub struct ClockPanel {
    pub dir: &'static str,
    pub seg_base: char,
    pub pos_base: i32,
    pub colon: &'static [&'static str],
    pub aux: &'static [&'static str],
    pub center: Option<&'static str>,
    pub chip_current: bool,
    pub optional: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Input {
    pub keys_device: &'static str,
    pub action_key: Option<u16>,
    pub mute: Option<MuteSwitch>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MuteSwitch {
    Device(&'static str),
    Gpio(u32),
    Key(u16),
}

impl Input {
    pub fn has_button_events(&self) -> bool {
        self.action_key.is_some()
    }

    pub fn hardware_mute(&self) -> bool {
        matches!(self.mute, Some(MuteSwitch::Device(_) | MuteSwitch::Gpio(_)))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct LightSensor {
    pub optional: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ZigbeeRadio {
    pub tty: &'static str,
    pub reset: &'static str,
    pub boot: &'static str,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Earcons {
    pub error: &'static str,
    pub wake: &'static str,
    pub cancel: &'static str,
    pub timer: &'static str,
    pub volume: &'static str,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Preroll {
    pub rules: &'static [WakePreroll],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WakePreroll {
    pub id: &'static str,
    pub rule: PrerollRule,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrerollRule {
    FirstStage { offset: i32, fallback: u32 },
    Fixed(u32),
}

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
        WakePreroll {
            id: "ricardo",
            rule: PrerollRule::Fixed(8),
        },
    ],
};
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

const MINI_VOLUME: VolumeCurve = VolumeCurve {
    map: STOCK_VOLUME.map,
    min_step: true,
    software_volume: true,
};

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
        mute: Some(MuteSwitch::Gpio(414)),
    },
    lux: Some(LightSensor { optional: true }),
    zigbee: None,
    earcons: Earcons {
        error: "ru-RU/playback_error.opus",
        wake: "common/spotter_start.opus",
        cancel: "common/spotter_cancel.opus",
        timer: "common/timer.opus",
        volume: "common/volume_sound.opus",
    },
    wake_preroll: Some(PLUM_PREROLL),
};

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
            format: CaptureFormat::S32 { shift: 8 },
            mic_channels: &[0, 1, 2, 3],
            spk_channels: &[6, 7],
            vqe_preset: "yandexmini",
        },
        playback: Playback {
            pcm: "mini",
            alsa: Some(Alsa {
                config: "/data/pulsar/asound.conf",
                lib: "/system/vendor/quasar/libs/libasound.so.2",
            }),
            spk_mute_ctl: None,
            amp_enable_paths: &["/sys/class/tdm_state/amp_table"],
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
    wake_preroll: None,
};

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

pub const ALL: [&DeviceProfile; 5] = [&MIDI, &LITE2, &MINI2, &MINI, &MINI3];

#[cfg(test)]
mod tests {
    use super::*;

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
            (&MINI, "mini", Some("/data/pulsar/asound.conf"), None, 1),
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
