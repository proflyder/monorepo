//! In-memory gateway state: pending actions, the agent-mode switch and the
//! restart cooldown.
//!
//! Ports `internal/{pending,agentmode}` and the cooldown half of
//! `internal/tools/fix.go`. State lives in the process on purpose: a pending
//! action is short-lived and bound to one replica, and the agent-mode thread is
//! a routing hint that must disappear on restart rather than survive as stale
//! configuration.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use jiff::Timestamp;

use super::clock::Clock;

/// The only action kind the gateway can execute.
pub const ACTION_ROLLOUT_RESTART: &str = "rollout-restart";

/// Workload annotation holding the last successful restart time (RFC3339).
pub const LAST_RESTART_ANNOTATION: &str = "mcp-gateway/last-restart";

/// The single confirmed-later action of the gateway.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Action {
    /// Catalog id the action targets.
    pub service_id: String,
    /// Namespace of the object to patch.
    pub namespace: String,
    /// Workload kind of the object to patch.
    pub workload_kind: String,
    /// Workload name of the object to patch.
    pub workload_name: String,
    /// Always [`ACTION_ROLLOUT_RESTART`] today; kept explicit for audit output.
    pub kind: String,
    /// Defers execution so the reply can be spoken first.
    pub self_disrupting: bool,
    /// Delay used when `self_disrupting` is true.
    pub delay: Duration,
    /// Catalog warning shown at prepare time.
    pub warning: String,
}

/// A pending action plus the instant it was stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingAction {
    /// The stored action.
    pub action: Action,
    /// When the action was prepared.
    pub created_at: Timestamp,
}

/// Keeps at most one pending action with a TTL.
pub struct PendingStore {
    ttl: Duration,
    clock: Arc<dyn Clock>,
    pending: Mutex<Option<PendingAction>>,
}

impl PendingStore {
    /// Builds a store with the given TTL and clock.
    pub fn new(ttl: Duration, clock: Arc<dyn Clock>) -> Self {
        Self {
            ttl,
            clock,
            pending: Mutex::new(None),
        }
    }

    /// The configured lifetime of a pending action.
    #[must_use]
    pub fn ttl(&self) -> Duration {
        self.ttl
    }

    /// Replaces any previous pending action and returns its creation time.
    pub fn put(&self, action: Action) -> Timestamp {
        let created_at = self.clock.now();
        let mut pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
        *pending = Some(PendingAction { action, created_at });
        created_at
    }

    /// Returns the pending action without consuming it. Expired actions are
    /// dropped and reported as absent.
    pub fn peek(&self) -> Option<PendingAction> {
        self.lock_and_peek()
    }

    /// Returns and clears the pending action. Expired actions are dropped.
    pub fn take(&self) -> Option<PendingAction> {
        let mut pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
        let current = pending.take()?;
        drop(pending);
        if self.expired(&current) {
            return None;
        }
        Some(current)
    }

    /// Drops any pending action.
    pub fn clear(&self) {
        let mut pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
        *pending = None;
    }

    /// The instant a pending action prepared at `created_at` expires.
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

/// Stores at most one value per key with a shared TTL.
pub struct KeyedStore<T> {
    ttl: Duration,
    clock: Arc<dyn Clock>,
    entries: Mutex<HashMap<String, (T, Timestamp)>>,
}

impl<T> KeyedStore<T> {
    /// Builds a keyed store with the given TTL and clock.
    pub fn new(ttl: Duration, clock: Arc<dyn Clock>) -> Self {
        Self {
            ttl,
            clock,
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// The configured lifetime of one entry.
    #[must_use]
    pub fn ttl(&self) -> Duration {
        self.ttl
    }

    /// Replaces any previous value under `key` and returns its creation time.
    pub fn put(&self, key: &str, value: T) -> Timestamp {
        let created_at = self.clock.now();
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        entries.insert(key.to_owned(), (value, created_at));
        created_at
    }

    /// Returns the value under `key` without consuming it. Expired values are
    /// dropped and reported as absent.
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

    /// Returns and clears the value under `key`. Expired values are dropped.
    pub fn take(&self, key: &str) -> Option<T> {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = entries.remove(key)?;
        drop(entries);
        if self.expired(entry.1) {
            return None;
        }
        Some(entry.0)
    }

    /// Drops the value under `key`.
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

/// The single active conversation thread of the voice path.
#[derive(Debug, Default)]
pub struct AgentMode {
    thread_id: Mutex<Option<String>>,
}

impl AgentMode {
    /// Builds an empty state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Makes `thread_id` the active conversation.
    pub fn enter(&self, thread_id: &str) {
        let mut current = self
            .thread_id
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *current = Some(thread_id.to_owned());
    }

    /// Clears the active conversation.
    pub fn exit(&self) {
        let mut current = self
            .thread_id
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *current = None;
    }

    /// The active thread, if any.
    #[must_use]
    pub fn active(&self) -> Option<String> {
        self.thread_id
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Clears the state only when it still points at `thread_id`, so a newer
    /// conversation is not dropped by a stale failure.
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

/// How long ago a service was restarted and how long the cooldown still lasts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cooldown {
    /// Seconds since the last restart.
    pub elapsed_seconds: i64,
    /// Seconds left before a restart is allowed again.
    pub remaining_seconds: i64,
}

/// Returns the active cooldown, or `None` when a restart is allowed.
///
/// The last restart comes from the [`LAST_RESTART_ANNOTATION`] workload
/// annotation, so the cooldown survives a gateway restart and is shared by all
/// replicas that read the same object.
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

/// Reads the last restart annotation.
#[must_use]
pub fn last_restart_of(annotations: &BTreeMap<String, String>) -> Option<Timestamp> {
    annotations
        .get(LAST_RESTART_ANNOTATION)?
        .trim()
        .parse::<Timestamp>()
        .ok()
}

/// Formats a duration the way the restart flow speaks it.
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
