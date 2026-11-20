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

pub fn abis(code: &str) -> &'static [&'static str] {
    match code {
        "cucumber" => &["aarch64", "armhf"],
        "plum" | "station-mini2" | "bergamot" | "yandexmini" => &["armhf"],
        _ => &[],
    }
}

pub fn arch_of(code: &str, loaders: &[&str]) -> &'static str {
    for a in abis(code) {
        if loaders.contains(a) {
            return a;
        }
    }
    ""
}

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

pub fn has_zigbee(code: &str) -> bool {
    code == "cucumber"
}

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
