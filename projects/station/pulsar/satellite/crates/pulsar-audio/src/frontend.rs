//! Microphone front-end: ALSA capture (mic + speaker-feedback channels, layout per Model) ->
//! Yandex VQE (AEC / beamforming / noise reduction, per-model preset) -> Yandex phrase spotters
//! (stock models from /system/vendor/quasar/spotter_models): the wake word ("Алиса"/"Яндекс"),
//! the optional on-device «Рикардо» detector, and, optionally, the command model
//! ("стоп", "громче", ...).
//!
//! Two threads: a capture thread that only drains ALSA into a FIFO, and a processing thread
//! (VQE + spotters). The Lite 2's PDM capture allows at most ~42 ms of buffer (2048 frames at
//! 8 ch S16); VQE chunks occasionally take longer than the remaining slack, which overran the
//! device buffer when reading and processing shared one thread.

use std::path::{Path, PathBuf};
use std::sync::atomic::{
    AtomicBool, AtomicU32, AtomicU64, Ordering::AcqRel, Ordering::Acquire, Ordering::Relaxed,
};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use pulsar_alsa::{AlsaError, Format, HwRequest, HwStep, HwTrace, Pcm, Stream};
use pulsar_core::MusicFrame;
use pulsar_dsp::endpoint::EndpointPrior;
use pulsar_dsp::level::{
    LEVEL_CHUNKS, LEVEL_SAMPLES, LevelHistory, level_db, mean_square, percentile_db,
};
use pulsar_dsp::{Ring, SpectrumAnalyzer, to_s16_slice};
use pulsar_model::{Capture as CaptureLayout, CaptureFormat, Preroll, PrerollRule};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tracing::{error, info, warn};

use crate::engine::{CustomFeed, Engine, Stage, VQE_OMNI, VQE_OMNI_WITH_GAIN};
use crate::wake_events::{
    self, Capture, Counters, Job, Outcome, Recording, WakeEventOptions, WakeEventsRuntime,
    WakeInput, logic,
};

/// Pre-roll ring: 1.024 s at 16 kHz (32 chunks of 32 ms, 64 of 16 ms on the Mini 3 and Mini),
/// more than any wake word's confirmation delay.
const PREROLL_SAMPLES: usize = 16384;

/// ALSA buffer in VQE chunks: 256 ms with 32 ms chunks, 128 ms on the Mini 3 and Mini (16 ms);
/// the device may grant less (Lite 2: 2048 frames).
const BUFFER_CHUNKS: u64 = 8;
const READ_WAIT_MS: i32 = 200;
const REOPEN_DELAY: Duration = Duration::from_secs(2);
const CHUNK_WAIT: Duration = Duration::from_millis(500);
// The VQE and the spotters run on the processing thread; the C++ daemon ran them on a default
// glibc thread (8 MiB stack), Rust's default is 2 MiB.
const PROCESSING_STACK: usize = 8 << 20;

pub enum FrontendEvent {
    /// level_db: wake word level in dBFS on VQE_OMNI (after echo cancellation, before AGC),
    /// rounded to 0.1 dB; -100 = no data. `audio` is the 0.5 s pre-roll ending with the chunk
    /// that confirmed the phrase: without it an immediate command loses its first syllables.
    Wake {
        phrase: String,
        level_db: f32,
        /// 16 kHz mono audio from the end of the wake word up to and including the confirming
        /// chunk, oldest first; empty without values for the model (`Preroll`).
        /// The `Audio` events after it continue it without a gap until
        /// [`FrontendHandle::wake_handled`].
        preroll: Vec<i16>,
        /// Levels of the wake window on the same channel as `Audio`, for the experimental
        /// local endpoint detector. None until a full window was seen.
        endpoint_prior: Option<EndpointPrior>,
        /// Id of the recorded wake event (`<utc>-hit-<n>`) when the Ricardo detector's hit was
        /// recorded; the controller writes its dialog outcome to that sidecar. None for the native
        /// spotters, with recording off and on a regular end of the front-end.
        event_id: Option<String>,
    },
    Command {
        word: String,
    },
    /// 16 kHz mono, one VQE chunk (512 samples, 256 on the Mini 3 and Mini), while streaming is on
    /// or a `Wake` is not yet handled.
    Audio(Vec<i16>),
    /// One per 32 ms FFT window while music analysis is on.
    Music(MusicFrame),
}

/// Counters for frontend_probe.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrontendStats {
    /// Frames read from ALSA.
    pub captured_frames: u64,
    /// VQE chunks processed.
    pub chunks: u64,
    /// Frames waiting in the FIFO for the processing thread.
    pub fifo_frames: u64,
    /// Frames dropped because processing fell more than 1 s behind.
    pub dropped_frames: u64,
}

/// Controls of the front-end. Clones share it; the threads stop when the last one is dropped.
#[derive(Clone)]
pub struct FrontendHandle {
    inner: Arc<Inner>,
}

/// Starts the front-end: dlopen, VQE for the model's preset, the native wake word spotter from
/// `wake_model_dir` (e.g. .../spotter_models/activation/alisa), the optional Ricardo detector
/// from `custom_wake_model`, and, if given, the command spotter
/// (.../spotter_models/command), then the capture and processing threads. `custom_active` selects
/// the Ricardo detector initially. On failure (logged, "microphone front-end failed to start")
/// the handle works but no event ever comes. A capture device that cannot be opened is retried
/// every 2 s.
pub fn spawn(
    capture: &'static CaptureLayout,
    preroll: Option<&'static Preroll>,
    wake_model_dir: &Path,
    custom_wake_model: Option<&Path>,
    custom_active: bool,
    command_model_dir: Option<PathBuf>,
    wake: Option<&WakeEventOptions>,
) -> (FrontendHandle, UnboundedReceiver<FrontendEvent>) {
    let (tx, rx) = unbounded_channel();
    // The manifest next to the loaded model sets the detector's cutoff / window; near-miss
    // recording and the sidecars use the same numbers.
    let (thresholds, from_manifest) =
        custom_wake_model.map_or((logic::Thresholds::default(), false), logic::thresholds_for);
    if let Some(path) = custom_wake_model {
        info!(
            "Ricardo thresholds: cutoff {:.2}, window {} ({})",
            thresholds.cutoff,
            thresholds.window,
            if from_manifest {
                format!("{}", path.with_extension("json").display())
            } else {
                "defaults, no valid manifest".to_owned()
            }
        );
    }
    let mut inner = Inner {
        shared: Arc::new(Shared::new(custom_active)),
        pipe: None,
        threads: Vec::new(),
        frame: 0,
        wake: wake.and_then(|w| wake_events::start(w, thresholds, from_manifest)),
        _events: tx.clone(),
    };
    let command_dir = command_model_dir.filter(|d| !d.as_os_str().is_empty());
    let started = match Engine::start(
        capture,
        wake_model_dir,
        custom_wake_model,
        command_dir.as_deref(),
        thresholds,
    ) {
        Ok(eng) => {
            inner.frame = eng.frame();
            inner
                .start_threads(capture, preroll, eng, tx)
                .inspect_err(|e| error!("audio threads: {e}"))
                .is_ok()
        }
        Err(e) => {
            error!("audio engine: {e}");
            false
        }
    };
    if !started {
        inner.stop();
        inner.frame = 0;
        error!("microphone front-end failed to start");
    }
    (
        FrontendHandle {
            inner: Arc::new(inner),
        },
        rx,
    )
}

impl FrontendHandle {
    /// Switches the wake word model (applied by the processing thread before the next chunk).
    pub fn set_model(&self, dir: PathBuf) {
        *lock(&self.inner.shared.pending_model) = (!dir.as_os_str().is_empty()).then_some(dir);
    }

    /// Wake word detection on/off (e.g. software mute). On by default.
    pub fn set_detection(&self, on: bool) {
        self.inner.shared.detect.store(on, Relaxed);
    }

    /// Selects the Ricardo detector (true) or the native spotter (false).
    pub fn set_custom_wake(&self, on: bool) {
        self.inner.shared.custom.store(on, Relaxed);
    }

    /// Command spotter on/off (commands without the wake word).
    pub fn set_commands(&self, on: bool) {
        self.inner.shared.commands.store(on, Relaxed);
    }

    /// While on, cleaned 16 kHz audio comes as [`FrontendEvent::Audio`].
    pub fn set_streaming(&self, on: bool) {
        self.inner.shared.stream.store(on, Relaxed);
    }

    /// The owner handled one [`FrontendEvent::Wake`]: ends the stream hold it started. Call it
    /// after `set_streaming` for that wake word.
    pub fn wake_handled(&self) {
        let _ = self
            .inner
            .shared
            .wake_pending
            .try_update(AcqRel, Acquire, |v| v.checked_sub(1));
    }

    /// Pre-roll in [`FrontendEvent::Wake`] on/off (`Preroll`). On by default; the
    /// stream hold after a `Wake` does not depend on it.
    pub fn set_preroll(&self, on: bool) {
        self.inner.shared.preroll.store(on, Relaxed);
    }

    /// While on, music features of the speaker reference come as [`FrontendEvent::Music`].
    pub fn set_music_analysis(&self, on: bool) {
        self.inner.shared.music.store(on, Relaxed);
    }

    /// Counters of the capture and processing threads (frontend_probe).
    pub fn stats(&self) -> FrontendStats {
        let s = &self.inner.shared;
        FrontendStats {
            captured_frames: s.captured_frames.load(Relaxed),
            chunks: s.chunks.load(Relaxed),
            fifo_frames: s.fifo_frames.load(Relaxed),
            dropped_frames: s.dropped_frames.load(Relaxed),
        }
    }

    /// Samples per [`FrontendEvent::Audio`] chunk (`Engine::frame`); 0 when the front-end did not
    /// start.
    pub fn frame(&self) -> usize {
        self.inner.frame
    }

    /// Wake-event recording (switch «Wake: запись событий»), on by default. Off: no window is
    /// kept and a pending one is dropped; the detector still hears the microphone.
    pub fn set_wake_recording(&self, on: bool) {
        self.inner.shared.recording.store(on, Relaxed);
    }

    /// Dry run (switch «Wake: тестовый режим (без ответа)»): a Ricardo hit is recorded with the
    /// outcome `dry_run` and does not answer, arbitrate or duck the music.
    pub fn set_wake_dry_run(&self, on: bool) {
        self.inner.shared.dry_run.store(on, Relaxed);
    }

    /// «Wake: не услышала»: records the last 30 s of the ring. Ignored while the detector is off
    /// (muted), without the Ricardo model, or with recording off.
    pub fn wake_missed(&self) {
        self.inner.shared.wake_miss.store(true, Relaxed);
    }

    /// The dialog started by a recorded hit ended: fills `outcome` (and `stt_text` when STT
    /// reported one) of that event's sidecar.
    pub fn wake_event_outcome(&self, id: &str, outcome: Outcome, stt_text: Option<&str>) {
        if let Some(w) = &self.inner.wake {
            if !w.handle.set_outcome(id, outcome, stt_text) {
                warn!("wake events: writer is gone, outcome of {id} not saved");
            }
        } else {
            warn!("wake events: recording is not running, outcome of {id} not saved");
        }
    }

    /// «Wake: ложное срабатывание»: labels the newest hit of the last 10 minutes.
    pub fn wake_event_false_alarm(&self) {
        if let Some(w) = &self.inner.wake {
            if !w.handle.false_alarm() {
                warn!("wake events: writer is gone, false-alarm label not saved");
            }
        } else {
            warn!("wake events: recording is not running, false-alarm label not saved");
        }
    }

    /// Every block read from ALSA, interleaved as captured (`capture_channels` per frame), goes
    /// to `tap` while one is set (frontend_probe --record).
    pub fn set_raw_tap(&self, tap: Option<UnboundedSender<Vec<i16>>>) {
        let s = &self.inner.shared;
        let mut t = lock(&s.tap);
        s.tap_on.store(tap.is_some(), Relaxed);
        *t = tap;
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Written by the handle, read by the threads (and the counters the other way round).
struct Shared {
    running: AtomicBool,
    // With stop_cv: wakes the capture thread's reopen delay on stop.
    stop_lock: Mutex<()>,
    stop_cv: Condvar,
    detect: AtomicBool,
    /// Ricardo detector (true) or native Yandex spotter (false).
    custom: AtomicBool,
    commands: AtomicBool,
    stream: AtomicBool,
    /// `Wake` events sent and not yet handled: audio keeps coming meanwhile, so nothing is lost
    /// between the confirmation and the owner's `set_streaming(true)`.
    wake_pending: AtomicU32,
    preroll: AtomicBool,
    music: AtomicBool,
    /// Wake-event recording on (no ring, no writes when off).
    recording: AtomicBool,
    /// Dry run: a hit is recorded, nothing answers.
    dry_run: AtomicBool,
    /// «Wake: не услышала» pressed; the processing thread takes the ring.
    wake_miss: AtomicBool,
    pending_model: Mutex<Option<PathBuf>>,
    tap_on: AtomicBool,
    tap: Mutex<Option<UnboundedSender<Vec<i16>>>>,
    captured_frames: AtomicU64,
    chunks: AtomicU64,
    fifo_frames: AtomicU64,
    dropped_frames: AtomicU64,
}

impl Shared {
    fn new(custom_active: bool) -> Self {
        Self {
            running: AtomicBool::new(false),
            stop_lock: Mutex::new(()),
            stop_cv: Condvar::new(),
            detect: AtomicBool::new(true),
            custom: AtomicBool::new(custom_active),
            commands: AtomicBool::new(false),
            stream: AtomicBool::new(false),
            wake_pending: AtomicU32::new(0),
            preroll: AtomicBool::new(true),
            music: AtomicBool::new(false),
            recording: AtomicBool::new(true),
            dry_run: AtomicBool::new(false),
            wake_miss: AtomicBool::new(false),
            pending_model: Mutex::new(None),
            tap_on: AtomicBool::new(false),
            tap: Mutex::new(None),
            captured_frames: AtomicU64::new(0),
            chunks: AtomicU64::new(0),
            fifo_frames: AtomicU64::new(0),
            dropped_frames: AtomicU64::new(0),
        }
    }

    fn running(&self) -> bool {
        self.running.load(Relaxed)
    }

    /// Sleeps `d` or until stopped.
    fn pause(&self, d: Duration) {
        let g = lock(&self.stop_lock);
        let _ = self
            .stop_cv
            .wait_timeout_while(g, d, |_| self.running())
            .unwrap_or_else(PoisonError::into_inner);
    }
}

/// The FIFO between the threads with its wake-up.
struct Pipe {
    fifo: Mutex<Ring<i16>>,
    ready: Condvar,
}

struct Inner {
    shared: Arc<Shared>,
    pipe: Option<Arc<Pipe>>,
    threads: Vec<JoinHandle<()>>,
    /// Samples per VQE output chunk; 0 without a running engine.
    frame: usize,
    /// Writer thread of the wake-event recording; None without the feature or its directory.
    wake: Option<WakeEventsRuntime>,
    // Keeps the event channel open: without a front-end, recv() waits forever instead of
    // returning None.
    _events: UnboundedSender<FrontendEvent>,
}

impl Inner {
    fn start_threads(
        &mut self,
        capture: &'static CaptureLayout,
        preroll: Option<&'static Preroll>,
        eng: Engine,
        tx: UnboundedSender<FrontendEvent>,
    ) -> std::io::Result<()> {
        let nch = capture.channels as usize;
        let period = eng.chunk;
        let pipe = Arc::new(Pipe {
            // 1 s of audio.
            fifo: Mutex::new(Ring::new(period * nch, capture.rate as usize * nch)),
            ready: Condvar::new(),
        });
        self.pipe = Some(pipe.clone());
        self.shared.running.store(true, Relaxed);
        let (shared, p) = (self.shared.clone(), pipe.clone());
        self.threads.push(
            std::thread::Builder::new()
                .name("audio-capture".into())
                .spawn(move || capture_thread(capture, period, &shared, &p))?,
        );
        let shared = self.shared.clone();
        let wake = self.wake.as_ref().and_then(WakeEventsRuntime::input);
        self.threads.push(
            std::thread::Builder::new()
                .name("audio-process".into())
                .stack_size(PROCESSING_STACK)
                .spawn(move || processing(capture, preroll, eng, &shared, &pipe, &tx, wake))?,
        );
        Ok(())
    }

    fn stop(&mut self) {
        {
            let _g = lock(&self.shared.stop_lock);
            self.shared.running.store(false, Relaxed);
        }
        self.shared.stop_cv.notify_all();
        if let Some(pipe) = &self.pipe {
            drop(lock(&pipe.fifo));
            pipe.ready.notify_all();
        }
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
        // After the processing thread: its job sender is gone, so the writer exits and writes
        // the final `stats.json`.
        if let Some(w) = &mut self.wake {
            w.shutdown();
        }
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Capture buffer per sample format.
enum CaptureBuf {
    S16(Vec<i16>),
    /// Raw S32 frames and their 16-bit conversion.
    S32 {
        raw: Vec<i32>,
        out: Vec<i16>,
        shift: u32,
    },
}

impl CaptureBuf {
    fn new(format: CaptureFormat, samples: usize) -> Self {
        match format {
            CaptureFormat::S16 => Self::S16(vec![0; samples]),
            CaptureFormat::S32 { shift } => Self::S32 {
                raw: vec![0; samples],
                out: vec![0; samples],
                shift,
            },
        }
    }

    fn read(&mut self, pcm: &mut Pcm, frames: usize) -> Result<usize, AlsaError> {
        match self {
            Self::S16(buf) => pcm.readi(buf, frames),
            Self::S32 { raw, .. } => pcm.readi_s32(raw, frames),
        }
    }

    /// The first `samples` samples read, as the VQE's 16-bit input.
    fn samples(&mut self, samples: usize) -> &[i16] {
        match self {
            Self::S16(buf) => &buf[..samples],
            Self::S32 { raw, out, shift } => {
                s32_to_s16(&raw[..samples], *shift, &mut out[..samples]);
                &out[..samples]
            }
        }
    }
}

/// S32_LE carries a right-justified 24-bit sample on the Mini: shifting out the low `shift` bits
/// keeps the sign extension and gives the VQE's 16-bit input; anything wider saturates.
fn s32_to_s16(src: &[i32], shift: u32, dst: &mut [i16]) {
    for (d, &s) in dst.iter_mut().zip(src) {
        *d = (s >> shift).clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
    }
}

fn capture_thread(capture: &'static CaptureLayout, period: usize, shared: &Shared, pipe: &Pipe) {
    let nch = capture.channels as usize;
    let mut pcm: Option<Pcm> = None;
    let mut hw_period = period;
    let mut buf = CaptureBuf::new(capture.format, period * nch);
    while shared.running() {
        let Some(p) = pcm.as_mut() else {
            match open_capture(capture, period) {
                Ok((p, per)) => {
                    pcm = Some(p);
                    hw_period = per;
                }
                Err(e) => {
                    error!("{e}");
                    shared.pause(REOPEN_DELAY);
                }
            }
            continue;
        };
        let mut r = buf.read(p, hw_period);
        if r == Err(AlsaError::new(HwStep::Read, -libc::EAGAIN)) {
            match p.wait(READ_WAIT_MS) {
                // Ready, or timeout (re-checks running).
                Ok(_) => continue,
                Err(e) => r = Err(e),
            }
        }
        let n = match r {
            Ok(n) => n,
            Err(e) => {
                warn!("capture: {e}, recovering");
                lock(&pipe.fifo).drop_partial();
                if p.recover(e.code, true).is_err() {
                    pcm = None;
                }
                continue;
            }
        };
        let block = buf.samples(n * nch);
        shared.captured_frames.fetch_add(n as u64, Relaxed);
        if shared.tap_on.load(Relaxed)
            && let Some(tap) = lock(&shared.tap).as_ref()
        {
            let _ = tap.send(block.to_vec());
        }
        let dropped = {
            let mut f = lock(&pipe.fifo);
            let dropped = f.push(block).map(|items| items / nch);
            shared.fifo_frames.store((f.len() / nch) as u64, Relaxed);
            dropped
        };
        pipe.ready.notify_one();
        if let Some(d) = dropped {
            shared.dropped_frames.fetch_add(d as u64, Relaxed);
            warn!("capture: processing behind, dropping {d} frames");
        }
    }
}

/// The failing capture hw_params step and its error, or the rate the device offered instead of
/// the model's.
#[derive(Debug, PartialEq, Eq)]
pub enum HwFail {
    Step(HwStep, i32),
    Rate(u32),
}

/// Outcome of the capture hw_params in the order the C++ daemon checked them: the rate right
/// after set_rate_near, then period, buffer, apply (the period / buffer read back afterwards were
/// not checked there). Ok = the period and buffer the device chose.
pub fn hw_outcome(t: &HwTrace, rate: u32) -> Result<(u64, u64), HwFail> {
    match t.failed {
        Some((
            op @ (HwStep::Any | HwStep::Access | HwStep::Format | HwStep::Channels | HwStep::Rate),
            e,
        )) => Err(HwFail::Step(op, e)),
        _ if t.near.rate != rate => Err(HwFail::Rate(t.near.rate)),
        Some((op @ (HwStep::Period | HwStep::Buffer | HwStep::Apply), e)) => {
            Err(HwFail::Step(op, e))
        }
        _ => Ok((t.near.period_frames, t.near.buffer_frames)),
    }
}

/// Почему захват не открылся: шаг ALSA или не та частота, на которую согласился драйвер.
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("capture {pcm}: {source}")]
    Alsa {
        pcm: &'static str,
        #[source]
        source: AlsaError,
    },
    #[error("capture {pcm}: rate {got} Hz, need {want}")]
    Rate {
        pcm: &'static str,
        got: u32,
        want: u32,
    },
}

/// Opens the model's capture PCM: its sample width (S16 or S32), channels, exactly its rate (a
/// silent rate mismatch would feed the VQE and spotters wrongly timed audio), period = one VQE
/// chunk, buffer = 8. The period may differ (the FIFO decouples ALSA periods from VQE chunks).
/// Returns the PCM and the frames to read at once.
fn open_capture(capture: &CaptureLayout, period: usize) -> Result<(Pcm, usize), CaptureError> {
    let dev = capture.pcm;
    let format = match capture.format {
        CaptureFormat::S16 => Format::S16Le,
        CaptureFormat::S32 { .. } => Format::S32Le,
    };
    // Non-blocking: reads never wait inside ALSA, so a stop never hangs in snd_pcm_readi.
    let mut pcm = Pcm::open(dev, Stream::Capture, true)
        .map_err(|source| CaptureError::Alsa { pcm: dev, source })?;
    let t = pcm.set_hw_params_traced(&HwRequest {
        format,
        channels: capture.channels,
        rate: capture.rate,
        rate_exact: false,
        period_frames: Some(period as u64),
        buffer_frames: Some(period as u64 * BUFFER_CHUNKS),
    });
    match hw_outcome(&t, capture.rate) {
        Ok((per, buf)) => {
            info!(
                "capture {dev}: {} Hz, {} ch, {} bits, period {per}, buffer {buf}",
                t.near.rate,
                capture.channels,
                capture.format.bits()
            );
            let per = usize::try_from(per).unwrap_or(usize::MAX);
            Ok((pcm, per.min(period)))
        }
        Err(HwFail::Step(op, code)) => Err(CaptureError::Alsa {
            pcm: dev,
            source: AlsaError::new(op, code),
        }),
        Err(HwFail::Rate(got)) => Err(CaptureError::Rate {
            pcm: dev,
            got,
            want: capture.rate,
        }),
    }
}

/// Capture frames -> VQE input: `mics` / `spk` frame-interleaved, scaled to [-1, 1].
pub fn deinterleave(
    input: &[i16],
    nch: usize,
    mic_ch: &[u32],
    spk_ch: &[u32],
    mics: &mut [f32],
    spk: &mut [f32],
) {
    let (nmic, nspk) = (mic_ch.len(), spk_ch.len());
    for (i, f) in input.chunks_exact(nch).enumerate() {
        for (c, &ch) in mic_ch.iter().enumerate() {
            mics[i * nmic + c] = f32::from(f[ch as usize]) * (1.0 / 32768.0);
        }
        for (c, &ch) in spk_ch.iter().enumerate() {
            spk[i * nspk + c] = f32::from(f[ch as usize]) * (1.0 / 32768.0);
        }
    }
}

/// Wake levels for the local endpoint detector on the stream channel, once the window is full.
/// A partial window would count its zero padding as a -100 dB floor.
fn endpoint_prior(hist: &[f32; LEVEL_CHUNKS], filled: usize) -> Option<EndpointPrior> {
    (filled == LEVEL_CHUNKS).then(|| EndpointPrior {
        noise_db: percentile_db(hist, 0.10).as_f32(),
        speech_db: level_db(hist).as_f32(),
    })
}

/// Pre-roll length in chunks for `rule`, given the chunks since the 1st-stage hit.
fn preroll_chunks(rule: Option<PrerollRule>, first_stage_age: Option<usize>) -> usize {
    match (rule, first_stage_age) {
        (None, _) => 0,
        (Some(PrerollRule::Fixed(n)), _) => n as usize,
        (Some(PrerollRule::FirstStage { offset, .. }), Some(a)) => {
            (a as i64 + i64::from(offset)).max(0) as usize
        }
        (Some(PrerollRule::FirstStage { fallback, .. }), None) => fallback as usize,
    }
}

/// Pre-roll rule for the active detector. The custom Ricardo detector has no stock model
/// directory, so its id is not `Engine::wake_id` (which still points at the native «Алиса»
/// model kept for rollback) but the `ricardo` entry of `Preroll`.
fn wake_preroll_rule(
    preroll: Option<&'static Preroll>,
    shared: &Shared,
    stock_id: &str,
) -> Option<PrerollRule> {
    if !shared.preroll.load(Relaxed) {
        return None;
    }
    let id = if shared.custom.load(Relaxed) {
        "ricardo"
    } else {
        stock_id
    };
    preroll?.rules.iter().find(|p| p.id == id).map(|p| p.rule)
}

/// "-" for None in the "wake timing" log.
struct Chunks(Option<usize>);

impl std::fmt::Display for Chunks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Some(n) => write!(f, "{n}"),
            None => f.write_str("-"),
        }
    }
}

/// Capture state of the wake-event recording, all on the processing thread.
struct WakeState {
    capture: Capture,
    jobs: SyncSender<Job>,
    counters: Arc<Counters>,
}

impl WakeState {
    /// Hands a finished window to the writer; a full queue drops it and counts the drop.
    fn send(&self, rec: Recording) {
        if self.jobs.try_send(Job::Save(Box::new(rec))).is_err() {
            self.counters.dropped.fetch_add(1, Relaxed);
            warn!("wake events: writer is behind, window dropped");
        }
    }
}

fn processing(
    capture: &'static CaptureLayout,
    preroll: Option<&'static Preroll>,
    mut eng: Engine,
    shared: &Shared,
    pipe: &Pipe,
    tx: &UnboundedSender<FrontendEvent>,
    wake_input: Option<WakeInput>,
) {
    let period = eng.chunk;
    let nch = capture.channels as usize;
    let (mic_ch, spk_ch) = (capture.mic_channels, capture.spk_channels);
    let nspk = spk_ch.len();
    let mut input = vec![0i16; period * nch];
    let mut mics = vec![0f32; period * mic_ch.len()];
    let mut spk = vec![0f32; period * nspk];
    let mut level_hist = LevelHistory::new();
    // The same on VQE_OMNI_WITH_GAIN, the channel the stream (and so the endpoint detector)
    // carries: the wake prior must not mix channels.
    let mut gain_hist = [0f32; LEVEL_CHUNKS];
    let mut gain_pos = 0;
    let mut gain_filled = 0usize;
    let mut ana = SpectrumAnalyzer::new(capture.rate, period);
    if !ana.ok() {
        warn!("music analysis: unsupported capture layout");
    }
    let mut music_was_on = false;
    let mut custom_failed = false;
    let frame = eng.frame();
    // Пре-ролл в s16: кольцо хранит уже сконвертированные выборки, тот же чанк отдаётся
    // детектору Ricardo и в `Audio`.
    let mut ring = Ring::new(frame, PREROLL_SAMPLES);
    let mut pcm_chunk = vec![0i16; frame];
    let mut wake = wake_input.map(|w| WakeState {
        capture: Capture::new(frame, w.thresholds.cutoff),
        jobs: w.jobs,
        counters: w.counters,
    });
    let chunk_ms = frame as u64 * 1000 / u64::from(logic::SAMPLE_RATE);
    // Chunks since the wake spotter's 1st-stage hit, while its verifier waits.
    let mut first_stage_age: Option<usize> = None;

    while shared.running() {
        let pending = lock(&shared.pending_model).take();
        if let Some(dir) = pending {
            first_stage_age = None;
            if !eng.set_wake_model(&dir) {
                error!("failed to switch wake word model to {}", dir.display());
            }
        }
        // Exactly one VQE chunk.
        {
            let (mut f, _) = pipe
                .ready
                .wait_timeout_while(lock(&pipe.fifo), CHUNK_WAIT, |f| {
                    !f.has_chunk() && shared.running()
                })
                .unwrap_or_else(PoisonError::into_inner);
            if !f.take_chunk(&mut input) {
                continue;
            }
            shared.fifo_frames.store((f.len() / nch) as u64, Relaxed);
        }
        shared.chunks.fetch_add(1, Relaxed);

        deinterleave(&input, nch, mic_ch, spk_ch, &mut mics, &mut spk);
        let music_on = shared.music.load(Relaxed) && ana.ok();
        if music_on && !music_was_on {
            ana.reset();
        }
        music_was_on = music_on;
        let mut mf = MusicFrame::default();
        if music_on && ana.process(&spk, nspk, &mut mf) {
            let _ = tx.send(FrontendEvent::Music(mf));
        }
        if !eng.process(&mics, &spk) {
            ring.clear();
            // The prior must not span a gap in the audio.
            gain_hist = [0.0; LEVEL_CHUNKS];
            gain_pos = 0;
            gain_filled = 0;
            continue;
        }

        // VQE_OMNI is cleaned of the station's own playback but has no AGC, so it keeps the
        // distance to the talker; VQE_OMNI_WITH_GAIN, which the spotter hears, normalizes it
        // away.
        if let Some(d) = eng.output(VQE_OMNI)
            && !d.is_empty()
        {
            level_hist.push(d);
        }

        // Every chunk goes into the ring, so a pre-roll never spans a gap. A `Wake` not yet
        // handled holds the stream open: the owner switches streaming on only when it handles
        // the event, and the chunks in between continue the pre-roll. `wake_pending` is read
        // first: its Acquire pairs with `wake_handled`, after the owner's `set_streaming(true)`.
        let custom_active = shared.custom.load(Relaxed);
        let detecting = shared.detect.load(Relaxed);
        let dry_run = shared.dry_run.load(Relaxed);
        let mut custom_hit = None;
        let mut custom_mean = None;
        let mut custom_feeding = false;
        let mut wake_hit_id = None;
        if let Some(d) = eng.output(VQE_OMNI_WITH_GAIN).filter(|d| !d.is_empty()) {
            gain_hist[gain_pos] = mean_square(d);
            gain_pos = (gain_pos + 1) % LEVEL_CHUNKS;
            gain_filled = (gain_filled + 1).min(LEVEL_CHUNKS);
            let stored = if d.len() == frame && frame > 0 {
                to_s16_slice(d, &mut pcm_chunk);
                ring.push_chunk(&pcm_chunk)
            } else {
                ring.clear();
                false
            };
            if stored {
                let slot = &pcm_chunk[..];
                // The Ricardo detector hears the same cleaned chunk as the ring and the
                // stock spotter; its confirmation then takes the pre-roll from the ring.
                if custom_active && detecting && !custom_failed {
                    custom_feeding = true;
                    match eng.feed_custom(slot) {
                        CustomFeed::Fed { hit: true, score } => {
                            custom_hit = Some(score);
                            custom_mean = Some(score);
                        }
                        CustomFeed::Fed { hit: false, score } => {
                            // The windowed mean of every chunk drives the near-miss band;
                            // negative means the detector window is still filling.
                            custom_mean = (score >= 0.0).then_some(score);
                        }
                        CustomFeed::Failed(e) => {
                            custom_failed = true;
                            error!("Ricardo wake word: {e}");
                        }
                        CustomFeed::Unavailable => {
                            custom_failed = true;
                            error!("Ricardo wake word: detector is not loaded");
                        }
                    }
                }
                // The same detector input, in a 31 s ring: a hit keeps 3 s before and 1 s
                // after, a miss the last 30 s. Off when muted or without the Ricardo model.
                let recording = wake.is_some()
                    && shared.recording.load(Relaxed)
                    && custom_active
                    && detecting
                    && !custom_failed;
                if recording {
                    let now = pulsar_core::now_ms();
                    let utc = logic::utc_ms();
                    let miss = shared.wake_miss.swap(false, Relaxed);
                    let st = wake.as_mut().expect("wake is Some when recording");
                    if miss && let Some(rec) = st.capture.miss(utc) {
                        st.counters.miss.fetch_add(1, Relaxed);
                        st.send(rec);
                    }
                    let p = st
                        .capture
                        .push(slot, custom_mean, custom_hit, dry_run, now, utc);
                    if p.hit.is_some() {
                        st.counters.hits.fetch_add(1, Relaxed);
                    }
                    if p.near {
                        st.counters.near.fetch_add(1, Relaxed);
                    }
                    if p.dropped {
                        st.counters.dropped.fetch_add(1, Relaxed);
                    }
                    for rec in p.finished {
                        st.send(rec);
                    }
                    wake_hit_id = p.hit.map(|h| h.id);
                } else {
                    // No ring without recording: a miss button pressed meanwhile is void.
                    shared.wake_miss.store(false, Relaxed);
                    if let Some(st) = wake.as_mut() {
                        st.capture.reset();
                    }
                }
                if shared.wake_pending.load(Acquire) > 0 || shared.stream.load(Relaxed) {
                    let _ = tx.send(FrontendEvent::Audio(slot.to_vec()));
                }
            }
        } else {
            ring.clear();
            // A missing output is a gap: restart the prior window.
            gain_hist = [0.0; LEVEL_CHUNKS];
            gain_pos = 0;
            gain_filled = 0;
        }

        // Listening time is the audio the detector actually heard (contract v1).
        if custom_feeding && let Some(st) = wake.as_ref() {
            st.counters.listening_ms.fetch_add(chunk_ms, Relaxed);
            if dry_run {
                st.counters.dry_run_ms.fetch_add(chunk_ms, Relaxed);
            }
        }

        if let Some(score) = custom_hit {
            if dry_run {
                // Recorded with the outcome `dry_run` by the writer; no earcon, no claim, no
                // dialog, and the music keeps playing.
                info!(
                    "Ricardo wake word detected (probability {score:.3}): dry run, not answering"
                );
            } else {
                let level = level_hist.level().as_f32();
                let speech_end = level_hist.speech_end(frame);
                let endpoint_prior = endpoint_prior(&gain_hist, gain_filled);
                let rule = wake_preroll_rule(preroll, shared, "ricardo");
                let n = preroll_chunks(rule, None).min(ring.chunks());
                let preroll = ring.tail(n);
                info!(
                    "Ricardo wake word detected (probability {score:.3}); \
                     wake timing: speech_end={} preroll={}",
                    Chunks(speech_end),
                    preroll.len() / frame
                );
                shared.wake_pending.fetch_add(1, Relaxed);
                let _ = tx.send(FrontendEvent::Wake {
                    phrase: "Рикардо".into(),
                    level_db: level,
                    preroll,
                    endpoint_prior,
                    event_id: wake_hit_id,
                });
            }
        } else if !custom_active && detecting {
            // Kept for the level window's 1.5 s, whatever the VQE chunk.
            first_stage_age = first_stage_age
                .map(|a| a + 1)
                .filter(|&a| a * frame < LEVEL_CHUNKS * LEVEL_SAMPLES);
            let spotted = eng.feed_wake();
            match spotted.stage {
                Stage::FirstStageHit => first_stage_age = Some(0),
                Stage::Rejected => first_stage_age = None,
                Stage::None => {}
            }
            if !spotted.phrases.is_empty() {
                let level = level_hist.level().as_f32();
                let speech_end = level_hist.speech_end(frame);
                let endpoint_prior = endpoint_prior(&gain_hist, gain_filled);
                let rule = wake_preroll_rule(preroll, shared, eng.wake_id());
                let n = preroll_chunks(rule, first_stage_age).min(ring.chunks());
                let preroll = ring.tail(n);
                info!(
                    "wake timing: first_stage={} speech_end={} preroll={}",
                    Chunks(first_stage_age),
                    Chunks(speech_end),
                    preroll.len() / frame
                );
                first_stage_age = None;
                for phrase in spotted.phrases {
                    shared.wake_pending.fetch_add(1, Relaxed);
                    let _ = tx.send(FrontendEvent::Wake {
                        phrase,
                        level_db: level,
                        preroll: preroll.clone(),
                        endpoint_prior,
                        event_id: None,
                    });
                }
            }
        } else {
            first_stage_age = None;
        }
        if shared.commands.load(Relaxed) {
            for word in eng.feed_command() {
                let _ = tx.send(FrontendEvent::Command { word });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsar_alsa::HwResult;
    use pulsar_model::profile_from_compatible;

    #[test]
    fn s32_capture_keeps_the_top_16_of_24_bits() {
        // Right-justified 24-bit samples (the Mini, shift 8): the sign survives the shift, and
        // a value outside 24 bits saturates instead of wrapping.
        let src = [
            -1 << 8,
            0x007F_FF00,
            -0x0080_0000,
            0x0000_00FF,
            -1,
            0x0100_0000,
            i32::MIN,
        ];
        let mut dst = [0i16; 7];
        s32_to_s16(&src, 8, &mut dst);
        assert_eq!(dst, [-1, 32767, -32768, 0, -1, 32767, -32768]);
    }

    #[test]
    fn preroll_length_per_rule() {
        let first = |offset, fallback| Some(PrerollRule::FirstStage { offset, fallback });
        assert_eq!(preroll_chunks(None, Some(5)), 0);
        assert_eq!(preroll_chunks(Some(PrerollRule::Fixed(7)), None), 7);
        assert_eq!(preroll_chunks(Some(PrerollRule::Fixed(7)), Some(2)), 7);
        assert_eq!(preroll_chunks(first(3, 9), Some(4)), 7);
        assert_eq!(preroll_chunks(first(-3, 9), Some(4)), 1);
        // An anchor closer than the offset: nothing rather than the word itself.
        assert_eq!(preroll_chunks(first(-3, 9), Some(2)), 0);
        // No 1st-stage hit before the confirmation.
        assert_eq!(preroll_chunks(first(-3, 9), None), 9);
    }

    #[test]
    fn ricardo_preroll_rule_uses_the_custom_id() {
        let midi = profile_from_compatible(b"yandex,cucumber\0").unwrap();
        // The custom detector is active: the stock model id ("alisa", kept for rollback) must
        // not select the rule, or the Ricardo word loses its pre-roll.
        let custom = Shared::new(true);
        assert_eq!(
            wake_preroll_rule(midi.wake_preroll.as_ref(), &custom, "alisa"),
            Some(PrerollRule::Fixed(8))
        );
        // Switched back to a stock spotter: the custom entry no longer applies.
        let stock = Shared::new(false);
        assert_eq!(
            wake_preroll_rule(midi.wake_preroll.as_ref(), &stock, "alisa"),
            Some(PrerollRule::Fixed(2))
        );
    }

    #[test]
    fn ricardo_preroll_is_the_last_256_ms_of_the_ring() {
        const FRAME: usize = 512; // 16 kHz, one VQE chunk = 32 ms
        const RING_CHUNKS: usize = PREROLL_SAMPLES / FRAME;
        let midi = profile_from_compatible(b"yandex,cucumber\0").unwrap();
        let mut ring = Ring::new(FRAME, PREROLL_SAMPLES);
        let mut chunk = vec![0i16; FRAME];
        for k in 0..RING_CHUNKS {
            to_s16_slice(&[k as f32 / 32768.0; FRAME], &mut chunk);
            ring.push_chunk(&chunk);
        }
        let rule = wake_preroll_rule(midi.wake_preroll.as_ref(), &Shared::new(true), "ricardo");
        let n = preroll_chunks(rule, None);
        assert_eq!(n, 8);
        let preroll = ring.tail(n);
        assert_eq!(preroll.len(), n * FRAME);
        assert_eq!(preroll.len() / 16, 256, "milliseconds at 16 kHz");
        // Oldest first, ending with the chunk that confirmed the word.
        assert_eq!(preroll[0], (RING_CHUNKS - n) as i16);
        assert_eq!(preroll[preroll.len() - 1], (RING_CHUNKS - 1) as i16);
    }

    #[test]
    fn preroll_off_is_empty_but_the_wake_hold_is_separate() {
        let midi = profile_from_compatible(b"yandex,cucumber\0").unwrap();
        let shared = Shared::new(true);
        shared.preroll.store(false, Relaxed);
        let n = preroll_chunks(
            wake_preroll_rule(midi.wake_preroll.as_ref(), &shared, "ricardo"),
            None,
        );
        assert_eq!(n, 0);
        let mut ring = Ring::new(512, PREROLL_SAMPLES);
        ring.push_chunk(&[16384; 512]);
        assert!(ring.tail(n).is_empty());
        // `wake_pending` holds the stream open regardless of the pre-roll switch: the owner
        // raises it for every Wake and lowers it once after `set_streaming`.
        shared.wake_pending.fetch_add(1, Relaxed);
        assert_eq!(shared.wake_pending.load(Relaxed), 1);
        let _ = shared
            .wake_pending
            .try_update(AcqRel, Acquire, |v| v.checked_sub(1));
        assert_eq!(shared.wake_pending.load(Relaxed), 0);
    }

    #[test]
    fn deinterleave_supported_layouts() {
        let midi = profile_from_compatible(b"yandex,cucumber\0").unwrap();
        let lite2 = profile_from_compatible(b"yandex,plum\0").unwrap();
        let mini3 = profile_from_compatible(b"yandex,bergamot\0").unwrap();
        for m in [midi, lite2, mini3] {
            let cap = &m.audio.capture;
            let nch = cap.channels as usize;
            let frames = 4;
            // Sample = frame * 100 + channel.
            let input: Vec<i16> = (0..frames * nch)
                .map(|s| ((s / nch) * 100 + s % nch) as i16)
                .collect();
            let mut mics = vec![0.0; frames * cap.mic_channels.len()];
            let mut spk = vec![0.0; frames * cap.spk_channels.len()];
            deinterleave(
                &input,
                nch,
                cap.mic_channels,
                cap.spk_channels,
                &mut mics,
                &mut spk,
            );
            let v = |frame: usize, ch: u32| (frame * 100 + ch as usize) as f32 / 32768.0;
            for i in 0..frames {
                for (c, &ch) in cap.mic_channels.iter().enumerate() {
                    assert_eq!(mics[i * cap.mic_channels.len() + c], v(i, ch));
                }
                for (c, &ch) in cap.spk_channels.iter().enumerate() {
                    assert_eq!(spk[i * cap.spk_channels.len() + c], v(i, ch));
                }
            }
        }
        // Lite 2: mic 0 on ch 0, mic 1 on ch 2, speaker feedback on ch 6.
        let input: Vec<i16> = vec![
            -32768, 1, 16384, 3, 4, 5, 32767, 7, //
            8, 9, 10, 11, 12, 13, 14, 15,
        ];
        let (mut mics, mut spk) = ([0.0; 4], [0.0; 2]);
        deinterleave(
            &input,
            8,
            lite2.audio.capture.mic_channels,
            lite2.audio.capture.spk_channels,
            &mut mics,
            &mut spk,
        );
        assert_eq!(mics, [-1.0, 0.5, 8.0 / 32768.0, 10.0 / 32768.0]);
        assert_eq!(spk, [32767.0 / 32768.0, 14.0 / 32768.0]);
    }

    fn trace(rate: u32, failed: Option<(HwStep, i32)>) -> HwTrace {
        let r = HwResult {
            rate,
            period_frames: 1536,
            buffer_frames: 12288,
        };
        HwTrace {
            near: r,
            actual: r,
            failed,
        }
    }

    #[test]
    fn hw_params_checked_in_cxx_order() {
        assert_eq!(hw_outcome(&trace(48000, None), 48000), Ok((1536, 12288)));
        // The rate is checked right after set_rate_near, before period / buffer / apply.
        assert_eq!(
            hw_outcome(&trace(44100, None), 48000),
            Err(HwFail::Rate(44100))
        );
        assert_eq!(
            hw_outcome(&trace(44100, Some((HwStep::Apply, -5))), 48000),
            Err(HwFail::Rate(44100))
        );
        assert_eq!(
            hw_outcome(&trace(44100, Some((HwStep::Channels, -22))), 48000),
            Err(HwFail::Step(HwStep::Channels, -22))
        );
        assert_eq!(
            hw_outcome(&trace(48000, Some((HwStep::Rate, -22))), 48000),
            Err(HwFail::Step(HwStep::Rate, -22))
        );
        assert_eq!(
            hw_outcome(&trace(48000, Some((HwStep::Buffer, -22))), 48000),
            Err(HwFail::Step(HwStep::Buffer, -22))
        );
        assert_eq!(
            hw_outcome(&trace(48000, Some((HwStep::Apply, -16))), 48000),
            Err(HwFail::Step(HwStep::Apply, -16))
        );
        // Reading the sizes back was not a C++ step: the near values count.
        assert_eq!(
            hw_outcome(&trace(48000, Some((HwStep::GetPeriod, -5))), 48000),
            Ok((1536, 12288))
        );
    }

    #[test]
    fn without_the_yandex_library_the_handle_is_degraded() {
        // No /system/vendor/quasar/libs on the build host: dlopen fails.
        let midi = profile_from_compatible(b"yandex,cucumber\0").unwrap();
        let (h, mut rx) = spawn(
            &midi.audio.capture,
            midi.wake_preroll.as_ref(),
            Path::new("/nonexistent"),
            None,
            false,
            None,
            None,
        );
        h.set_streaming(true);
        h.set_model(PathBuf::from("/nonexistent/2"));
        let h2 = h.clone();
        drop(h);
        h2.set_music_analysis(true);
        assert_eq!(h2.stats(), FrontendStats::default());
        // No event, and the channel stays open while a handle lives.
        assert!(matches!(
            rx.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ));
        drop(h2);
        assert!(matches!(
            rx.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected)
        ));
    }
}
