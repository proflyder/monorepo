//! Atomically updated metric state. Every recorder only does relaxed atomic
//! loads/stores, so the audio processing thread and the call loop never block or
//! allocate here.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::system::SystemSample;

/// `f64` gauge over an `AtomicU64` bit pattern.
#[derive(Debug)]
pub struct Gauge(AtomicU64);

impl Gauge {
    #[must_use]
    pub const fn new() -> Self {
        Self(AtomicU64::new(0))
    }

    pub fn set(&self, value: f64) {
        self.0.store(value.to_bits(), Ordering::Relaxed);
    }

    #[must_use]
    pub fn get(&self) -> f64 {
        f64::from_bits(self.0.load(Ordering::Relaxed))
    }
}

impl Default for Gauge {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Default)]
pub struct Counter(AtomicU64);

impl Counter {
    #[must_use]
    pub const fn new() -> Self {
        Self(AtomicU64::new(0))
    }

    pub fn inc(&self) {
        self.add(1);
    }

    pub fn add(&self, n: u64) {
        self.0.fetch_add(n, Ordering::Relaxed);
    }

    #[must_use]
    pub fn get(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

/// Upper bounds of one histogram; the `+Inf` bucket is implicit.
pub const HISTOGRAM_SLOTS: usize = 16;

/// Cumulative histogram with fixed bounds. At most [`HISTOGRAM_SLOTS`] bounds;
/// the constructors below fit.
#[derive(Debug)]
pub struct Histogram {
    bounds: &'static [f64],
    buckets: [AtomicU64; HISTOGRAM_SLOTS],
    sum: AtomicU64,
    count: AtomicU64,
}

impl Histogram {
    #[must_use]
    pub const fn new(bounds: &'static [f64]) -> Self {
        Self {
            bounds,
            buckets: [const { AtomicU64::new(0) }; HISTOGRAM_SLOTS],
            sum: AtomicU64::new(0),
            count: AtomicU64::new(0),
        }
    }

    pub fn observe(&self, value: f64) {
        if !value.is_finite() || value < 0.0 {
            return;
        }
        let slot = self
            .bounds
            .iter()
            .position(|&bound| value <= bound)
            .unwrap_or(self.bounds.len());
        if let Some(bucket) = self.buckets.get(slot) {
            bucket.fetch_add(1, Ordering::Relaxed);
        }
        add_f64(&self.sum, value);
        self.count.fetch_add(1, Ordering::Relaxed);
    }

    /// Cumulative counts per bucket, then `+Inf`.
    #[must_use]
    pub fn buckets(&self) -> Vec<(f64, u64)> {
        let mut running = 0;
        let mut out = Vec::with_capacity(self.bounds.len() + 1);
        for (i, &bound) in self.bounds.iter().enumerate() {
            running += self.buckets[i].load(Ordering::Relaxed);
            out.push((bound, running));
        }
        running += self.buckets[self.bounds.len()].load(Ordering::Relaxed);
        out.push((f64::INFINITY, running));
        out
    }

    #[must_use]
    pub fn sum(&self) -> f64 {
        f64::from_bits(self.sum.load(Ordering::Relaxed))
    }

    #[must_use]
    pub fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }
}

/// Atomic `f64` addition through a compare-exchange loop.
fn add_f64(atomic: &AtomicU64, value: f64) {
    let mut old = atomic.load(Ordering::Relaxed);
    loop {
        let new = f64::from_bits(old) + value;
        match atomic.compare_exchange_weak(old, new.to_bits(), Ordering::Relaxed, Ordering::Relaxed)
        {
            Ok(_) => return,
            Err(current) => old = current,
        }
    }
}

const ASSISTANT_BOUNDS: [f64; 14] = [
    0.1, 0.2, 0.3, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0, 5.0, 7.5, 10.0, 15.0, 30.0,
];
const CALL_BOUNDS: [f64; 11] = [
    5.0, 10.0, 20.0, 30.0, 60.0, 120.0, 300.0, 600.0, 1200.0, 1800.0, 3600.0,
];
const SCORE_BOUNDS: [f64; 10] = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 0.99];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PipelineResult {
    Ok,
    Error,
    Timeout,
}

impl PipelineResult {
    pub(crate) const fn index(self) -> usize {
        match self {
            Self::Ok => 0,
            Self::Error => 1,
            Self::Timeout => 2,
        }
    }

    pub(crate) const LABELS: [&'static str; 3] = ["ok", "error", "timeout"];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackKind {
    Tts,
    Announce,
    MediaAnnounce,
    Earcon,
    Ring,
}

impl PlaybackKind {
    pub(crate) const fn index(self) -> usize {
        match self {
            Self::Tts => 0,
            Self::Announce => 1,
            Self::MediaAnnounce => 2,
            Self::Earcon => 3,
            Self::Ring => 4,
        }
    }

    pub(crate) const LABELS: [&'static str; 5] =
        ["tts", "announce", "media_announce", "earcon", "ring"];
}

/// Fixed registry of the daemon's metrics.
#[derive(Debug)]
pub struct Metrics {
    pub(crate) info: OnceLock<(String, String)>,
    pub(crate) wake_model: OnceLock<(String, String)>,
    pub(crate) ha_connected: Gauge,
    pub(crate) ha_reconnects: Counter,
    pub(crate) pipeline: [Counter; 3],
    pub(crate) wake_to_stt: Histogram,
    pub(crate) wake_to_answer: Histogram,
    pub(crate) playback_started: [Counter; 5],
    pub(crate) playback_finished: [[Counter; 2]; 5],
    pub(crate) alsa_xruns: Counter,
    pub(crate) mute: [Gauge; 2],
    pub(crate) volume: Gauge,
    pub(crate) call_active: Gauge,
    pub(crate) call_incoming: Counter,
    pub(crate) call_ended: Counter,
    pub(crate) call_duration: Histogram,
    pub(crate) call_rx_packets: Counter,
    pub(crate) call_packets_lost: Counter,
    pub(crate) call_fec: Counter,
    pub(crate) call_plc: Counter,
    pub(crate) call_decode_errors: Counter,
    pub(crate) call_underrun: Counter,
    pub(crate) call_overflow: Counter,
    pub(crate) wake_detections: Counter,
    pub(crate) wake_near: Counter,
    pub(crate) wake_miss: Counter,
    pub(crate) wake_labels: [Counter; 2],
    pub(crate) wake_score: Histogram,
    pub(crate) wake_threshold: Gauge,
}

impl Metrics {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            info: OnceLock::new(),
            wake_model: OnceLock::new(),
            ha_connected: Gauge::new(),
            ha_reconnects: Counter::new(),
            pipeline: [const { Counter::new() }; 3],
            wake_to_stt: Histogram::new(&ASSISTANT_BOUNDS),
            wake_to_answer: Histogram::new(&ASSISTANT_BOUNDS),
            playback_started: [const { Counter::new() }; 5],
            playback_finished: [
                [const { Counter::new() }; 2],
                [const { Counter::new() }; 2],
                [const { Counter::new() }; 2],
                [const { Counter::new() }; 2],
                [const { Counter::new() }; 2],
            ],
            alsa_xruns: Counter::new(),
            mute: [const { Gauge::new() }; 2],
            volume: Gauge::new(),
            call_active: Gauge::new(),
            call_incoming: Counter::new(),
            call_ended: Counter::new(),
            call_duration: Histogram::new(&CALL_BOUNDS),
            call_rx_packets: Counter::new(),
            call_packets_lost: Counter::new(),
            call_fec: Counter::new(),
            call_plc: Counter::new(),
            call_decode_errors: Counter::new(),
            call_underrun: Counter::new(),
            call_overflow: Counter::new(),
            wake_detections: Counter::new(),
            wake_near: Counter::new(),
            wake_miss: Counter::new(),
            wake_labels: [const { Counter::new() }; 2],
            wake_score: Histogram::new(&SCORE_BOUNDS),
            wake_threshold: Gauge::new(),
        }
    }

    /// Version and model labels of `pulsar_info`; the first call wins.
    pub fn set_info(&self, version: &str, model: &str) {
        let _ = self.info.set((version.to_owned(), model.to_owned()));
    }

    /// `pulsar_wake_model_info`: loaded detector file and daemon version.
    pub fn set_wake_model(&self, model: &str, version: &str) {
        let _ = self.wake_model.set((model.to_owned(), version.to_owned()));
    }

    pub fn set_ha_connected(&self, connected: bool) {
        self.ha_connected.set(if connected { 1.0 } else { 0.0 });
    }

    pub fn ha_reconnect(&self) {
        self.ha_reconnects.inc();
    }

    pub fn pipeline_finished(&self, result: PipelineResult) {
        self.pipeline[result.index()].inc();
    }

    pub fn observe_wake_to_stt(&self, seconds: f64) {
        self.wake_to_stt.observe(seconds);
    }

    pub fn observe_wake_to_answer(&self, seconds: f64) {
        self.wake_to_answer.observe(seconds);
    }

    pub fn playback_started(&self, kind: PlaybackKind) {
        self.playback_started[kind.index()].inc();
    }

    pub fn playback_finished(&self, kind: PlaybackKind, ok: bool) {
        self.playback_finished[kind.index()][usize::from(!ok)].inc();
    }

    pub fn alsa_xrun(&self) {
        self.alsa_xruns.inc();
    }

    /// `kind`: 0 — hardware mute switch, 1 — mute from Home Assistant.
    pub fn set_mute(&self, hardware: bool, software: bool) {
        self.mute[0].set(if hardware { 1.0 } else { 0.0 });
        self.mute[1].set(if software { 1.0 } else { 0.0 });
    }

    pub fn set_volume(&self, ratio: f64) {
        self.volume.set(ratio);
    }

    pub fn set_call_active(&self, active: bool) {
        self.call_active.set(if active { 1.0 } else { 0.0 });
    }

    pub fn call_incoming(&self) {
        self.call_incoming.inc();
    }

    pub fn call_ended(&self, seconds: f64) {
        self.call_ended.inc();
        self.call_duration.observe(seconds);
    }

    pub fn call_rx_packet(&self) {
        self.call_rx_packets.inc();
    }

    pub fn call_packet_lost(&self, n: u64) {
        self.call_packets_lost.add(n);
    }

    pub fn call_fec(&self) {
        self.call_fec.inc();
    }

    pub fn call_plc(&self) {
        self.call_plc.inc();
    }

    pub fn call_decode_error(&self) {
        self.call_decode_errors.inc();
    }

    pub fn call_underrun(&self) {
        self.call_underrun.inc();
    }

    pub fn call_overflow(&self) {
        self.call_overflow.inc();
    }

    pub fn wake_detected(&self) {
        self.wake_detections.inc();
    }

    pub fn wake_near(&self) {
        self.wake_near.inc();
    }

    pub fn wake_miss(&self) {
        self.wake_miss.inc();
    }

    /// `false` — «ложное срабатывание», `missed` — «пропуск» из обратной связи.
    pub fn wake_label(&self, false_alarm: bool) {
        self.wake_labels[usize::from(!false_alarm)].inc();
    }

    pub fn observe_wake_score(&self, score: f64) {
        self.wake_score.observe(score);
    }

    pub fn set_wake_threshold(&self, threshold: f64) {
        self.wake_threshold.set(threshold);
    }

    pub(crate) fn render(&self, sample: &SystemSample) -> String {
        crate::render::render(self, sample)
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}
