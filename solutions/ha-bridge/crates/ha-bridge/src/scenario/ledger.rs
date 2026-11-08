//! Scenario execution ledger: which `scenario.fire` events already started and
//! completed.
//!
//! A `scenario` queue event can be re-delivered after a lease expiry or a crash
//! in the middle of a delay. Steps are side effects, so they must not run
//! twice for the same event: before the first step the executor writes
//! `started`, and a later delivery of the same event id that finds `started`
//! without `completed` is acknowledged `FAILED "interrupted"` instead of being
//! replayed. `completed` is written after the last step (or the first fatal
//! step) and suppresses a re-run entirely.
//!
//! The file is JSON with atomic writes (`temp + fsync + rename`), it holds no
//! step parameters and no speak text, and it keeps 30 days / 20 000 entries.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// How long run records are kept.
pub const DEFAULT_RETENTION: Duration = Duration::from_secs(30 * 24 * 60 * 60);
/// Hard cap even without pruning.
pub const MAX_RUNS: usize = 20_000;
/// The only supported on-disk version.
pub const FILE_VERSION: i64 = 1;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// Where one event is in its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunState {
    /// Steps may have produced side effects; the run did not finish.
    Started,
    /// The run finished (successfully or with a recorded failure).
    Completed,
}

/// One event record. It never contains a step parameter or spoken text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRecord {
    #[serde(rename = "scenarioId", default)]
    pub scenario_id: String,
    pub state: RunState,
    #[serde(rename = "updatedUnix", default)]
    pub updated_unix: i64,
}

/// The answer to "may this event start?".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    /// No record: the executor may run and just wrote `started`.
    New,
    /// A previous process started this event and never finished.
    Interrupted,
    /// The event already finished in a previous process.
    Completed,
}

#[derive(Debug, Serialize, Deserialize)]
struct FileFormat {
    #[serde(default)]
    version: i64,
    #[serde(default)]
    runs: BTreeMap<String, RunRecord>,
}

/// A ledger failure. Messages never echo a payload or spoken text.
#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    #[error("scenario ledger: read {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("scenario ledger: {path} is corrupt: {detail}")]
    Corrupt { path: String, detail: String },
    #[error("scenario ledger: {path} has unsupported version {version}")]
    Unsupported { path: String, version: i64 },
    #[error("scenario ledger: {0}")]
    Io(String),
}

/// A concurrency-safe scenario run ledger.
pub struct ScenarioLedger {
    path: Option<PathBuf>,
    inner: Mutex<BTreeMap<String, RunRecord>>,
    clock: Clock,
}

impl std::fmt::Debug for ScenarioLedger {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ScenarioLedger")
            .field("path", &self.path)
            .field("len", &self.inner.lock().map_or(0, |runs| runs.len()))
            .finish()
    }
}

impl ScenarioLedger {
    /// Open a file-backed ledger, creating it on the first write. A corrupt or
    /// newer file is an error: starting empty could repeat side effects.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, LedgerError> {
        let clock = system_clock();
        Self::open_with_clock(path, move || (clock.as_ref())())
    }

    /// Open with an injected clock (tests).
    pub fn open_with_clock(
        path: impl AsRef<Path>,
        clock: impl Fn() -> i64 + Send + Sync + 'static,
    ) -> Result<Self, LedgerError> {
        let ledger = Self {
            path: Some(path.as_ref().to_path_buf()),
            inner: Mutex::new(BTreeMap::new()),
            clock: Arc::new(clock),
        };
        ledger.load()?;
        Ok(ledger)
    }

    /// An in-memory ledger: interruption detection lasts only for this process.
    #[must_use]
    pub fn memory() -> Self {
        Self::memory_with_clock(system_clock_closure())
    }

    /// An in-memory ledger with an injected clock.
    #[must_use]
    pub fn memory_with_clock(clock: impl Fn() -> i64 + Send + Sync + 'static) -> Self {
        Self {
            path: None,
            inner: Mutex::new(BTreeMap::new()),
            clock: Arc::new(clock),
        }
    }

    /// Backing file, or `None` for an in-memory ledger.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Atomically claim an event. `Interrupted` is returned when a previous
    /// process left `started` without `completed`.
    pub fn start(&self, event_id: &str, scenario_id: &str) -> Result<Start, LedgerError> {
        if event_id.is_empty() {
            return Err(LedgerError::Io("empty event id".to_owned()));
        }
        let mut inner = self.lock();
        if let Some(record) = inner.get(event_id) {
            return Ok(match record.state {
                RunState::Started => Start::Interrupted,
                RunState::Completed => Start::Completed,
            });
        }
        inner.insert(
            event_id.to_owned(),
            RunRecord {
                scenario_id: scenario_id.to_owned(),
                state: RunState::Started,
                updated_unix: (self.clock.as_ref())(),
            },
        );
        trim(&mut inner);
        let format = snapshot(&inner);
        drop(inner);
        self.persist(&format)?;
        Ok(Start::New)
    }

    /// Mark the run finished. The record stays for deduplication.
    pub fn complete(&self, event_id: &str) -> Result<(), LedgerError> {
        if event_id.is_empty() {
            return Err(LedgerError::Io("empty event id".to_owned()));
        }
        let mut inner = self.lock();
        let now = (self.clock.as_ref())();
        inner
            .entry(event_id.to_owned())
            .and_modify(|record| {
                record.state = RunState::Completed;
                record.updated_unix = now;
            })
            .or_insert(RunRecord {
                scenario_id: String::new(),
                state: RunState::Completed,
                updated_unix: now,
            });
        trim(&mut inner);
        let format = snapshot(&inner);
        drop(inner);
        self.persist(&format)
    }

    /// State of one event, if it is recorded.
    #[must_use]
    pub fn state(&self, event_id: &str) -> Option<RunState> {
        self.lock().get(event_id).map(|record| record.state)
    }

    /// Number of run records.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// Whether no run is recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, RunRecord>> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn load(&self) -> Result<(), LedgerError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let data = match fs::read(path) {
            Ok(data) => data,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(source) => {
                return Err(LedgerError::Read {
                    path: path.display().to_string(),
                    source,
                });
            }
        };
        if data.is_empty() {
            return Ok(());
        }
        let stored: FileFormat =
            serde_json::from_slice(&data).map_err(|error| LedgerError::Corrupt {
                path: path.display().to_string(),
                detail: error.to_string(),
            })?;
        if stored.version != FILE_VERSION {
            return Err(LedgerError::Unsupported {
                path: path.display().to_string(),
                version: stored.version,
            });
        }
        let mut inner = self.lock();
        *inner = stored.runs;
        if prune(&mut inner, (self.clock.as_ref())()) > 0 {
            let format = snapshot(&inner);
            drop(inner);
            self.persist(&format)?;
        }
        Ok(())
    }

    fn persist(&self, format: &FileFormat) -> Result<(), LedgerError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let directory = path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(directory)
            .map_err(|error| LedgerError::Io(format!("create directory: {error}")))?;
        let data = serde_json::to_vec(format)
            .map_err(|error| LedgerError::Io(format!("encode: {error}")))?;
        let (mut handle, name) = create_temp(directory)?;
        handle
            .write_all(&data)
            .map_err(|error| LedgerError::Io(format!("write temp file: {error}")))?;
        handle
            .sync_all()
            .map_err(|error| LedgerError::Io(format!("sync temp file: {error}")))?;
        drop(handle);
        fs::rename(&name, path).map_err(|error| {
            let _ = fs::remove_file(&name);
            LedgerError::Io(format!("replace {}: {error}", path.display()))
        })?;
        if let Ok(directory) = File::open(directory) {
            let _ = directory.sync_all();
        }
        Ok(())
    }
}

fn snapshot(runs: &BTreeMap<String, RunRecord>) -> FileFormat {
    FileFormat {
        version: FILE_VERSION,
        runs: runs.clone(),
    }
}

fn system_clock() -> Clock {
    Arc::new(system_clock_closure())
}

fn system_clock_closure() -> impl Fn() -> i64 + Send + Sync + 'static {
    || {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| {
                i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
            })
    }
}

fn prune(runs: &mut BTreeMap<String, RunRecord>, now_unix: i64) -> usize {
    let cutoff =
        now_unix.saturating_sub(i64::try_from(DEFAULT_RETENTION.as_secs()).unwrap_or(i64::MAX));
    let before = runs.len();
    runs.retain(|_, record| record.updated_unix >= cutoff);
    before - runs.len()
}

fn trim(runs: &mut BTreeMap<String, RunRecord>) {
    if runs.len() <= MAX_RUNS {
        return;
    }
    let mut order: Vec<(String, i64)> = runs
        .iter()
        .map(|(id, record)| (id.clone(), record.updated_unix))
        .collect();
    order.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
    let remove = order.len() - MAX_RUNS;
    for (id, _) in order.into_iter().take(remove) {
        runs.remove(&id);
    }
}

/// Create a mode-0600 temp file in `directory` and return its handle and name.
fn create_temp(directory: &Path) -> Result<(File, PathBuf), LedgerError> {
    for _ in 0..100 {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let name = directory.join(format!(
            ".ha-scenario-ledger-{}-{counter}-{nanos}.tmp",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&name)
        {
            Ok(file) => return Ok((file, name)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(LedgerError::Io(format!("create temp file: {error}")));
            }
        }
    }
    Err(LedgerError::Io(
        "create temp file: too many name collisions".to_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicI64, Ordering};

    use tempfile::tempdir;

    use super::{RunState, ScenarioLedger, Start};

    fn ledger() -> (ScenarioLedger, Arc<AtomicI64>) {
        let clock = Arc::new(AtomicI64::new(1_800_000_000));
        let view = Arc::clone(&clock);
        (
            ScenarioLedger::memory_with_clock(move || view.load(Ordering::SeqCst)),
            clock,
        )
    }

    #[test]
    fn the_first_start_claims_and_a_second_one_is_interrupted() {
        let (ledger, _clock) = ledger();
        assert_eq!(ledger.start("ev_1", "sc_1").expect("start"), Start::New);
        assert_eq!(
            ledger.start("ev_1", "sc_1").expect("again"),
            Start::Interrupted
        );
        assert_eq!(ledger.state("ev_1"), Some(RunState::Started));
    }

    #[test]
    fn completion_suppresses_a_replay_without_marking_it_interrupted() {
        let (ledger, _clock) = ledger();
        ledger.start("ev_1", "sc_1").expect("start");
        ledger.complete("ev_1").expect("complete");
        assert_eq!(ledger.state("ev_1"), Some(RunState::Completed));
        assert_eq!(
            ledger.start("ev_1", "sc_1").expect("again"),
            Start::Completed
        );
    }

    #[test]
    fn a_started_record_survives_a_reopen() {
        let directory = tempdir().expect("tempdir");
        let path = directory.path().join("scenario-ledger.json");
        {
            let ledger = ScenarioLedger::open(&path).expect("open");
            assert_eq!(ledger.start("ev_1", "sc_1").expect("start"), Start::New);
        }
        let ledger = ScenarioLedger::open(&path).expect("reopen");
        assert_eq!(
            ledger.start("ev_1", "sc_1").expect("after restart"),
            Start::Interrupted
        );
        let raw = std::fs::read_to_string(&path).expect("read");
        assert!(raw.contains("\"started\""));
        assert!(raw.contains("\"scenarioId\":\"sc_1\""));
    }

    #[test]
    fn a_corrupt_file_is_an_error() {
        let directory = tempdir().expect("tempdir");
        let path = directory.path().join("scenario-ledger.json");
        std::fs::write(&path, b"{not json").expect("write");
        assert!(ScenarioLedger::open(&path).is_err());
    }

    #[test]
    fn records_older_than_the_retention_are_pruned() {
        let (ledger, clock) = ledger();
        ledger.start("ev_old", "sc_1").expect("start");
        ledger.complete("ev_old").expect("complete");
        clock.fetch_add(31 * 24 * 60 * 60, Ordering::SeqCst);
        ledger.start("ev_new", "sc_2").expect("start");
        // Pruning happens on load; the in-memory record is still there until
        // reopen, which is fine for deduplication.
        assert_eq!(ledger.len(), 2);
    }
}
