//! Контракт v1 wake-событий: сайдкары `<id>.json`, `stats.json` и манифест модели.
//!
//! Формат читают тренировочные инструменты, поэтому он зафиксирован байт в байт: порядок полей,
//! имена и запись чисел. Структуру и разбор даёт serde, числа пишутся сериализаторами
//! [`Num6`] / [`Num3`], потому что формат контракта — `%.6` / `%.3` без экспоненты, а не
//! кратчайшее представление `f64`. Строки пишет `serde_json`; его единственное расхождение с
//! контрактом (`\b` и `\f` вместо `\u0008` / `\u000c`) правит [`contract_json`].

use serde::{Deserialize, Serialize, Serializer};
use serde_json::value::RawValue;

/// Число контракта с точностью 6 знаков после запятой, без экспоненты.
#[derive(Clone, Copy, Debug)]
pub struct Num6(pub f64);

/// Число контракта с точностью 3 знака после запятой, без экспоненты.
#[derive(Clone, Copy, Debug)]
pub struct Num3(pub f64);

impl Serialize for Num6 {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        raw_number(num6(self.0), s)
    }
}

impl Serialize for Num3 {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        raw_number(num3(self.0), s)
    }
}

/// Литерал числа как есть: `RawValue` не даёт `f64`-сериализатору переформатировать его.
fn raw_number<S: Serializer>(text: String, s: S) -> Result<S::Ok, S::Error> {
    let raw = RawValue::from_string(text).map_err(serde::ser::Error::custom)?;
    raw.serialize(s)
}

/// `%.6`, хвостовые нули и точка убраны; «-» от отрицательного нуля остаётся как в v1.
pub fn num6(v: f64) -> String {
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" {
        "0".into()
    } else {
        s.into()
    }
}

/// `%.3`, хвостовые нули и точка убраны.
pub fn num3(v: f64) -> String {
    let s = format!("{v:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// Сериализует значение в одну строку контракта с завершающим переводом строки и приводит
/// экранирование управляющих символов к формату v1.
pub fn contract_json<T: Serialize>(value: &T) -> String {
    let mut text = serde_json::to_string(value).expect("contract structs are always serializable");
    normalize_escapes(&mut text);
    text.push('\n');
    text
}

/// `serde_json` пишет backspace / form feed двумя буквами; контракт v1 — `\u0008` / `\u000c`.
/// Вне строковых литералов замена невозможна, экранирование внутри — только через `\`.
fn normalize_escapes(text: &mut String) {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut start = 0;
    let mut i = 0;
    let mut in_str = false;
    while i < bytes.len() {
        let c = bytes[i];
        if in_str && c == b'\\' && i + 1 < bytes.len() {
            let e = bytes[i + 1];
            if e == b'b' || e == b'f' {
                out.push_str(&text[start..i]);
                out.push_str(if e == b'b' { "\\u0008" } else { "\\u000c" });
                i += 2;
                start = i;
                continue;
            }
            // Экранированный символ (`\"`, `\\`, `\uXXXX`) не переключает строку.
            i += 2;
            continue;
        }
        if c == b'"' {
            in_str = !in_str;
        }
        i += 1;
    }
    out.push_str(&text[start..]);
    *text = out;
}

/// Events of one long listening period, loaded from `stats.json` and extended by the counters of
/// this run.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stats {
    pub since_utc: String,
    pub listening_seconds: f64,
    pub hits: u64,
    pub near: u64,
    pub miss: u64,
    pub dry_run_listening_seconds: f64,
    pub dropped: u64,
}

#[derive(Serialize)]
struct StatsWire<'a> {
    schema: u32,
    since_utc: &'a str,
    listening_seconds: Num6,
    hits: u64,
    near: u64,
    miss: u64,
    dry_run_listening_seconds: Num6,
    dropped: u64,
    model_sha256: &'a str,
}

#[derive(Deserialize)]
struct StatsDoc {
    since_utc: Option<String>,
    listening_seconds: Option<f64>,
    #[serde(default)]
    hits: u64,
    #[serde(default)]
    near: u64,
    #[serde(default)]
    miss: u64,
    #[serde(default)]
    dry_run_listening_seconds: f64,
    #[serde(default)]
    dropped: u64,
}

pub fn stats_json(s: &Stats, model_sha256: &str) -> String {
    contract_json(&StatsWire {
        schema: 1,
        since_utc: &s.since_utc,
        listening_seconds: Num6(s.listening_seconds),
        hits: s.hits,
        near: s.near,
        miss: s.miss,
        dry_run_listening_seconds: Num6(s.dry_run_listening_seconds),
        dropped: s.dropped,
        model_sha256,
    })
}

pub fn parse_stats(text: &str) -> Option<Stats> {
    let doc: StatsDoc = parse(text)?;
    Some(Stats {
        since_utc: doc.since_utc?,
        listening_seconds: doc.listening_seconds?,
        hits: doc.hits,
        near: doc.near,
        miss: doc.miss,
        dry_run_listening_seconds: doc.dry_run_listening_seconds,
        dropped: doc.dropped,
    })
}

/// `<id>.json`, the sidecar of contract v1.
#[derive(Clone, Debug, PartialEq)]
pub struct Sidecar {
    pub schema: u32,
    pub id: String,
    pub kind: String,
    pub utc: String,
    pub sample_rate: u32,
    pub pre_ms: u32,
    pub post_ms: u32,
    pub score_peak: f64,
    pub cutoff: f64,
    pub window: u32,
    pub model_sha256: String,
    pub pulsar_version: String,
    pub dry_run: bool,
    pub outcome: Option<String>,
    pub stt_text: Option<String>,
    pub label: Option<String>,
    pub label_source: Option<String>,
    pub labeled_utc: Option<String>,
}

#[derive(Serialize)]
struct SidecarWire<'a> {
    schema: u32,
    id: &'a str,
    kind: &'a str,
    utc: &'a str,
    sample_rate: u32,
    pre_ms: u32,
    post_ms: u32,
    score_peak: Num3,
    cutoff: Num6,
    window: u32,
    model_sha256: &'a str,
    pulsar_version: &'a str,
    dry_run: bool,
    outcome: Option<&'a str>,
    stt_text: Option<&'a str>,
    label: Option<&'a str>,
    label_source: Option<&'a str>,
    labeled_utc: Option<&'a str>,
}

#[derive(Deserialize)]
struct SidecarDoc {
    #[serde(default = "default_schema")]
    schema: u32,
    id: Option<String>,
    kind: Option<String>,
    utc: Option<String>,
    #[serde(default = "default_sample_rate")]
    sample_rate: u32,
    #[serde(default)]
    pre_ms: u32,
    #[serde(default)]
    post_ms: u32,
    #[serde(default)]
    score_peak: f64,
    #[serde(default)]
    cutoff: f64,
    #[serde(default)]
    window: u32,
    #[serde(default)]
    model_sha256: String,
    #[serde(default)]
    pulsar_version: String,
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    outcome: Option<String>,
    #[serde(default)]
    stt_text: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    label_source: Option<String>,
    #[serde(default)]
    labeled_utc: Option<String>,
}

fn default_schema() -> u32 {
    1
}

fn default_sample_rate() -> u32 {
    crate::logic::SAMPLE_RATE
}

pub fn sidecar_json(s: &Sidecar) -> String {
    contract_json(&SidecarWire {
        schema: s.schema,
        id: &s.id,
        kind: &s.kind,
        utc: &s.utc,
        sample_rate: s.sample_rate,
        pre_ms: s.pre_ms,
        post_ms: s.post_ms,
        score_peak: Num3(s.score_peak),
        cutoff: Num6(s.cutoff),
        window: s.window,
        model_sha256: &s.model_sha256,
        pulsar_version: &s.pulsar_version,
        dry_run: s.dry_run,
        outcome: s.outcome.as_deref(),
        stt_text: s.stt_text.as_deref(),
        label: s.label.as_deref(),
        label_source: s.label_source.as_deref(),
        labeled_utc: s.labeled_utc.as_deref(),
    })
}

pub fn parse_sidecar(text: &str) -> Option<Sidecar> {
    let doc: SidecarDoc = parse(text)?;
    Some(Sidecar {
        schema: doc.schema,
        id: doc.id?,
        kind: doc.kind?,
        utc: doc.utc?,
        sample_rate: doc.sample_rate,
        pre_ms: doc.pre_ms,
        post_ms: doc.post_ms,
        score_peak: doc.score_peak,
        cutoff: doc.cutoff,
        window: doc.window,
        model_sha256: doc.model_sha256,
        pulsar_version: doc.pulsar_version,
        dry_run: doc.dry_run,
        outcome: doc.outcome,
        stt_text: doc.stt_text,
        label: doc.label,
        label_source: doc.label_source,
        labeled_utc: doc.labeled_utc,
    })
}

/// Разбор JSON: None на любую синтаксическую или типовую ошибку (как прежний парсер).
fn parse<T: serde::de::DeserializeOwned>(text: &str) -> Option<T> {
    serde_json::from_str(text).ok()
}

#[derive(Deserialize)]
struct ManifestDoc {
    micro: MicroDoc,
}

#[derive(Deserialize)]
struct MicroDoc {
    probability_cutoff: f64,
    sliding_window_size: u64,
}

/// `micro.probability_cutoff` and `micro.sliding_window_size` of the model manifest.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Thresholds {
    pub cutoff: f32,
    pub window: usize,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            cutoff: crate::logic::DEFAULT_CUTOFF as f32,
            window: crate::logic::DEFAULT_WINDOW,
        }
    }
}

/// The manifest next to the loaded `.tflite` (same stem, `.json`); None when it is missing or its
/// values are outside the contract's ranges, so the loaded defaults stay.
pub fn manifest_thresholds(text: &str) -> Option<Thresholds> {
    let doc: ManifestDoc = parse(text)?;
    let cutoff = doc.micro.probability_cutoff;
    let window = doc.micro.sliding_window_size;
    ((cutoff > 0.0 && cutoff < 1.0) && (1..=50).contains(&window)).then_some(Thresholds {
        cutoff: cutoff as f32,
        window: window as usize,
    })
}

/// Reads the thresholds for `model`; the bool tells whether the manifest was used.
pub fn thresholds_for(model: &std::path::Path) -> (Thresholds, bool) {
    let path = model.with_extension("json");
    std::fs::read_to_string(&path).map_or_else(
        |_| (Thresholds::default(), false),
        |text| {
            manifest_thresholds(&text).map_or_else(|| (Thresholds::default(), false), |t| (t, true))
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_match_the_contract_format() {
        assert_eq!(num6(0.0), "0");
        assert_eq!(num6(12345.6), "12345.6");
        assert_eq!(num6(1.23456789), "1.234568");
        assert_eq!(num6(0.000001), "0.000001");
        assert_eq!(num6(-0.0), "-0");
        assert_eq!(num3(0.934), "0.934");
        assert_eq!(num3(0.9), "0.9");
        assert_eq!(num3(0.0004), "0");
    }

    #[test]
    fn escapes_are_normalized_inside_strings_only() {
        assert_eq!(num6(1.0), "1");
        let text = contract_json(&serde_json::json!({"a": "\u{8}\u{c}\"x"}));
        assert_eq!(text, "{\"a\":\"\\u0008\\u000c\\\"x\"}\n");
    }

    #[test]
    fn stats_and_sidecars_round_trip() {
        let stats = Stats {
            since_utc: "2026-11-01T10:00:00.000Z".into(),
            listening_seconds: 12345.6,
            hits: 10,
            near: 40,
            miss: 2,
            dry_run_listening_seconds: 3600.0,
            dropped: 0,
        };
        let text = stats_json(&stats, "abc");
        assert_eq!(parse_stats(&text), Some(stats));
        assert!(text.ends_with("}\n"));

        let side = Sidecar {
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
        };
        let text = sidecar_json(&side);
        assert_eq!(parse_sidecar(&text), Some(side.clone()));
        assert!(text.contains("\"outcome\":null"));
        assert!(text.contains("\"score_peak\":0.934"));
        let labeled = Sidecar {
            outcome: Some("answered".into()),
            stt_text: Some(String::new()),
            label: Some("false".into()),
            label_source: Some("user".into()),
            labeled_utc: Some("2026-11-01T10:20:00.000Z".into()),
            ..side
        };
        assert_eq!(parse_sidecar(&sidecar_json(&labeled)), Some(labeled));
    }

    #[test]
    fn manifest_values_are_validated() {
        let ok = r#"{"micro":{"probability_cutoff":0.87,"sliding_window_size":12}}"#;
        assert_eq!(
            manifest_thresholds(ok),
            Some(Thresholds {
                cutoff: 0.87,
                window: 12
            })
        );
        for bad in [
            r#"{"micro":{"probability_cutoff":0,"sliding_window_size":12}}"#,
            r#"{"micro":{"probability_cutoff":1,"sliding_window_size":12}}"#,
            r#"{"micro":{"probability_cutoff":0.9,"sliding_window_size":0}}"#,
            r#"{"micro":{"probability_cutoff":0.9,"sliding_window_size":51}}"#,
            r#"{"micro":{"probability_cutoff":0.9}}"#,
            r#"{"other":{}}"#,
            "not json",
        ] {
            assert_eq!(manifest_thresholds(bad), None, "{bad}");
        }
        assert_eq!(Thresholds::default().window, 10);
    }

    #[test]
    fn malformed_text_is_rejected() {
        for text in ["", "{", "{\"a\":}", "[1,]", "{\"a\" 1}", "tru"] {
            assert!(parse_stats(text).is_none(), "{text}");
            assert!(parse_sidecar(text).is_none(), "{text}");
        }
    }
}
