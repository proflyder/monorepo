//! Pure decisions of wake-event recording: ids and UTC text, retention, the near-miss peak and
//! window sizes. No I/O here, so every rule of the contract is unit-tested next to it.
//!
//! Контракт JSON, его чтение и запись — в [`crate::contract`]; в этом модуле только то, что
//! решается без файлов.

use std::path::Path;

pub use crate::contract::{
    Sidecar, Stats, Thresholds, parse_sidecar, parse_stats, sidecar_json, stats_json,
};

pub const SAMPLE_RATE: u32 = 16000;
pub const DEFAULT_CUTOFF: f64 = 0.9;
pub const DEFAULT_WINDOW: usize = 10;
/// Cheap near-miss band: a local maximum of the windowed mean below the cutoff counts from here.
pub const NEAR_THRESHOLD: f32 = 0.5;
/// Two near-misses are at least this far apart.
pub const NEAR_GAP_MS: i64 = 3000;
/// No near-miss this soon after a confirmed hit.
pub const NEAR_AFTER_HIT_MS: i64 = 2000;
/// «Wake: ложное срабатывание» labels the newest hit of this window.
pub const FALSE_ALARM_WINDOW_MS: i64 = 10 * 60 * 1000;
/// Retention caps: 300 MB or 3000 events.
pub const MAX_BYTES: u64 = 300 * 1024 * 1024;
pub const MAX_EVENTS: usize = 3000;

/// Event kinds of contract v1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Hit,
    Near,
    Miss,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hit => "hit",
            Self::Near => "near",
            Self::Miss => "miss",
        }
    }
}

/// Outcome of a hit's dialog, filled by the controller when the dialog ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Answered,
    EmptyStt,
    Cancelled,
    Error,
    DryRun,
    ArbiterLost,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Answered => "answered",
            Self::EmptyStt => "empty_stt",
            Self::Cancelled => "cancelled",
            Self::Error => "error",
            Self::DryRun => "dry_run",
            Self::ArbiterLost => "arbiter_lost",
        }
    }
}

/// Milliseconds since the Unix epoch (wall clock, for ids and sidecars; the audio path uses a
/// monotonic clock for its intervals).
pub fn utc_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// `YYYY-MM-DDTHH:MM:SS.mmmZ`.
pub fn format_utc(ms: i64) -> String {
    let fmt = time::macros::format_description!(
        "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
    );
    instant(ms)
        .and_then(|t| t.format(&fmt).ok())
        .unwrap_or_default()
}

/// `YYYYMMDDTHHMMSSZ`, the id prefix.
pub fn format_id_time(ms: i64) -> String {
    let fmt = time::macros::format_description!("[year][month][day]T[hour][minute][second]Z");
    instant(ms)
        .and_then(|t| t.format(&fmt).ok())
        .unwrap_or_default()
}

/// `instant` возвращает UTC-время контракта; отрицательные значения (до эпохи) клипаются к ней,
/// как в прежнем гражданском календаре.
fn instant(ms: i64) -> Option<time::OffsetDateTime> {
    time::OffsetDateTime::from_unix_timestamp_nanos(i128::from(ms.max(0)) * 1_000_000).ok()
}

/// `<UTC yyyymmddTHHMMSSZ>-<kind>-<seq>`, `seq` a per-boot counter.
pub fn event_id(ms: i64, kind: Kind, seq: u64) -> String {
    format!("{}-{}-{seq}", format_id_time(ms), kind.as_str())
}

/// Parses `format_utc` back; used for retention order and the false-alarm window.
pub fn parse_utc(text: &str) -> Option<i64> {
    const WITH_MS: &[time::format_description::FormatItem<'static>] = time::macros::format_description!(
        "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
    );
    const WITHOUT_MS: &[time::format_description::FormatItem<'static>] =
        time::macros::format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]Z");
    time::PrimitiveDateTime::parse(text, &WITH_MS)
        .or_else(|_| time::PrimitiveDateTime::parse(text, &WITHOUT_MS))
        .ok()
        .map(|t| (t.assume_utc().unix_timestamp_nanos() / 1_000_000) as i64)
}

/// One WAV chunk of PCM16 mono 16 kHz: a 44-byte canonical header and the samples.
pub fn wav(samples: &[i16]) -> Vec<u8> {
    pulsar_dsp::wav::pcm16(samples, 1, SAMPLE_RATE)
}

/// What retention and the Ha buttons need from a sidecar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Meta {
    pub id: String,
    pub kind: String,
    pub utc_ms: i64,
    pub labeled: bool,
    /// WAV + sidecar size.
    pub bytes: u64,
}

/// Indices to delete, oldest first: unlabeled `near`, then unlabeled `hit`, then anything. Stops
/// as soon as both caps hold.
pub fn retention_victims(
    events: &[Meta],
    total_bytes: u64,
    max_bytes: u64,
    max_events: usize,
) -> Vec<usize> {
    let mut order: Vec<usize> = (0..events.len()).collect();
    order.sort_by(|&a, &b| {
        events[a]
            .utc_ms
            .cmp(&events[b].utc_ms)
            .then_with(|| events[a].id.cmp(&events[b].id))
    });
    let mut removed = vec![false; events.len()];
    let (mut count, mut bytes) = (events.len(), total_bytes);
    let mut victims = Vec::new();
    while count > max_events || bytes > max_bytes {
        let pick = order
            .iter()
            .copied()
            .find(|&i| !removed[i] && events[i].kind == "near" && !events[i].labeled)
            .or_else(|| {
                order
                    .iter()
                    .copied()
                    .find(|&i| !removed[i] && events[i].kind == "hit" && !events[i].labeled)
            })
            .or_else(|| order.iter().copied().find(|&i| !removed[i]));
        let Some(i) = pick else { break };
        removed[i] = true;
        count -= 1;
        bytes = bytes.saturating_sub(events[i].bytes);
        victims.push(i);
    }
    victims
}

/// Index of the newest hit within `FALSE_ALARM_WINDOW_MS` of `now_ms`.
pub fn false_alarm_target(events: &[Meta], now_ms: i64) -> Option<usize> {
    events
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            e.kind == "hit" && now_ms.saturating_sub(e.utc_ms) <= FALSE_ALARM_WINDOW_MS
        })
        .max_by(|(_, a), (_, b)| a.utc_ms.cmp(&b.utc_ms).then_with(|| a.id.cmp(&b.id)))
        .map(|(i, _)| i)
}

/// A local maximum of the windowed mean that stays below the cutoff: `prev` is above what came
/// before it and is not below the current chunk.
pub fn near_peak(prev2: Option<f32>, prev: f32, cur: f32, cutoff: f32) -> bool {
    let before = prev2.unwrap_or(-1.0);
    prev > before && prev >= cur && prev >= NEAR_THRESHOLD && prev < cutoff
}

/// Whole VQE chunks covering at least `ms` of 16 kHz audio.
pub fn chunks_for(ms: u64, frame: usize) -> usize {
    let samples = (u64::from(SAMPLE_RATE) * ms / 1000) as usize;
    samples.div_ceil(frame.max(1))
}

/// Манифест рядом с загруженной моделью; формат — [`crate::contract`].
pub fn thresholds_for(model: &Path) -> (Thresholds, bool) {
    crate::contract::thresholds_for(model)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_text_round_trips() {
        // 2026-11-01T10:15:00.123Z
        let ms = parse_utc("2026-11-01T10:15:00.123Z").unwrap();
        assert_eq!(format_utc(ms), "2026-11-01T10:15:00.123Z");
        assert_eq!(format_id_time(ms), "20261101T101500Z");
        assert_eq!(event_id(ms, Kind::Hit, 17), "20261101T101500Z-hit-17");
        assert_eq!(parse_utc("1970-01-01T00:00:00.000Z"), Some(0));
        assert_eq!(
            parse_utc("2024-02-29T23:59:59.999Z")
                .map(format_utc)
                .as_deref(),
            Some("2024-02-29T23:59:59.999Z")
        );
        // Without milliseconds the same instant; malformed text is rejected.
        assert_eq!(parse_utc("2026-11-01T10:15:00Z"), Some(ms / 1000 * 1000));
        for bad in ["", "2026-11-01", "x026-11-01T10:15:00.123Z"] {
            assert_eq!(parse_utc(bad), None, "{bad}");
        }
        assert_eq!(format_id_time(ms).len(), 16);
    }

    #[test]
    fn wav_header_is_canonical_pcm16_mono_16k() {
        let w = wav(&[0, 1, -1, i16::MAX]);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(&w[36..40], b"data");
        assert_eq!(u32::from_le_bytes(w[40..44].try_into().unwrap()), 8);
        assert_eq!(&w[44..], &[0, 0, 1, 0, 0xff, 0xff, 0xff, 0x7f]);
    }

    fn meta(id: &str, kind: &str, utc_ms: i64, labeled: bool, bytes: u64) -> Meta {
        Meta {
            id: id.into(),
            kind: kind.into(),
            utc_ms,
            labeled,
            bytes,
        }
    }

    #[test]
    fn retention_prefers_unlabeled_near_then_hit() {
        let e = vec![
            meta("a", "hit", 1, false, 100),
            meta("b", "near", 2, false, 100),
            meta("c", "near", 3, true, 100),
            meta("d", "hit", 4, false, 100),
            meta("e", "miss", 5, true, 100),
        ];
        // Two over: the oldest unlabeled near and the next one (labeled near comes later).
        assert_eq!(retention_victims(&e, 500, 300, 10), vec![1, 0]);
        // Only the count is over: oldest unlabeled near / hit first, then anything.
        assert_eq!(retention_victims(&e, 500, 10_000, 3), vec![1, 0]);
        // Nothing over.
        assert!(retention_victims(&e, 500, 500, 5).is_empty());
        // All labeled: oldest overall first.
        let all = vec![
            meta("x", "miss", 1, true, 10),
            meta("y", "hit", 2, true, 10),
        ];
        assert_eq!(retention_victims(&all, 20, 10, 10), vec![0]);
    }

    #[test]
    fn false_alarm_is_the_newest_recent_hit() {
        let now = 1_000_000_000;
        let e = vec![
            meta("old", "hit", now - FALSE_ALARM_WINDOW_MS - 1, false, 1),
            meta("h1", "hit", now - 60_000, false, 1),
            meta("h2", "hit", now - 1000, false, 1),
            meta("n", "near", now - 500, false, 1),
        ];
        assert_eq!(false_alarm_target(&e, now), Some(2));
        assert_eq!(false_alarm_target(&e[..1], now), None);
        assert_eq!(false_alarm_target(&[], now), None);
    }

    #[test]
    fn near_peak_needs_a_fall_below_the_window() {
        // Rise, peak 0.7, fall: a near-miss.
        assert!(near_peak(Some(0.4), 0.7, 0.6, 0.9));
        // Still rising or flat: not yet a peak.
        assert!(!near_peak(Some(0.4), 0.6, 0.7, 0.9));
        assert!(near_peak(Some(0.4), 0.7, 0.7, 0.9));
        // Below the near band, at or above the cutoff: no near event.
        assert!(!near_peak(Some(0.1), 0.4, 0.2, 0.9));
        assert!(!near_peak(Some(0.5), 0.95, 0.3, 0.9));
        // First windowed chunk: nothing before it, so not a local maximum.
        assert!(near_peak(None, 0.7, 0.6, 0.9));
    }

    #[test]
    fn window_chunks_cover_at_least_the_window() {
        assert_eq!(chunks_for(3000, 512), 94); // 3008 ms
        assert_eq!(chunks_for(1000, 512), 32); // 1024 ms
        assert_eq!(chunks_for(6000, 512), 188); // 6016 ms
        assert_eq!(chunks_for(3000, 256), 188); // 3008 ms, Mini 3 / Mini chunks
    }
}
