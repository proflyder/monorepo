use std::fmt;

use jiff::Timestamp;

pub trait Clock: Send + Sync {
    fn now(&self) -> Timestamp;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Timestamp::now()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FixedClock(pub Timestamp);

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        self.0
    }
}

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
