//! Wake-event recording for the Ricardo detector (contract v1): the audio windows of confirmed
//! hits, near-misses and user-reported misses, their WAV + JSON sidecars, retention and stats.
//!
//! The audio processing thread keeps the ring and the capture state ([`Capture`]) and hands
//! finished buffers to one writer thread through a bounded channel (`try_send`: a stalled disk
//! never blocks the microphone; the drop is counted). The writer owns all file I/O and the
//! in-memory index; the controller reaches it through [`WakeEventsHandle`] for the dialog outcome
//! and the «false alarm» label, and the processing thread through its own handle for the «miss»
//! button.
#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

pub mod capture;
pub mod contract;
pub mod logic;
mod writer;

pub use capture::Capture;
pub use contract::{Sidecar, Stats, Thresholds};
pub use logic::{Kind, Meta, Outcome};

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::mpsc::{self, SyncSender};
use std::thread::JoinHandle;

use tracing::{error, info, warn};

/// Ring of the detector's input stream: the 30 s of a miss plus the 3 s before a hit.
pub const RING_MS: u32 = 31_000;
pub const HIT_PRE_MS: u32 = 3000;
pub const POST_MS: u32 = 1000;
pub const MISS_MS: u32 = 30_000;

/// Finished windows waiting for the writer thread.
const JOB_QUEUE: usize = 8;

/// Ошибка записи файла контракта: путь и причина, без потери контекста шага.
#[derive(Debug, thiserror::Error)]
pub enum WakeStoreError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// Where and for which build the events are recorded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WakeEventOptions {
    /// `/data/pulsar/wake-events`.
    pub dir: PathBuf,
    /// The daemon's `VERSION`.
    pub version: String,
    /// The loaded `.tflite` (its sha256 goes into every sidecar).
    pub model: PathBuf,
}

/// One finished window on its way to the writer thread.
#[derive(Debug)]
pub struct Recording {
    pub id: String,
    pub kind: Kind,
    pub utc_ms: i64,
    pub pre_ms: u32,
    pub post_ms: u32,
    pub score_peak: f32,
    pub dry_run: bool,
    pub samples: Vec<i16>,
    pub label: Option<&'static str>,
    pub label_source: Option<&'static str>,
}

pub enum Job {
    Save(Box<Recording>),
}

pub(crate) enum Command {
    /// The dialog started by this hit ended.
    Outcome {
        id: String,
        outcome: Outcome,
        stt_text: Option<String>,
    },
    /// «Wake: ложное срабатывание»: label the newest hit of the last 10 minutes.
    FalseAlarm,
}

/// Counters of this run; the writer adds them to the loaded `stats.json`.
#[derive(Default)]
pub struct Counters {
    /// Audio actually fed to the detector (not muted, detector active).
    pub listening_ms: AtomicU64,
    /// The subset while the dry-run switch was on.
    pub dry_run_ms: AtomicU64,
    pub hits: AtomicU64,
    pub near: AtomicU64,
    pub miss: AtomicU64,
    /// Windows the writer queue had no room for.
    pub dropped: AtomicU64,
}

/// The controller's side of the writer thread.
#[derive(Clone)]
pub struct WakeEventsHandle {
    cmd: mpsc::Sender<Command>,
}

impl WakeEventsHandle {
    /// Fills `outcome` (and `stt_text` when the dialog reported one) of a hit's sidecar. False
    /// when the writer thread is gone (the file then keeps a null outcome).
    pub fn set_outcome(&self, id: &str, outcome: Outcome, stt_text: Option<&str>) -> bool {
        self.cmd
            .send(Command::Outcome {
                id: id.to_owned(),
                outcome,
                stt_text: stt_text.map(str::to_owned),
            })
            .is_ok()
    }

    /// Labels the newest hit of the last 10 minutes `false` / `user`.
    pub fn false_alarm(&self) -> bool {
        self.cmd.send(Command::FalseAlarm).is_ok()
    }
}

/// What the processing thread needs to create and save windows.
pub struct WakeInput {
    pub jobs: SyncSender<Job>,
    pub counters: Arc<Counters>,
    pub thresholds: Thresholds,
}

pub struct WakeEventsRuntime {
    pub handle: WakeEventsHandle,
    pub thresholds: Thresholds,
    counters: Arc<Counters>,
    jobs: Option<SyncSender<Job>>,
    writer: Option<JoinHandle<()>>,
}

impl WakeEventsRuntime {
    /// A clone for the processing thread; None once the runtime is shutting down.
    pub fn input(&self) -> Option<WakeInput> {
        Some(WakeInput {
            jobs: self.jobs.clone()?,
            counters: self.counters.clone(),
            thresholds: self.thresholds,
        })
    }

    /// Stops the writer after the processing thread has stopped and its sender is gone; the final
    /// `stats.json` is written on the way out.
    pub fn shutdown(&mut self) {
        self.jobs = None;
        if let Some(t) = self.writer.take() {
            let _ = t.join();
        }
    }
}

/// Lower-case hex SHA-256.
pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(ring::digest::digest(&ring::digest::SHA256, data).as_ref())
}

/// Starts the writer thread with the thresholds the detector was loaded with. None: the
/// directory cannot be created (logged), so a run records nothing.
pub fn start(
    opts: &WakeEventOptions,
    thresholds: Thresholds,
    from_manifest: bool,
) -> Option<WakeEventsRuntime> {
    let dir = opts.dir.clone();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        error!("wake events: cannot create {}: {e}", dir.display());
        return None;
    }
    let sha256 = match std::fs::read(&opts.model) {
        Ok(bytes) => sha256_hex(&bytes),
        Err(e) => {
            warn!(
                "wake events: cannot read the model {}: {e}; sidecars carry no sha256",
                opts.model.display()
            );
            String::new()
        }
    };
    info!(
        "wake events: {} (cutoff {:.2}, window {}, {}, model {})",
        dir.display(),
        thresholds.cutoff,
        thresholds.window,
        if from_manifest {
            "model manifest"
        } else {
            "defaults"
        },
        if sha256.is_empty() {
            "sha256 unavailable".to_owned()
        } else {
            format!("sha256 {}", &sha256[..12.min(sha256.len())])
        }
    );
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (job_tx, job_rx) = mpsc::sync_channel(JOB_QUEUE);
    let counters = Arc::new(Counters::default());
    let writer = writer::spawn(
        dir,
        opts.version.clone(),
        sha256,
        thresholds,
        counters.clone(),
        cmd_rx,
        job_rx,
    )?;
    Some(WakeEventsRuntime {
        handle: WakeEventsHandle { cmd: cmd_tx },
        thresholds,
        counters,
        jobs: Some(job_tx),
        writer: Some(writer),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_is_lower_case_hex() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(sha256_hex(b"abc").len(), 64);
    }
}
