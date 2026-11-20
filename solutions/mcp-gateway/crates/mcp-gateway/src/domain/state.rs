use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use jiff::Timestamp;

use super::clock::Clock;

pub const ACTION_ROLLOUT_RESTART: &str = "rollout-restart";

pub const LAST_RESTART_ANNOTATION: &str = "mcp-gateway/last-restart";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Action {
    pub service_id: String,
    pub namespace: String,
    pub workload_kind: String,
    pub workload_name: String,
    pub kind: String,
    pub self_disrupting: bool,
    pub delay: Duration,
    pub warning: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingAction {
    pub action: Action,
    pub created_at: Timestamp,
}

pub struct PendingStore {
    ttl: Duration,
    clock: Arc<dyn Clock>,
    pending: Mutex<Option<PendingAction>>,
}

impl PendingStore {
    pub fn new(ttl: Duration, clock: Arc<dyn Clock>) -> Self {
        Self {
            ttl,
            clock,
            pending: Mutex::new(None),
        }
    }

    #[must_use]
    pub fn ttl(&self) -> Duration {
        self.ttl
    }

    pub fn put(&self, action: Action) -> Timestamp {
        let created_at = self.clock.now();
        let mut pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
        *pending = Some(PendingAction { action, created_at });
        created_at
    }

    pub fn peek(&self) -> Option<PendingAction> {
        self.lock_and_peek()
    }

    pub fn take(&self) -> Option<PendingAction> {
        let mut pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
        let current = pending.take()?;
        drop(pending);
        if self.expired(&current) {
            return None;
        }
        Some(current)
    }

    pub fn clear(&self) {
        let mut pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
        *pending = None;
    }

    #[must_use]
    pub fn expires_at(&self, created_at: Timestamp) -> Timestamp {
        let ttl = i128::try_from(self.ttl.as_nanos()).unwrap_or(i128::MAX);
        Timestamp::from_nanosecond(created_at.as_nanosecond().saturating_add(ttl))
            .unwrap_or(Timestamp::MAX)
    }

    fn lock_and_peek(&self) -> Option<PendingAction> {
        let mut pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
        let current = pending.as_ref()?;
        if self.expired(current) {
            *pending = None;
            drop(pending);
            return None;
        }
        let current = current.clone();
        drop(pending);
        Some(current)
    }

    fn expired(&self, pending: &PendingAction) -> bool {
        let ttl = i128::try_from(self.ttl.as_nanos()).unwrap_or(i128::MAX);
        let elapsed = self
            .clock
            .now()
            .as_nanosecond()
            .saturating_sub(pending.created_at.as_nanosecond());
        elapsed >= ttl
    }
}

pub struct KeyedStore<T> {
    ttl: Duration,
    clock: Arc<dyn Clock>,
    entries: Mutex<HashMap<String, (T, Timestamp)>>,
}

impl<T> KeyedStore<T> {
    pub fn new(ttl: Duration, clock: Arc<dyn Clock>) -> Self {
        Self {
            ttl,
            clock,
            entries: Mutex::new(HashMap::new()),
        }
    }

    #[must_use]
    pub fn ttl(&self) -> Duration {
        self.ttl
    }

    pub fn put(&self, key: &str, value: T) -> Timestamp {
        let created_at = self.clock.now();
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        entries.insert(key.to_owned(), (value, created_at));
        created_at
    }

    pub fn peek(&self, key: &str) -> Option<T>
    where
        T: Clone,
    {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = entries.get(key)?;
        if self.expired(entry.1) {
            entries.remove(key);
            drop(entries);
            return None;
        }
        let value = entry.0.clone();
        drop(entries);
        Some(value)
    }

    pub fn take(&self, key: &str) -> Option<T> {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = entries.remove(key)?;
        drop(entries);
        if self.expired(entry.1) {
            return None;
        }
        Some(entry.0)
    }

    pub fn clear(&self, key: &str) {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        entries.remove(key);
    }

    fn expired(&self, created_at: Timestamp) -> bool {
        let ttl = i128::try_from(self.ttl.as_nanos()).unwrap_or(i128::MAX);
        let elapsed = self
            .clock
            .now()
            .as_nanosecond()
            .saturating_sub(created_at.as_nanosecond());
        elapsed >= ttl
    }
}

#[derive(Debug, Default)]
pub struct AgentMode {
    thread_id: Mutex<Option<String>>,
}

impl AgentMode {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enter(&self, thread_id: &str) {
        let mut current = self
            .thread_id
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *current = Some(thread_id.to_owned());
    }

    pub fn exit(&self) {
        let mut current = self
            .thread_id
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *current = None;
    }

    #[must_use]
    pub fn active(&self) -> Option<String> {
        self.thread_id
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn clear_if(&self, thread_id: &str) {
        let mut current = self
            .thread_id
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if current.as_deref() == Some(thread_id) {
            *current = None;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cooldown {
    pub elapsed_seconds: i64,
    pub remaining_seconds: i64,
}

#[must_use]
pub fn restart_cooldown(
    last_restart: Option<Timestamp>,
    now: Timestamp,
    cooldown: Duration,
) -> Option<Cooldown> {
    let last = last_restart?;
    let elapsed = now.as_second().saturating_sub(last.as_second());
    let cooldown_seconds = i64::try_from(cooldown.as_secs()).unwrap_or(i64::MAX);
    if elapsed < cooldown_seconds {
        return Some(Cooldown {
            elapsed_seconds: elapsed,
            remaining_seconds: cooldown_seconds - elapsed,
        });
    }
    None
}

#[must_use]
pub fn last_restart_of(annotations: &BTreeMap<String, String>) -> Option<Timestamp> {
    annotations
        .get(LAST_RESTART_ANNOTATION)?
        .trim()
        .parse::<Timestamp>()
        .ok()
}

#[must_use]
pub fn human_duration(seconds: i64) -> String {
    if seconds >= 3_600 && seconds % 3_600 == 0 {
        format!("{} ч", seconds / 3_600)
    } else if seconds >= 60 {
        format!("{} мин", seconds / 60)
    } else {
        format!("{seconds} с")
    }
}
