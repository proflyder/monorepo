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
//!
//! Внешний мир подключается через порты (`CaptureSource`, `WakeEngine`, `Detector`,
//! `WakeStore`, `Clock`): у каждого есть продакшен-реализация и скриптовый фейк, поэтому поток
//! обработки проверяется без станции. Состояние фронтенда и все переключатели живут только на
//! потоке обработки; команды идут одним каналом, а `stats` — снимок для синхронных геттеров.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;

use pulsar_core::MusicFrame;
use pulsar_dsp::Ring;
use pulsar_dsp::endpoint::EndpointPrior;
use pulsar_model::{Capture as CaptureLayout, Preroll};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tracing::{error, info, warn};

use crate::engine::{WakeEngine, YandexEngine};
use crate::wake_events::{self, Outcome, WakeEventOptions, WakeEventsRuntime, logic};

mod adapters;
mod capture;
mod cmd;
mod ports;
mod processing;
mod state;
#[cfg(test)]
mod test_support;

pub use capture::CaptureError;

use adapters::{RecordingStore, RicardoDetector};
use capture::{AlsaCapture, CaptureLoop, Pipe, REOPEN_DELAY};
use cmd::{CaptureCmd, FrontendCmd};
use ports::CaptureSource;
use ports::SystemClock;
use processing::{Processing, ProcessingConfig, processing_thread};

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
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (tap_tx, tap_rx) = mpsc::channel();
    let stats = Arc::new(Mutex::new(FrontendStats::default()));
    let running = Arc::new(AtomicBool::new(false));
    let mut inner = Inner {
        cmd: cmd_tx,
        tap: tap_tx,
        stats: stats.clone(),
        running: running.clone(),
        pipe: None,
        threads: Vec::new(),
        frame: 0,
        wake: wake.and_then(|w| wake_events::start(w, thresholds, from_manifest)),
        // Keeps the event channel open: without a front-end, recv() waits forever instead of
        // returning None.
        _events: tx.clone(),
    };
    let command_dir = command_model_dir.filter(|d| !d.as_os_str().is_empty());
    // Ricardo грузится между VQE и спотерами — см. комментарий в `YandexEngine::start`:
    // порядок RTLD_GLOBAL-загрузок проверен на станции и не переставляется.
    let (engine, detector) =
        match YandexEngine::start(capture, wake_model_dir, command_dir.as_deref(), || {
            custom_wake_model.and_then(|path| {
                RicardoDetector::load(path, thresholds)
                    .inspect_err(|e| {
                        error!("Ricardo wake word: {e}");
                    })
                    .ok()
            })
        }) {
            Ok(pair) => pair,
            Err(e) => {
                error!("audio engine: {e}");
                inner.stop();
                error!("microphone front-end failed to start");
                return (
                    FrontendHandle {
                        inner: Arc::new(inner),
                    },
                    rx,
                );
            }
        };
    inner.frame = engine.frame();
    let store = inner
        .wake
        .as_ref()
        .and_then(WakeEventsRuntime::input)
        .map(|input| RecordingStore::new(input, engine.frame()));
    let cfg = ProcessingConfig {
        capture,
        preroll,
        cmd_rx,
        tx,
        stats: stats.clone(),
        custom: custom_active,
    };
    let processing = Processing::new(cfg, engine, detector, store, SystemClock);
    let period = processing.capture_chunk;
    let nch = capture.channels as usize;
    let pipe = Arc::new(Pipe {
        // 1 s of audio.
        fifo: Mutex::new(Ring::new(period * nch, capture.rate as usize * nch)),
        ready: Condvar::new(),
    });
    inner.pipe = Some(pipe.clone());
    running.store(true, Relaxed);
    let mut open_capture =
        move || AlsaCapture::open(capture, period).map(|s| Box::new(s) as Box<dyn CaptureSource>);
    let capture_thread = match std::thread::Builder::new()
        .name("audio-capture".into())
        .spawn({
            let running = running.clone();
            let pipe = pipe.clone();
            move || {
                CaptureLoop {
                    open: &mut open_capture,
                    nch,
                    period,
                    running: &running,
                    pipe: &pipe,
                    stats: &stats,
                    tap_rx,
                    reopen_delay: REOPEN_DELAY,
                }
                .run();
            }
        }) {
        Ok(t) => t,
        Err(e) => {
            error!("audio threads: {e}");
            inner.stop();
            inner.frame = 0;
            error!("microphone front-end failed to start");
            return (
                FrontendHandle {
                    inner: Arc::new(inner),
                },
                rx,
            );
        }
    };
    inner.threads.push(capture_thread);
    let processing_thread = std::thread::Builder::new()
        .name("audio-process".into())
        .stack_size(PROCESSING_STACK)
        .spawn(move || processing_thread(processing, &running, &pipe));
    match processing_thread {
        Ok(t) => inner.threads.push(t),
        Err(e) => {
            error!("audio threads: {e}");
            inner.stop();
            inner.frame = 0;
            error!("microphone front-end failed to start");
        }
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
        self.send(FrontendCmd::SetModel(
            (!dir.as_os_str().is_empty()).then_some(dir),
        ));
    }

    /// Wake word detection on/off (e.g. software mute). On by default.
    pub fn set_detection(&self, on: bool) {
        self.send(FrontendCmd::SetDetection(on));
    }

    /// Selects the Ricardo detector (true) or the native spotter (false).
    pub fn set_custom_wake(&self, on: bool) {
        self.send(FrontendCmd::SetCustomWake(on));
    }

    /// Command spotter on/off (commands without the wake word).
    pub fn set_commands(&self, on: bool) {
        self.send(FrontendCmd::SetCommands(on));
    }

    /// While on, cleaned 16 kHz audio comes as [`FrontendEvent::Audio`].
    pub fn set_streaming(&self, on: bool) {
        self.send(FrontendCmd::SetStreaming(on));
    }

    /// The owner handled one [`FrontendEvent::Wake`]: ends the stream hold it started. Call it
    /// after `set_streaming` for that wake word.
    pub fn wake_handled(&self) {
        self.send(FrontendCmd::WakeHandled);
    }

    /// Pre-roll in [`FrontendEvent::Wake`] on/off (`Preroll`). On by default; the
    /// stream hold after a `Wake` does not depend on it.
    pub fn set_preroll(&self, on: bool) {
        self.send(FrontendCmd::SetPreroll(on));
    }

    /// While on, music features of the speaker reference come as [`FrontendEvent::Music`].
    pub fn set_music_analysis(&self, on: bool) {
        self.send(FrontendCmd::SetMusic(on));
    }

    /// Counters of the capture and processing threads (frontend_probe).
    pub fn stats(&self) -> FrontendStats {
        *lock(&self.inner.stats)
    }

    /// Samples per [`FrontendEvent::Audio`] chunk (`WakeEngine::frame`); 0 when the front-end did
    /// not start.
    pub fn frame(&self) -> usize {
        self.inner.frame
    }

    /// Wake-event recording (switch «Wake: запись событий»), on by default. Off: no window is
    /// kept and a pending one is dropped; the detector still hears the microphone.
    pub fn set_wake_recording(&self, on: bool) {
        self.send(FrontendCmd::SetRecording(on));
    }

    /// Dry run (switch «Wake: тестовый режим (без ответа)»): a Ricardo hit is recorded with the
    /// outcome `dry_run` and does not answer, arbitrate or duck the music.
    pub fn set_wake_dry_run(&self, on: bool) {
        self.send(FrontendCmd::SetDryRun(on));
    }

    /// «Wake: не услышала»: records the last 30 s of the ring. Ignored while the detector is off
    /// (muted), without the Ricardo model, or with recording off.
    pub fn wake_missed(&self) {
        self.send(FrontendCmd::Missed);
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
        let _ = self.inner.tap.send(CaptureCmd::Tap(tap));
    }

    fn send(&self, cmd: FrontendCmd) {
        let _ = self.inner.cmd.send(cmd);
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

struct Inner {
    cmd: Sender<FrontendCmd>,
    tap: Sender<CaptureCmd>,
    /// Снимок счётчиков: пишут потоки, читает синхронный `stats()`.
    stats: Arc<Mutex<FrontendStats>>,
    /// Флаг жизни потоков, не состояние фронтенда: состояние живёт в `StateMachine` потока
    /// обработки.
    running: Arc<AtomicBool>,
    pipe: Option<Arc<Pipe>>,
    threads: Vec<JoinHandle<()>>,
    /// Samples per VQE output chunk; 0 without a running engine.
    frame: usize,
    /// Writer thread of the wake-event recording; None without the feature or its directory.
    wake: Option<WakeEventsRuntime>,
    _events: UnboundedSender<FrontendEvent>,
}

impl Inner {
    fn stop(&mut self) {
        self.running.store(false, Relaxed);
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

#[cfg(test)]
mod tests {
    use super::*;
    use pulsar_model::profile_from_compatible;

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
