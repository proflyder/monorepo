//! Time source abstraction.
//!
//! The domain never reads the ambient clock directly: store and engine
//! operations take `now` as an argument or ask a [`Clock`], which keeps tick
//! scenarios deterministic.

use std::time::{SystemTime, UNIX_EPOCH};

/// Wall-clock time in Unix seconds.
pub trait Clock: Send + Sync {
    /// Current time as Unix seconds.
    fn now_unix(&self) -> i64;
}

/// A clock frozen at a fixed instant, for tests and golden fixtures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedClock {
    now_unix: i64,
}

impl FixedClock {
    /// Creates a clock frozen at `now_unix`.
    #[must_use]
    pub const fn new(now_unix: i64) -> Self {
        Self { now_unix }
    }
}

impl Clock for FixedClock {
    fn now_unix(&self) -> i64 {
        self.now_unix
    }
}

/// The process wall clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_unix(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| {
                i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
            })
    }
}
