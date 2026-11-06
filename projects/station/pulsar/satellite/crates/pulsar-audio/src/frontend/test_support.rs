//! Скриптовые порты и хелперы тестов фронтенда.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering::Relaxed;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};

use pulsar_alsa::{AlsaError, HwStep};
use pulsar_dsp::endpoint::EndpointPrior;
use pulsar_model::{Capture as CaptureLayout, Preroll, profile_from_compatible};
use tokio::sync::mpsc::unbounded_channel;

use crate::engine::{Spotted, VQE_OMNI, VQE_OMNI_WITH_GAIN, WakeEngine};
use crate::ricardo::DetectorError;

use super::capture::CaptureError;
use super::cmd::FrontendCmd;
use super::ports::{CaptureSource, Clock, Detection, Detector, WakeStore};
use super::processing::{Processing, ProcessingConfig};
use super::{FrontendEvent, FrontendStats, lock};

/// One chunk a scripted engine answers for: the two VQE outputs and process success.
pub(super) struct Step {
    gain: Vec<f32>,
    omni: Vec<f32>,
    ok: bool,
}

impl Step {
    pub(super) fn gain(frame: usize, amp: f32) -> Self {
        Self {
            gain: vec![amp; frame],
            omni: vec![amp; frame],
            ok: true,
        }
    }
}

pub(super) struct ScriptedEngine {
    frame: usize,
    chunk: usize,
    steps: VecDeque<Step>,
    gain: Vec<f32>,
    omni: Vec<f32>,
    ok: bool,
    spotted: VecDeque<Spotted>,
    commands: VecDeque<Vec<String>>,
    wake_id: String,
    models: Vec<PathBuf>,
}

impl ScriptedEngine {
    pub(super) fn new(steps: Vec<Step>) -> Self {
        Self::with_sizes(512, 512, steps)
    }

    /// Движок с размерами VQE конкретной модели: `chunk` кадров захвата → `frame` выходных.
    pub(super) fn with_sizes(chunk: usize, frame: usize, steps: Vec<Step>) -> Self {
        Self {
            frame,
            chunk,
            steps: steps.into(),
            gain: vec![0.0; frame],
            omni: vec![0.0; frame],
            ok: true,
            spotted: VecDeque::new(),
            commands: VecDeque::new(),
            wake_id: "alisa".into(),
            models: Vec::new(),
        }
    }
}

impl WakeEngine for ScriptedEngine {
    fn chunk(&self) -> usize {
        self.chunk
    }

    fn frame(&self) -> usize {
        self.frame
    }

    fn process(&mut self, _mics: &[f32], _spk: &[f32]) -> bool {
        if let Some(s) = self.steps.pop_front() {
            self.gain = s.gain;
            self.omni = s.omni;
            self.ok = s.ok;
        }
        self.ok
    }

    fn output(&self, ty: i32) -> Option<&[f32]> {
        match ty {
            VQE_OMNI => Some(&self.omni),
            VQE_OMNI_WITH_GAIN => Some(&self.gain),
            _ => None,
        }
    }

    fn feed_wake(&mut self) -> Spotted {
        self.spotted.pop_front().unwrap_or_default()
    }

    fn feed_command(&mut self) -> Vec<String> {
        self.commands.pop_front().unwrap_or_default()
    }

    fn set_wake_model(&mut self, dir: &Path) -> bool {
        self.models.push(dir.to_owned());
        self.wake_id = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        true
    }

    fn wake_id(&self) -> &str {
        &self.wake_id
    }
}

pub(super) struct ScriptedDetector {
    results: VecDeque<Result<Detection, DetectorError>>,
    fed: usize,
}

impl ScriptedDetector {
    pub(super) fn new(results: Vec<Result<Detection, DetectorError>>) -> Self {
        Self {
            results: results.into(),
            fed: 0,
        }
    }
}

impl Detector for ScriptedDetector {
    fn feed(&mut self, _pcm: &[i16]) -> Result<Detection, DetectorError> {
        self.fed += 1;
        self.results.pop_front().unwrap_or(Ok(Detection {
            hit: false,
            score: -1.0,
        }))
    }
}

#[derive(Default)]
pub(super) struct StoreLog {
    pub(super) misses: usize,
    pub(super) listens: Vec<(u64, bool)>,
    pub(super) pushes: Vec<(Option<f32>, Option<f32>, bool)>,
    pub(super) resets: usize,
    pub(super) hit: Option<String>,
}

pub(super) struct TestStore {
    log: Arc<Mutex<StoreLog>>,
}

impl WakeStore for TestStore {
    fn listening(&mut self, chunk_ms: u64, dry_run: bool) {
        lock(&self.log).listens.push((chunk_ms, dry_run));
    }

    fn missed(&mut self, _utc_ms: i64) {
        lock(&self.log).misses += 1;
    }

    fn push(
        &mut self,
        _chunk: &[i16],
        mean: Option<f32>,
        hit: Option<f32>,
        dry_run: bool,
        _now_ms: i64,
        _utc_ms: i64,
    ) -> Option<String> {
        let mut log = lock(&self.log);
        log.pushes.push((mean, hit, dry_run));
        log.hit.clone()
    }

    fn reset(&mut self) {
        lock(&self.log).resets += 1;
    }
}

pub(super) struct NullStore;

impl WakeStore for NullStore {
    fn listening(&mut self, _chunk_ms: u64, _dry_run: bool) {}
    fn missed(&mut self, _utc_ms: i64) {}
    fn push(
        &mut self,
        _chunk: &[i16],
        _mean: Option<f32>,
        _hit: Option<f32>,
        _dry_run: bool,
        _now_ms: i64,
        _utc_ms: i64,
    ) -> Option<String> {
        None
    }
    fn reset(&mut self) {}
}

#[derive(Clone)]
pub(super) struct FixedClock {
    mono: Arc<std::sync::atomic::AtomicI64>,
    utc: i64,
}

impl FixedClock {
    pub(super) fn new() -> Self {
        Self {
            mono: Arc::new(std::sync::atomic::AtomicI64::new(0)),
            utc: 0,
        }
    }
}

impl Clock for FixedClock {
    fn mono_ms(&self) -> i64 {
        self.mono.load(Relaxed)
    }

    fn utc_ms(&self) -> i64 {
        self.utc
    }
}

pub(super) struct Harness<E: WakeEngine, D: Detector, S: WakeStore, C: Clock> {
    pub(super) processing: Processing<E, D, S, C>,
    pub(super) cmd: Sender<FrontendCmd>,
}

pub(super) fn harness_with<E: WakeEngine, D: Detector, S: WakeStore, C: Clock>(
    capture: &'static CaptureLayout,
    preroll: Option<&'static Preroll>,
    engine: E,
    detector: Option<D>,
    store: Option<S>,
    clock: C,
    custom: bool,
) -> Harness<E, D, S, C> {
    let (cmd, cmd_rx) = mpsc::channel();
    let (tx, _rx) = unbounded_channel();
    let cfg = ProcessingConfig {
        capture,
        preroll,
        cmd_rx,
        tx,
        stats: Arc::new(Mutex::new(FrontendStats::default())),
        custom,
    };
    Harness {
        processing: Processing::new(cfg, engine, detector, store, clock),
        cmd,
    }
}

pub(super) fn midi() -> &'static CaptureLayout {
    &profile_from_compatible(b"yandex,cucumber\0")
        .unwrap()
        .audio
        .capture
}

/// Interleaved capture block with every channel equal to `sample`.
pub(super) fn block(capture: &CaptureLayout, frames: usize, sample: i16) -> Vec<i16> {
    vec![sample; frames * capture.channels as usize]
}

pub(super) fn process<E: WakeEngine, D: Detector, S: WakeStore, C: Clock>(
    h: &mut Harness<E, D, S, C>,
    sample: i16,
) -> Vec<FrontendEvent> {
    let input = block(midi(), 512, sample);
    h.processing.process_chunk(&input)
}

pub(super) type TestHarness = Harness<ScriptedEngine, ScriptedDetector, TestStore, FixedClock>;

pub(super) fn harness(
    mut engine: ScriptedEngine,
    spotted: Vec<Spotted>,
    detector: Vec<Result<Detection, DetectorError>>,
    preroll: Option<&'static Preroll>,
    custom: bool,
) -> (TestHarness, Arc<Mutex<StoreLog>>, FixedClock) {
    engine.spotted.extend(spotted);
    let log = Arc::new(Mutex::new(StoreLog::default()));
    let clock = FixedClock::new();
    let (cmd, cmd_rx) = mpsc::channel();
    let (tx, _rx) = unbounded_channel();
    let cfg = ProcessingConfig {
        capture: midi(),
        preroll,
        cmd_rx,
        tx,
        stats: Arc::new(Mutex::new(FrontendStats::default())),
        custom,
    };
    let processing = Processing::new(
        cfg,
        engine,
        Some(ScriptedDetector::new(detector)),
        Some(TestStore { log: log.clone() }),
        clock.clone(),
    );
    (Harness { processing, cmd }, log, clock)
}

pub(super) fn steps(n: usize, amp: f32) -> Vec<Step> {
    (0..n).map(|_| Step::gain(512, amp)).collect()
}

/// Each chunk's samples are its index, so a pre-roll is checked by value.
pub(super) fn indexed_steps(n: usize) -> Vec<Step> {
    (0..n)
        .map(|k| Step {
            gain: vec![k as f32 / 32768.0; 512],
            omni: vec![0.0; 512],
            ok: true,
        })
        .collect()
}

pub(super) fn misses(n: usize) -> Vec<Result<Detection, DetectorError>> {
    (0..n)
        .map(|_| {
            Ok(Detection {
                hit: false,
                score: 0.1,
            })
        })
        .collect()
}

pub(super) fn miss_then_hit(n: usize, score: f32) -> Vec<Result<Detection, DetectorError>> {
    let mut r = misses(n);
    r.push(Ok(Detection { hit: true, score }));
    r
}

pub(super) fn wake_of(events: &[FrontendEvent]) -> Option<(f32, Vec<i16>, Option<EndpointPrior>)> {
    events.iter().find_map(|ev| match ev {
        FrontendEvent::Wake {
            level_db,
            preroll,
            endpoint_prior,
            ..
        } => Some((*level_db, preroll.clone(), *endpoint_prior)),
        _ => None,
    })
}

pub(super) fn audio_count(events: &[FrontendEvent]) -> usize {
    events
        .iter()
        .filter(|ev| matches!(ev, FrontendEvent::Audio(_)))
        .count()
}

pub(super) enum ScriptRead {
    Block(Vec<i16>),
    Again,
    Error,
}

pub(super) struct ScriptedCapture {
    pub(super) reads: VecDeque<ScriptRead>,
    pub(super) recovered: bool,
    pub(super) recoveries: usize,
}

impl CaptureSource for ScriptedCapture {
    fn read(&mut self, out: &mut [i16]) -> Result<usize, CaptureError> {
        match self.reads.pop_front().unwrap_or(ScriptRead::Again) {
            ScriptRead::Block(block) => {
                out[..block.len()].copy_from_slice(&block);
                Ok(block.len())
            }
            ScriptRead::Again => Ok(0),
            ScriptRead::Error => Err(CaptureError::Alsa {
                pcm: "script",
                source: AlsaError::new(HwStep::Read, -5),
            }),
        }
    }

    fn wait(&mut self, _ms: i32) -> Result<(), CaptureError> {
        Ok(())
    }

    fn recover(&mut self) -> bool {
        self.recoveries += 1;
        self.recovered
    }
}
