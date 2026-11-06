//! Durable receipt ledger for events whose Home Assistant side effect already
//! succeeded.
//!
//! The file is the version-1 JSON contract the Go bridge writes on the same
//! PVC (`/data/ledger.json`): `{"version":1,"receipts":{...},"activeAlarm":{...}}`.
//! Writes are atomic (`temp + fsync + rename + fsync(dir)`), the retention
//! window is 30 days and the ledger never exceeds 20 000 receipts.
//!
//! The ledger narrows the duplicate window in at-least-once delivery; it cannot
//! make delivery exactly once, because a crash between the HA call and the
//! ledger write (or between the ledger write and the scheduler ack) still
//! re-delivers the event.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// How long receipts are kept for deduplication.
pub const DEFAULT_RETENTION: Duration = Duration::from_secs(30 * 24 * 60 * 60);
/// Hard cap even without pruning.
pub const MAX_RECEIPTS: usize = 20_000;
/// The only supported on-disk version.
pub const FILE_VERSION: i64 = 1;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// One successfully executed event. It never contains tokens or payloads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    #[serde(rename = "eventId", default)]
    pub event_id: String,
    #[serde(rename = "sourceId", default)]
    pub source_id: String,
    #[serde(rename = "action", default)]
    pub action: String,
    #[serde(rename = "script", default)]
    pub script: String,
    #[serde(rename = "completedUnix", default)]
    pub completed_unix: i64,
}

/// The alarm occurrence whose sound this bridge started and may stop.
///
/// Occurrence and due time come from the scheduler event, not bridge wall-clock
/// time, so a late STOP for an older occurrence (or one before a snoozed
/// re-fire) is refused even when the source id still matches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveAlarm {
    #[serde(rename = "sourceId", default)]
    pub source_id: String,
    #[serde(rename = "eventId", default)]
    pub event_id: String,
    #[serde(rename = "occurrenceUnix", default, skip_serializing_if = "is_zero")]
    pub occurrence_unix: i64,
    #[serde(rename = "fireDueUnix", default, skip_serializing_if = "is_zero")]
    pub fire_due_unix: i64,
    #[serde(rename = "sinceUnix", default)]
    pub since_unix: i64,
}

fn is_zero(value: &i64) -> bool {
    *value == 0
}

#[derive(Debug, Serialize, Deserialize)]
struct FileFormat {
    #[serde(default)]
    version: i64,
    #[serde(default)]
    receipts: BTreeMap<String, Receipt>,
    #[serde(
        rename = "activeAlarm",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    active_alarm: Option<ActiveAlarm>,
}

/// A ledger failure. Messages never echo a token or a payload.
#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    #[error("ledger: read {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("ledger: {path} is corrupt: {detail}")]
    Corrupt { path: String, detail: String },
    #[error("ledger: {path} has unsupported version {version}")]
    Unsupported { path: String, version: i64 },
    #[error("ledger: empty event id")]
    EmptyEventId,
    #[error("ledger: empty active alarm source id")]
    EmptyActiveSource,
    #[error("ledger: encode: {0}")]
    Encode(String),
    #[error("ledger: {0}")]
    Io(String),
}

#[derive(Debug, Default)]
struct Inner {
    receipts: BTreeMap<String, Receipt>,
    active: Option<ActiveAlarm>,
}

impl Inner {
    fn snapshot(&self) -> FileFormat {
        FileFormat {
            version: FILE_VERSION,
            receipts: self.receipts.clone(),
            active_alarm: self.active.clone(),
        }
    }
}

/// A concurrency-safe receipt ledger.
pub struct Ledger {
    path: Option<PathBuf>,
    inner: Mutex<Inner>,
    clock: Clock,
}

impl std::fmt::Debug for Ledger {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Ledger")
            .field("path", &self.path)
            .field(
                "len",
                &self.inner.lock().map_or(0, |inner| inner.receipts.len()),
            )
            .finish()
    }
}

impl Ledger {
    /// Open a file-backed ledger, creating it on the first write.
    ///
    /// A corrupt or newer file is an error: silently starting empty could
    /// duplicate a Home Assistant side effect.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, LedgerError> {
        let clock = system_clock();
        Self::open_with_clock(path, move || (clock.as_ref())())
    }

    /// Open with an injected clock (tests and golden vectors).
    pub fn open_with_clock(
        path: impl AsRef<Path>,
        clock: impl Fn() -> i64 + Send + Sync + 'static,
    ) -> Result<Self, LedgerError> {
        let path = path.as_ref().to_path_buf();
        let ledger = Self {
            path: Some(path),
            inner: Mutex::new(Inner::default()),
            clock: Arc::new(clock),
        };
        ledger.load()?;
        Ok(ledger)
    }

    /// An in-memory ledger: deduplication lasts only for this process.
    #[must_use]
    pub fn memory() -> Self {
        Self {
            path: None,
            inner: Mutex::new(Inner::default()),
            clock: system_clock(),
        }
    }

    /// An in-memory ledger with an injected clock.
    #[must_use]
    pub fn memory_with_clock(clock: impl Fn() -> i64 + Send + Sync + 'static) -> Self {
        Self {
            path: None,
            inner: Mutex::new(Inner::default()),
            clock: Arc::new(clock),
        }
    }

    /// Backing file, or `None` for an in-memory ledger.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Whether the event side effect was already recorded.
    #[must_use]
    pub fn seen(&self, event_id: &str) -> bool {
        if event_id.is_empty() {
            return false;
        }
        self.lock().receipts.contains_key(event_id)
    }

    /// Store one receipt and persist atomically. The in-memory entry is kept
    /// even when persistence fails so this process still deduplicates.
    pub fn record(&self, receipt: Receipt) -> Result<(), LedgerError> {
        if receipt.event_id.is_empty() {
            return Err(LedgerError::EmptyEventId);
        }
        let receipt = self.stamp(receipt);
        let mut inner = self.lock();
        inner.receipts.insert(receipt.event_id.clone(), receipt);
        trim(&mut inner);
        let format = Inner::snapshot(&inner);
        drop(inner);
        self.persist_format(&format)
    }

    /// The alarm source marked as the active sound, if any.
    #[must_use]
    pub fn active_alarm(&self) -> Option<ActiveAlarm> {
        self.lock()
            .active
            .as_ref()
            .filter(|active| !active.source_id.is_empty())
            .cloned()
    }

    /// Atomically store an accepted alarm FIRE receipt and mark the source as
    /// the active alarm.
    pub fn record_start(&self, receipt: Receipt, active: ActiveAlarm) -> Result<(), LedgerError> {
        if receipt.event_id.is_empty() {
            return Err(LedgerError::EmptyEventId);
        }
        if active.source_id.is_empty() {
            return Err(LedgerError::EmptyActiveSource);
        }
        let receipt = self.stamp(receipt);
        let mut marker = active;
        if marker.since_unix == 0 {
            marker.since_unix = (self.clock.as_ref())();
        }
        let mut inner = self.lock();
        inner.receipts.insert(receipt.event_id.clone(), receipt);
        inner.active = Some(marker);
        trim(&mut inner);
        let format = Inner::snapshot(&inner);
        drop(inner);
        self.persist_format(&format)
    }

    /// Store an accepted alarm STOP receipt and clear the active marker only
    /// when it still refers to `expect_source_id`.
    ///
    /// Returns whether the marker was cleared; a concurrent newer alarm keeps
    /// its marker.
    pub fn record_stop(
        &self,
        receipt: Receipt,
        expect_source_id: &str,
    ) -> Result<bool, LedgerError> {
        if receipt.event_id.is_empty() {
            return Err(LedgerError::EmptyEventId);
        }
        let receipt = self.stamp(receipt);
        let mut inner = self.lock();
        inner.receipts.insert(receipt.event_id.clone(), receipt);
        let cleared = inner
            .active
            .as_ref()
            .is_some_and(|active| active.source_id == expect_source_id);
        if cleared {
            inner.active = None;
        }
        trim(&mut inner);
        let format = Inner::snapshot(&inner);
        drop(inner);
        self.persist_format(&format)?;
        Ok(cleared)
    }

    /// Remove receipts older than the retention window.
    pub fn prune(&self) -> Result<(), LedgerError> {
        if self.path.is_none() {
            return Ok(());
        }
        let mut inner = self.lock();
        if prune(&mut inner, (self.clock.as_ref())()) > 0 {
            let format = Inner::snapshot(&inner);
            drop(inner);
            self.persist_format(&format)?;
        }
        Ok(())
    }

    /// Current clock value used for receipts and the active marker.
    #[must_use]
    pub fn now_unix(&self) -> i64 {
        (self.clock.as_ref())()
    }

    /// Number of stored receipts.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lock().receipts.len()
    }

    /// Whether no receipt is stored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn stamp(&self, mut receipt: Receipt) -> Receipt {
        if receipt.completed_unix == 0 {
            receipt.completed_unix = (self.clock.as_ref())();
        }
        receipt
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
        for (id, mut receipt) in stored.receipts {
            if receipt.event_id.is_empty() {
                receipt.event_id.clone_from(&id);
            }
            inner.receipts.insert(id, receipt);
        }
        if let Some(active) = stored.active_alarm
            && !active.source_id.is_empty()
        {
            inner.active = Some(active);
        }
        if prune(&mut inner, (self.clock.as_ref())()) > 0 {
            let format = Inner::snapshot(&inner);
            drop(inner);
            self.persist_format(&format)?;
        }
        Ok(())
    }

    fn persist_format(&self, format: &FileFormat) -> Result<(), LedgerError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let directory = path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(directory)
            .map_err(|error| LedgerError::Io(format!("create directory: {error}")))?;
        let data =
            serde_json::to_vec(format).map_err(|error| LedgerError::Encode(error.to_string()))?;

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

fn system_clock() -> Clock {
    Arc::new(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| {
                i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
            })
    })
}

fn prune(inner: &mut Inner, now_unix: i64) -> usize {
    let cutoff =
        now_unix.saturating_sub(i64::try_from(DEFAULT_RETENTION.as_secs()).unwrap_or(i64::MAX));
    let before = inner.receipts.len();
    inner
        .receipts
        .retain(|_, receipt| receipt.completed_unix >= cutoff);
    before - inner.receipts.len()
}

fn trim(inner: &mut Inner) {
    if inner.receipts.len() <= MAX_RECEIPTS {
        return;
    }
    let mut order: Vec<(String, i64)> = inner
        .receipts
        .iter()
        .map(|(id, receipt)| (id.clone(), receipt.completed_unix))
        .collect();
    order.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
    let remove = order.len() - MAX_RECEIPTS;
    for (id, _) in order.into_iter().take(remove) {
        inner.receipts.remove(&id);
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
            ".ha-bridge-ledger-{}-{counter}-{nanos}.tmp",
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
    use std::os::unix::fs::PermissionsExt as _;

    use tempfile::tempdir;

    use super::{ActiveAlarm, Ledger, Receipt};

    fn receipt(event_id: &str, completed_unix: i64) -> Receipt {
        Receipt {
            event_id: event_id.to_owned(),
            source_id: "alarm-1".to_owned(),
            action: "FIRE".to_owned(),
            script: "alarm_start".to_owned(),
            completed_unix,
        }
    }

    #[test]
    fn record_persists_with_mode_0600_and_reopens() {
        let directory = tempdir().expect("tempdir");
        let path = directory.path().join("state/ledger.json");
        let ledger = Ledger::open_with_clock(&path, || 1_700_000_000).expect("open");
        assert!(!ledger.seen("evt-1"));
        ledger.record(receipt("evt-1", 0)).expect("record");
        assert!(ledger.seen("evt-1"));

        let mode = std::fs::metadata(&path).expect("stat").permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "ledger mode");

        let reopened = Ledger::open_with_clock(&path, || 1_700_000_000).expect("reopen");
        assert!(reopened.seen("evt-1"));
        assert_eq!(reopened.len(), 1);

        let leftovers: Vec<_> = std::fs::read_dir(directory.path().join("state"))
            .expect("read dir")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".ha-bridge-ledger-")
            })
            .collect();
        assert!(leftovers.is_empty(), "temporary files left behind");
    }

    #[test]
    fn active_alarm_is_cleared_only_on_a_matching_stop() {
        let ledger = Ledger::memory_with_clock(|| 1_700_000_000);
        let active = ActiveAlarm {
            source_id: "alarm-a".to_owned(),
            event_id: "evt-fire".to_owned(),
            occurrence_unix: 10,
            fire_due_unix: 20,
            since_unix: 0,
        };
        ledger
            .record_start(receipt("evt-fire", 0), active)
            .expect("record start");
        let stored = ledger.active_alarm().expect("active");
        assert_eq!(stored.since_unix, 1_700_000_000);

        let cleared = ledger
            .record_stop(receipt("evt-other-stop", 0), "alarm-b")
            .expect("record stop");
        assert!(!cleared);
        assert!(ledger.active_alarm().is_some());

        let cleared = ledger
            .record_stop(receipt("evt-stop", 0), "alarm-a")
            .expect("record stop");
        assert!(cleared);
        assert!(ledger.active_alarm().is_none());
    }

    #[test]
    fn empty_ids_are_rejected_before_io() {
        let ledger = Ledger::memory();
        assert!(ledger.record(receipt("", 0)).is_err());
        assert!(
            ledger
                .record_start(
                    receipt("evt", 0),
                    ActiveAlarm {
                        source_id: String::new(),
                        event_id: "evt".to_owned(),
                        occurrence_unix: 0,
                        fire_due_unix: 0,
                        since_unix: 0,
                    },
                )
                .is_err()
        );
    }
}
