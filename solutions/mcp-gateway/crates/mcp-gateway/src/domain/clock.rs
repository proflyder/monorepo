//! Injectable time source.
//!
//! The domain never reads the ambient clock directly: reports, pending
//! actions and the restart cooldown ask a [`Clock`], so tests and golden
//! vectors can freeze time.

use jiff::Timestamp;

/// Wall-clock source of the domain.
pub trait Clock: Send + Sync {
    /// Current instant.
    fn now(&self) -> Timestamp;
}

/// A clock frozen at a fixed instant, for tests and golden fixtures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedClock {
    now: Timestamp,
}

impl FixedClock {
    /// Creates a clock frozen at `now`.
    #[must_use]
    pub const fn new(now: Timestamp) -> Self {
        Self { now }
    }

    /// The frozen instant.
    #[must_use]
    pub const fn instant(&self) -> Timestamp {
        self.now
    }
}

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        self.now
    }
}

/// The process wall clock.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Timestamp::now()
    }
}
