use time::OffsetDateTime;

/// Time source. Production uses [`SystemClock`]; tests use a fixed clock.
pub trait Clock: Send + Sync + 'static {
    fn now(&self) -> OffsetDateTime;
}

/// Wall clock in UTC.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }
}
