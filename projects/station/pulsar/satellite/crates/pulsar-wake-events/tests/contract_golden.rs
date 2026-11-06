//! Байт-совместимость контракта v1: файл `golden/contract_v1.txt` — вывод прежнего
//! самописного JSON-писателя (снят с `git show HEAD` до замены на serde) на наборе фикстур,
//! включая формат чисел, порядок полей, экранирование и завершающий перевод строки.

use pulsar_wake_events::contract::{
    Sidecar, Stats, contract_json, parse_sidecar, parse_stats, sidecar_json, stats_json,
};

const GOLDEN: &str = include_str!("golden/contract_v1.txt");

/// Текст секции `name` вместе с завершающим переводом строки.
fn section(name: &str) -> &'static str {
    let head = format!("== {name}\n");
    let start = GOLDEN
        .find(&head)
        .unwrap_or_else(|| panic!("no section {name}"))
        + head.len();
    let rest = &GOLDEN[start..];
    let end = rest.find("== ").unwrap_or(rest.len());
    &rest[..end]
}

fn stats(name: &str) -> Stats {
    match name {
        "zero" => Stats::default(),
        "plain" => Stats {
            since_utc: "2026-11-01T10:00:00.000Z".into(),
            listening_seconds: 12345.6,
            hits: 10,
            near: 40,
            miss: 2,
            dry_run_listening_seconds: 3600.0,
            dropped: 0,
        },
        "fractions" => Stats {
            since_utc: "1970-01-01T00:00:00.000Z".into(),
            listening_seconds: 1.23456789,
            hits: 1,
            near: 2,
            miss: 3,
            dry_run_listening_seconds: 0.000001,
            dropped: 4,
        },
        "negzero" => Stats {
            since_utc: "x".into(),
            listening_seconds: -0.0,
            dry_run_listening_seconds: 0.0000004,
            ..Stats::default()
        },
        _ => panic!("unknown stats fixture {name}"),
    }
}

fn base_sidecar() -> Sidecar {
    Sidecar {
        schema: 1,
        id: "20261101T101500Z-hit-17".into(),
        kind: "hit".into(),
        utc: "2026-11-01T10:15:00.123Z".into(),
        sample_rate: 16000,
        pre_ms: 3000,
        post_ms: 1000,
        score_peak: 0.934,
        cutoff: 0.9,
        window: 10,
        model_sha256: "deadbeef".into(),
        pulsar_version: "v0.5.1003".into(),
        dry_run: false,
        outcome: None,
        stt_text: None,
        label: None,
        label_source: None,
        labeled_utc: None,
    }
}

fn sidecar(name: &str) -> Sidecar {
    let mut s = base_sidecar();
    match name {
        "nulls" => {}
        "full" => {
            s.score_peak = 0.9;
            s.cutoff = 0.87;
            s.outcome = Some("answered".into());
            s.stt_text = Some("привет, \"свет\"\\ок\nновая строка\tтаб\u{1}\u{8}\u{c}".into());
            s.label = Some("false".into());
            s.label_source = Some("user".into());
            s.labeled_utc = Some("2026-11-01T10:20:00.000Z".into());
        }
        "numbers" => {
            s.id = "a/b".into();
            s.kind = "near".into();
            s.utc = "2026-11-01T10:15:00.000Z".into();
            s.score_peak = 0.0004;
            s.cutoff = 0.123456789;
            s.dry_run = true;
            s.outcome = Some("dry_run".into());
        }
        _ => panic!("unknown sidecar fixture {name}"),
    }
    s
}

#[test]
fn stats_bytes_match_the_previous_writer() {
    for name in ["zero", "plain", "fractions", "negzero"] {
        let want = section(&format!("stats {name}"));
        let got = stats_json(&stats(name), "deadbeef");
        assert_eq!(got, want, "stats {name}");
        // Разбор старого файла даёт те же значения, и повторная запись — те же байты.
        let parsed = parse_stats(want).unwrap_or_else(|| panic!("stats {name} does not parse"));
        assert_eq!(
            stats_json(&parsed, "deadbeef"),
            want,
            "stats {name} round trip"
        );
    }
}

#[test]
fn sidecar_bytes_match_the_previous_writer() {
    for name in ["nulls", "full", "numbers"] {
        let want = section(&format!("sidecar {name}"));
        let got = sidecar_json(&sidecar(name));
        assert_eq!(got, want, "sidecar {name}");
        let parsed = parse_sidecar(want).unwrap_or_else(|| panic!("sidecar {name} does not parse"));
        assert_eq!(sidecar_json(&parsed), want, "sidecar {name} round trip");
    }
}

#[test]
fn generic_json_normalizes_only_the_contract_escapes() {
    assert_eq!(
        contract_json(&serde_json::json!({"a": "x\ny", "b": "\u{8}\u{c}"})),
        "{\"a\":\"x\\ny\",\"b\":\"\\u0008\\u000c\"}\n"
    );
}
