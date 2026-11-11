//! Продакшен-адаптеры портов: Ricardo и запись wake-событий.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::mpsc::SyncSender;

use tracing::{info, warn};

use crate::ricardo::{DetectorError, RicardoWake};
use crate::wake_events::{Capture as WakeCapture, Counters, Job, Recording, WakeInput, logic};

use super::ports::{Detection, Detector, WakeStore};

/// Рикардо за портом [`Detector`]: модель и порог из манифеста, владение FFI-объектом остаётся
/// у одного потока обработки.
pub(super) struct RicardoDetector(RicardoWake);

impl RicardoDetector {
    pub(super) fn load(path: &Path, thresholds: logic::Thresholds) -> Result<Self, DetectorError> {
        let mut wake = RicardoWake::new(path)?;
        wake.configure(thresholds.cutoff, thresholds.window);
        info!("Ricardo wake word: {}", path.display());
        Ok(Self(wake))
    }
}

impl Detector for RicardoDetector {
    fn feed(&mut self, pcm: &[i16]) -> Result<Detection, DetectorError> {
        let (hit, score) = self.0.feed(pcm)?;
        Ok(Detection { hit, score })
    }
}

/// Продакшен-порт записи: кольцо wake-событий на потоке обработки и канал writer'а.
pub(super) struct RecordingStore {
    capture: WakeCapture,
    jobs: SyncSender<Job>,
    counters: Arc<Counters>,
}

impl RecordingStore {
    pub(super) fn new(input: WakeInput, frame: usize) -> Self {
        Self {
            capture: WakeCapture::new(frame, input.thresholds.cutoff),
            jobs: input.jobs,
            counters: input.counters,
        }
    }

    /// Hands a finished window to the writer; a full queue drops it and counts the drop.
    fn send(&self, rec: Recording) {
        if self.jobs.try_send(Job::Save(Box::new(rec))).is_err() {
            self.counters.dropped.fetch_add(1, Relaxed);
            warn!("wake events: writer is behind, window dropped");
        }
    }
}

impl WakeStore for RecordingStore {
    fn listening(&mut self, chunk_ms: u64, dry_run: bool) {
        self.counters.listening_ms.fetch_add(chunk_ms, Relaxed);
        if dry_run {
            self.counters.dry_run_ms.fetch_add(chunk_ms, Relaxed);
        }
    }

    fn missed(&mut self, utc_ms: i64) {
        if let Some(rec) = self.capture.miss(utc_ms) {
            self.counters.miss.fetch_add(1, Relaxed);
            pulsar_metrics::wake_miss();
            self.send(rec);
        }
    }

    fn push(
        &mut self,
        chunk: &[i16],
        mean: Option<f32>,
        hit: Option<f32>,
        dry_run: bool,
        now_ms: i64,
        utc_ms: i64,
    ) -> Option<String> {
        let p = self.capture.push(chunk, mean, hit, dry_run, now_ms, utc_ms);
        if p.hit.is_some() {
            self.counters.hits.fetch_add(1, Relaxed);
        }
        if p.near {
            self.counters.near.fetch_add(1, Relaxed);
            pulsar_metrics::wake_near();
        }
        if p.dropped {
            self.counters.dropped.fetch_add(1, Relaxed);
        }
        for rec in p.finished {
            self.send(rec);
        }
        p.hit.map(|h| h.id)
    }

    fn reset(&mut self) {
        self.capture.reset();
    }
}
