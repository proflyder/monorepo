//! Clock boundary: the process uses [`SystemClock`], tests use [`FixedClock`].

use std::fmt;

use jiff::Timestamp;

/// Wall clock used by the pipeline, the HTTP envelope and the schedules.
pub trait Clock: Send + Sync {
    fn now(&self) -> Timestamp;
}

/// The real clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Timestamp::now()
    }
}

/// A clock frozen at one instant.
#[derive(Debug, Clone, Copy)]
pub struct FixedClock(pub Timestamp);

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        self.0
    }
}

/// Debug formatting for an `Arc<dyn Clock>` holder.
pub(crate) fn debug_clock(clock: &dyn Clock) -> impl fmt::Debug + '_ {
    struct ClockDebug<'a>(&'a dyn Clock);
    impl fmt::Debug for ClockDebug<'_> {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter
                .debug_struct("Clock")
                .field("now", &self.0.now().to_string())
                .finish()
        }
    }
    ClockDebug(clock)
}
