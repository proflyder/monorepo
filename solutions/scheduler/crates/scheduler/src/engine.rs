//! The durable engine loop: one tick per interval plus a periodic purge.
//!
//! The loop owns no audio or network side effects; it only advances schedules
//! and timers into the durable event outbox. Every store call hops to
//! [`tokio::task::spawn_blocking`] because the store talks to the single SQLite
//! writer thread and must never block the async runtime.
//!
//! Readiness mirrors the Go engine: a successful tick within three intervals
//! (at least five seconds) and no newer failure.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::api::EngineStatus;
use crate::domain::Clock;
use crate::store::{Store, StoreError};

/// Default retention purge cadence (one hour, like the Go engine).
pub const DEFAULT_PURGE_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// The last observed tick facts.
#[derive(Debug, Clone, Default)]
struct State {
    last_tick_unix: i64,
    last_attempt_unix: i64,
    last_error_unix: i64,
    last_error: String,
}

/// Owns the tick cadence and the readiness snapshot.
#[derive(Clone)]
pub struct Engine {
    state: Arc<Mutex<State>>,
    clock: Arc<dyn Clock>,
    tick: Duration,
    purge: Duration,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Engine")
            .field("tick", &self.tick)
            .field("purge", &self.purge)
            .finish_non_exhaustive()
    }
}

impl Engine {
    /// Builds an engine. A zero tick or purge interval falls back to the Go
    /// defaults (1s, 1h).
    pub fn new(clock: Arc<dyn Clock>, tick: Duration, purge: Duration) -> Self {
        Self {
            state: Arc::new(Mutex::new(State::default())),
            clock,
            tick: if tick.is_zero() {
                Duration::from_secs(1)
            } else {
                tick
            },
            purge: if purge.is_zero() {
                DEFAULT_PURGE_INTERVAL
            } else {
                purge
            },
        }
    }

    /// Current readiness and last-tick facts for `GetStatus`.
    pub fn status(&self) -> EngineStatus {
        let now = self.clock.now_unix();
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let window = (3 * self.tick).max(Duration::from_secs(5));
        let window = i64::try_from(window.as_secs()).unwrap_or(i64::MAX);
        EngineStatus {
            ready: state.last_tick_unix > 0
                && now.saturating_sub(state.last_tick_unix) <= window
                && state.last_error.is_empty(),
            last_tick_unix: state.last_tick_unix,
            last_tick_attempt_unix: state.last_attempt_unix,
            last_tick_error_unix: state.last_error_unix,
            last_tick_error: state.last_error.clone(),
        }
    }

    /// Runs one engine pass. Failures are recorded so `GetStatus` cannot mask a
    /// broken loop.
    pub async fn tick(&self, store: &Arc<Store>) -> Result<(), StoreError> {
        let now = self.clock.now_unix();
        {
            let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            state.last_attempt_unix = now;
        }
        let store = Arc::clone(store);
        let result = tokio::task::spawn_blocking(move || store.tick(now)).await;
        match result {
            Ok(Ok(stats)) => {
                let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
                state.last_tick_unix = now;
                state.last_error.clear();
                state.last_error_unix = 0;
                drop(state);
                if stats.fired > 0
                    || stats.skipped > 0
                    || stats.timers > 0
                    || stats.expired > 0
                    || stats.leases_released > 0
                {
                    tracing::info!(
                        fired = stats.fired,
                        skipped = stats.skipped,
                        timers = stats.timers,
                        expired = stats.expired,
                        leases_released = stats.leases_released,
                        "scheduler tick"
                    );
                }
                Ok(())
            }
            Ok(Err(error)) => {
                self.record_failure(now, &error.to_string());
                Err(error)
            }
            Err(error) => {
                tracing::error!(%error, "scheduler tick task failed");
                let error = StoreError::Sqlite(home_sqlite::SqliteError::Panicked);
                self.record_failure(now, &error.to_string());
                Err(error)
            }
        }
    }

    /// Runs the loop until `cancel` fires.
    pub async fn run(&self, store: Arc<Store>, cancel: CancellationToken) {
        // Tick once immediately so readiness and due work do not wait a full
        // interval after startup.
        if let Err(error) = self.tick(&store).await
            && !cancel.is_cancelled()
        {
            tracing::error!(%error, "scheduler initial tick failed");
        }
        let mut last_purge = self.clock.now_unix();
        let mut interval = tokio::time::interval(self.tick);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // The first tick of a fresh interval completes immediately; consume it
        // so the loop waits one interval after the startup tick.
        interval.tick().await;
        loop {
            tokio::select! {
                () = cancel.cancelled() => return,
                _ = interval.tick() => {}
            }
            match self.tick(&store).await {
                Ok(()) => {}
                Err(_) if cancel.is_cancelled() => return,
                Err(error) => tracing::error!(%error, "scheduler tick failed"),
            }
            let now = self.clock.now_unix();
            if now.saturating_sub(last_purge) >= i64::try_from(self.purge.as_secs()).unwrap_or(0) {
                let store = Arc::clone(&store);
                match tokio::task::spawn_blocking(move || store.purge(now)).await {
                    Ok(Ok(removed)) if removed > 0 => {
                        tracing::info!(events = removed, "scheduler purged terminal events");
                    }
                    Ok(Ok(_)) => {}
                    Ok(Err(error)) => tracing::error!(%error, "scheduler purge failed"),
                    Err(error) => tracing::error!(%error, "scheduler purge task failed"),
                }
                last_purge = now;
            }
        }
    }

    fn record_failure(&self, now: i64, detail: &str) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.last_error_unix = now;
        state.last_error = truncate(detail, 200);
    }
}

fn truncate(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::domain::FixedClock;
    use crate::store::Config;

    #[tokio::test]
    async fn readiness_reflects_failures_on_the_same_clock() {
        let frozen = 1_767_268_800;
        let clock = Arc::new(FixedClock::new(frozen));
        let store = Arc::new(Store::open_in_memory(Config::default()).expect("open store"));
        let engine = Engine::new(clock, Duration::from_secs(1), DEFAULT_PURGE_INTERVAL);
        assert!(!engine.status().ready, "ready before the first tick");

        engine.tick(&store).await.expect("tick");
        assert!(engine.status().ready, "ready after a successful tick");

        let store = Arc::try_unwrap(store).expect("sole owner").close();
        assert!(store.is_ok());
    }

    #[tokio::test]
    async fn failed_tick_drops_readiness_and_records_the_error() {
        let frozen = 1_767_268_800;
        let clock = Arc::new(FixedClock::new(frozen));
        let store = Arc::new(Store::open_in_memory(Config::default()).expect("open store"));
        let engine = Engine::new(clock, Duration::from_secs(1), DEFAULT_PURGE_INTERVAL);
        engine.tick(&store).await.expect("tick");
        assert!(engine.status().ready);

        // A failure at the same clock instant must drop readiness: the contract
        // is the explicit error flag, not a timestamp comparison.
        engine.record_failure(frozen, "tick failed");
        let status = engine.status();
        assert!(!status.ready);
        assert_eq!(status.last_tick_error, "tick failed");
        assert_eq!(status.last_tick_unix, frozen);
    }

    #[test]
    fn readiness_window_covers_three_ticks_with_a_five_second_floor() {
        let clock = Arc::new(FixedClock::new(1_000));
        let engine = Engine::new(clock, Duration::from_millis(100), DEFAULT_PURGE_INTERVAL);
        {
            let mut state = engine.state.lock().expect("state");
            state.last_tick_unix = 997;
        }
        assert!(engine.status().ready, "a 5s floor applies to fast ticks");
        {
            let mut state = engine.state.lock().expect("state");
            state.last_tick_unix = 990;
        }
        assert!(!engine.status().ready, "an old tick is not ready");
    }
}
