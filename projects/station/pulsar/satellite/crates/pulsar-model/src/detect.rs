//! Определение модели станции по devicetree / `quasar.cfg` и процессные ALSA-настройки.
//!
//! Единственный модуль крейта, который читает файлы и окружение; разбор отделён от чтения и
//! покрыт тестами.
#![allow(unsafe_code)]

use std::sync::LazyLock;

use crate::profile::{Alsa, DeviceProfile, LITE2, MIDI, MINI, MINI2, MINI3, Playback};

/// Process-wide ALSA setup, before any thread or audio library is started. Only the Mini sets
/// ALSA_CONFIG_PATH: its dmix PCM lives in /data/pulsar/asound.conf (put there by
/// pulsar-install). A missing file leaves the firmware's config, where that PCM does not exist.
impl Playback {
    pub fn apply_env(&self) {
        let Some(alsa) = &self.alsa else {
            return;
        };
        if std::path::Path::new(alsa.config).is_file() {
            // SAFETY: daemon startup calls this before spawning any thread or loading a library.
            unsafe { std::env::set_var("ALSA_CONFIG_PATH", alsa.config) };
        } else {
            tracing::error!(
                "alsa: {} missing, ALSA_CONFIG_PATH not set: '{}' cannot open until pulsar-install \
                 puts it back",
                alsa.config,
                self.pcm
            );
        }
    }

    /// Absolute libasound path of the model, "" when the usual SONAME lookup should be used.
    pub fn alsa_lib(&self) -> &'static str {
        self.alsa.as_ref().map_or("", |a: &Alsa| a.lib)
    }
}

/// The `deviceType` value of quasar.cfg (e.g. "yandexmini"), the firmware's own model marker.
pub fn quasar_device_type(cfg: &str) -> Option<&str> {
    const KEY: &str = "\"deviceType\"";
    let after = cfg.find(KEY)? + KEY.len();
    let rest = cfg[after..].trim_start().strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(&rest[..end])
}

/// The Mini model from its board id, but only when quasar.cfg confirms a Yandex firmware:
/// `axg_s420_v03` is an Amlogic S420 reference board id, not a Yandex-specific one.
pub fn mini_from_board_and_cfg(dt_id: &[u8], quasar_cfg: &str) -> Option<&'static DeviceProfile> {
    (quasar_device_type(quasar_cfg) == Some("yandexmini"))
        .then(|| profile_from_dt_id(dt_id))
        .flatten()
}

/// The profile of this device, read once; None (logged) on unsupported hardware.
pub fn detect_profile() -> Option<&'static DeviceProfile> {
    static PROFILE: LazyLock<Option<&'static DeviceProfile>> = LazyLock::new(|| {
        let raw = std::fs::read("/proc/device-tree/compatible").unwrap_or_default();
        let profile = profile_from_compatible(&raw).or_else(|| {
            let dt = std::fs::read("/proc/device-tree/amlogic-dt-id").unwrap_or_default();
            let cfg =
                std::fs::read_to_string("/system/vendor/quasar/quasar.cfg").unwrap_or_default();
            mini_from_board_and_cfg(&dt, &cfg)
        });
        if profile.is_none() {
            let shown = String::from_utf8_lossy(&raw).replace('\0', " ");
            let dt = std::fs::read("/proc/device-tree/amlogic-dt-id").unwrap_or_default();
            let dt = String::from_utf8_lossy(&dt);
            tracing::error!(
                "unsupported device (devicetree compatible: {shown}, amlogic-dt-id: '{}')",
                dt.trim_end_matches('\0')
            );
        }
        profile
    });
    *PROFILE
}

/// The profile of the first exact "yandex,<codename>" entry of a NUL-separated compatible list,
/// e.g. "yandex,cucumber-dvt2\0yandex,cucumber\0amlogic, a5\0"; a board variant alone
/// ("yandex,bergamot-evt1") is no model.
pub fn profile_from_compatible(raw: &[u8]) -> Option<&'static DeviceProfile> {
    raw.split(|&b| b == 0).find_map(|entry| match entry {
        b"yandex,cucumber" => Some(&MIDI),
        b"yandex,plum" => Some(&LITE2),
        b"yandex,station-mini2" => Some(&MINI2),
        b"yandex,bergamot" => Some(&MINI3),
        _ => None,
    })
}

/// The Mini's devicetree has no yandex,* entry. The board id alone is not a Yandex marker; use
/// [`mini_from_board_and_cfg`] unless testing the id parser.
pub fn profile_from_dt_id(raw: &[u8]) -> Option<&'static DeviceProfile> {
    let id = raw.split(|&b| b == 0).next().unwrap_or_default();
    (id == b"axg_s420_v03").then_some(&MINI)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cucumber_is_midi() {
        let m = profile_from_compatible(b"yandex,cucumber-dvt2\0yandex,cucumber\0amlogic, a5\0")
            .unwrap();
        assert_eq!(m.identity.codename, "cucumber");
        assert_eq!(m.audio.capture.rate, 96000);
        assert_eq!(m.leds.logo.as_ref().unwrap().count, 6);
        assert_eq!(m.zigbee.as_ref().unwrap().tty, "/dev/ttyS2");
    }

    #[test]
    fn plum_is_lite2() {
        let m = profile_from_compatible(b"yandex,plum\0amlogic, axg\0").unwrap();
        assert_eq!(m.identity.codename, "plum");
        assert_eq!(m.leds.ring.first_index, 1);
        assert!(m.leds.logo.is_none());
        assert!(m.zigbee.is_none());
        // No trailing NUL: the last entry still counts.
        assert_eq!(
            profile_from_compatible(b"amlogic, axg\0yandex,plum")
                .unwrap()
                .identity
                .codename,
            "plum"
        );
    }

    #[test]
    fn station_mini2_is_mini2() {
        let m = profile_from_compatible(b"yandex,station-mini2\0amlogic, axg\0").unwrap();
        assert_eq!(m.identity.codename, "station-mini2");
        assert_eq!(m.audio.playback.pcm, "default");
        assert!(m.input.mute == Some(crate::profile::MuteSwitch::Gpio(414)));
        let clock = m.clock.as_ref().unwrap();
        assert!(clock.optional && m.lux.as_ref().unwrap().optional && !m.volume.min_step);
        assert_eq!(m.audio.capture.format, crate::profile::CaptureFormat::S16);
        assert_eq!(m.input.action_key, Some(582));
        assert!(m.input.has_button_events());
    }

    #[test]
    fn mini_is_detected_by_board_id() {
        let m = profile_from_dt_id(b"axg_s420_v03\0").unwrap();
        assert_eq!(m.identity.codename, "yandexmini");
        assert_eq!(m.audio.capture.rate, 48000);
        assert_eq!(
            m.audio.capture.format,
            crate::profile::CaptureFormat::S32 { shift: 8 }
        );
        assert!(matches!(
            m.leds.ring.backend,
            crate::profile::LedBackend::I2c { addr: 0x3c, .. }
        ));
        assert_eq!(m.leds.ring.count, 6);
        assert!(m.leds.logo.is_none());
        assert!(m.volume.software_volume);
        assert!(m.clock.is_none());
        // Only KEY_MUTE on the keypad: it toggles the mute, and there is no «Button» entity.
        assert_eq!(
            (m.input.action_key, m.input.mute),
            (None, Some(crate::profile::MuteSwitch::Key(113)))
        );
        assert!(!m.input.has_button_events());
        assert!(!m.input.hardware_mute());
        // No trailing NUL still counts.
        assert_eq!(
            profile_from_dt_id(b"axg_s420_v03")
                .unwrap()
                .identity
                .codename,
            "yandexmini"
        );
        assert!(profile_from_dt_id(b"axg_s420_v02\0").is_none());
        // axg_s420_v03 is an Amlogic reference-board id: only a Yandex firmware makes it a Mini.
        let cfg = r#"{"deviceType" : "yandexmini", "x": 1}"#;
        assert_eq!(
            mini_from_board_and_cfg(b"axg_s420_v03\0", cfg)
                .unwrap()
                .identity
                .codename,
            "yandexmini"
        );
        assert!(mini_from_board_and_cfg(b"axg_s420_v03\0", r#"{"deviceType":"other"}"#).is_none());
        assert!(mini_from_board_and_cfg(b"axg_s420_v02\0", cfg).is_none());
        assert_eq!(quasar_device_type(cfg), Some("yandexmini"));
        assert_eq!(quasar_device_type("{}"), None);
        // The installer and boot.sh reject these too.
        assert_eq!(quasar_device_type(r#"{"deviceType" "yandexmini"}"#), None);
        assert_eq!(quasar_device_type(r#"{"deviceType": yandexmini}"#), None);
    }

    #[test]
    fn bergamot_is_mini3() {
        let m = profile_from_compatible(b"yandex,bergamot-evt1\0yandex,bergamot\0amlogic, axg\0")
            .unwrap();
        assert_eq!(m.identity.codename, "bergamot");
        assert_eq!(m.audio.capture.vqe_preset, "bergamot");
        assert_eq!(m.audio.capture.mic_channels, &[0, 1, 2]);
        assert_eq!(m.audio.capture.spk_channels, &[6]);
        assert_eq!(m.leds.ring.count, 6);
        assert!(matches!(
            m.leds.ring.backend,
            crate::profile::LedBackend::Framebuffer { name: "logo", .. }
        ));
        assert!(m.leds.logo.is_none());
        assert!(m.clock.as_ref().unwrap().optional);
        assert!(!m.lux.as_ref().unwrap().optional);
    }

    #[test]
    fn bergamot_requires_exact_compatible() {
        // A board variant alone, also next to a non-yandex "bergamot".
        for raw in [
            b"yandex,bergamot-evt1\0amlogic, axg\0".as_slice(),
            b"yandex,bergamot-evt1\0amlogic,bergamot\0".as_slice(),
        ] {
            assert!(profile_from_compatible(raw).is_none(), "{raw:?}");
        }
    }

    #[test]
    fn unknown_hardware() {
        assert!(profile_from_compatible(b"").is_none());
        assert!(profile_from_compatible(b"yandex,cucumber-dvt2\0amlogic, a5\0").is_none());
        assert!(profile_from_compatible(b"yandex,yandexmini\0").is_none());
        assert!(profile_from_compatible(b"xyandex,plum\0").is_none());
        // The old installer pattern cut "yandex,station-mini2" at the hyphen.
        assert!(profile_from_compatible(b"yandex,station\0").is_none());
    }

    #[test]
    fn first_known_entry_wins() {
        assert_eq!(
            profile_from_compatible(b"yandex,plum\0yandex,cucumber\0")
                .unwrap()
                .identity
                .codename,
            "plum"
        );
        assert_eq!(
            profile_from_compatible(b"yandex,cucumber\0yandex,plum\0")
                .unwrap()
                .identity
                .codename,
            "cucumber"
        );
    }
}
