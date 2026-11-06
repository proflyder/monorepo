//! Time source for the domain: production uses the wall clock, tests inject a
//! fixed one.

use time::OffsetDateTime;

/// Wall-clock abstraction. Every decision that depends on "now" takes a
/// `&dyn Clock`, so the domain stays deterministic in tests.
pub trait Clock: Send + Sync {
    fn now(&self) -> OffsetDateTime;
}

/// The system clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }
}
