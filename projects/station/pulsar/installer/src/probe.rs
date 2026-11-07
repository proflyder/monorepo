//! Model detection: the devicetree codename, the supported userspace ABI and the Mini's
//! board-id + `quasar.cfg` pair. Must agree with `pulsar-model/src/detect.rs` and
//! `satellite/deploy/boot.sh` (see docs/development.md, «Установка и станция»).

/// Devicetree codename -> name. Only models listed in `abis` get installed: the daemon
/// drives model-specific hardware (mic array + VQE preset, LEDs, clock, inputs).
pub fn model_name(code: &str) -> &'static str {
    match code {
        "cucumber" => "Yandex Station Midi",
        "plum" => "Yandex Station Lite 2",
        "station-mini2" => "Yandex Station Mini 2",
        "bergamot" => "Yandex Station Mini 3",
        "yandexmini" => "Yandex Station Mini",
        _ => "",
    }
}

/// Userspace ABIs a model's firmware runs, its usual one first; empty = model not
/// supported. The installed binary is build/<arch>/pulsar for the first ABI whose dynamic
/// loader is on the station. A second ABI is only for a userspace actually seen on that
/// model: some Midi images run a 32-bit userspace on the 64-bit kernel (#13) and need the
/// armhf build (its ALSA ABI is the one in audio/alsa_abi.h, #14). Lite 2, Mini 2, Mini 3
/// and Mini stay armhf-only: no 64-bit userspace has been seen on them, and the aarch64
/// binary needs glibc 2.34 — on a miss it would crash-loop back to the Yandex stack (no
/// `.prev` on a first install). The armhf build targets glibc 2.23, the Mini's; the 2.30
/// userlands of the Lite 2, Mini 2 and Mini 3 run it as well.
pub fn abis(code: &str) -> &'static [&'static str] {
    match code {
        "cucumber" => &["aarch64", "armhf"],
        "plum" | "station-mini2" | "bergamot" | "yandexmini" => &["armhf"],
        _ => &[],
    }
}

/// The model's first ABI whose loader is on the station (loaders: the words the loader
/// probe printed). `""` = none of the model's ABIs: skip the station.
pub fn arch_of(code: &str, loaders: &[&str]) -> &'static str {
    for a in abis(code) {
        if loaders.contains(a) {
            return a;
        }
    }
    ""
}

/// The `deviceType` string of quasar.cfg, the firmware's own model marker. Deliberately a
/// tiny parser: the file is JSON-like and only this key is needed.
pub fn quasar_device_type(cfg: &str) -> &str {
    const KEY: &str = "\"deviceType\"";
    let Some(i) = cfg.find(KEY) else {
        return "";
    };
    let Some(rest) = cfg[i + KEY.len()..].trim_start().strip_prefix(':') else {
        return "";
    };
    let Some(rest) = rest.trim_start().strip_prefix('"') else {
        return "";
    };
    rest.find('"').map_or("", |j| &rest[..j])
}

/// The model has the Zigbee radio the daemon can run as a router (`radio/`), so the router
/// firmware image is installed next to the daemon.
pub fn has_zigbee(code: &str) -> bool {
    code == "cucumber"
}

/// Picks the model from the devicetree compatible list (NUL-separated, one entry per line
/// or space-separated here). The first entry with a supported codename wins: the list
/// starts with board-specific variants, e.g. "yandex,cucumber-dvt2" before "yandex,cucumber"
/// on the Midi. The first codename is returned even when nothing is supported, so that
/// probe can name the unknown device.
pub fn station_code(compat: &str) -> (String, bool) {
    let mut first = "";
    for entry in compat.split_whitespace() {
        let Some(code) = entry.strip_prefix("yandex,") else {
            continue;
        };
        if code.is_empty() {
            continue;
        }
        if first.is_empty() {
            first = code;
        }
        if !abis(code).is_empty() {
            return (code.to_string(), true);
        }
    }
    (first.to_string(), false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("../testdata/probe.json")).unwrap()
    }

    #[test]
    fn names_abis_and_zigbee_match_go_golden() {
        let f = fixture();
        for (code, want) in f["models"].as_object().unwrap() {
            assert_eq!(model_name(code), want.as_str().unwrap(), "model {code}");
        }
        for (code, want) in f["abis"].as_object().unwrap() {
            let want: Vec<&str> = want
                .as_array()
                .map(|a| a.iter().map(|v| v.as_str().unwrap()).collect())
                .unwrap_or_default();
            assert_eq!(abis(code), want.as_slice(), "abis {code}");
        }
        let zigbee: Vec<&str> = f["zigbee"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        for code in [
            "cucumber",
            "plum",
            "station-mini2",
            "bergamot",
            "yandexmini",
            "station",
        ] {
            assert_eq!(has_zigbee(code), zigbee.contains(&code), "zigbee {code}");
        }
    }

    #[test]
    fn arch_choice_matches_go_golden() {
        for c in fixture()["arch"].as_array().unwrap() {
            let loaders: Vec<&str> = c["loaders"]
                .as_array()
                .map(|a| a.iter().map(|v| v.as_str().unwrap()).collect())
                .unwrap_or_default();
            assert_eq!(
                arch_of(c["code"].as_str().unwrap(), &loaders),
                c["want"].as_str().unwrap()
            );
        }
    }

    #[test]
    fn quasar_device_type_matches_go_golden() {
        for c in fixture()["quasar"].as_array().unwrap() {
            assert_eq!(
                quasar_device_type(c["cfg"].as_str().unwrap()),
                c["want"].as_str().unwrap()
            );
        }
    }

    #[test]
    fn station_code_matches_go_golden() {
        for c in fixture()["station_code"].as_array().unwrap() {
            assert_eq!(
                station_code(c["compat"].as_str().unwrap()),
                (
                    c["code"].as_str().unwrap().to_string(),
                    c["supported"].as_bool().unwrap()
                )
            );
        }
    }

    /// The daemon's detector is the authority for these codenames; keep the two tables in
    /// step (detect.rs reads NUL-separated bytes, the installer trims to words).
    #[test]
    fn matches_daemon_detector_table() {
        for (raw, code) in [
            (
                b"yandex,cucumber-dvt2\0yandex,cucumber\0".as_slice(),
                "cucumber",
            ),
            (b"yandex,plum\0".as_slice(), "plum"),
            (b"yandex,station-mini2\0".as_slice(), "station-mini2"),
            (b"yandex,bergamot\0".as_slice(), "bergamot"),
        ] {
            let compat = String::from_utf8_lossy(raw).replace('\0', " ");
            assert_eq!(station_code(&compat), (code.to_string(), true));
        }
        assert_eq!(model_name("yandexmini"), "Yandex Station Mini");
        assert_eq!(
            quasar_device_type("{\"deviceType\":\"yandexmini\"}"),
            "yandexmini"
        );
    }
}
