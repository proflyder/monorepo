use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use time::OffsetDateTime;

/// 60 requests per minute per machine and per source IP.
pub const CHALLENGE_LIMIT: u32 = 60;
pub const RATE_WINDOW: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy)]
struct Entry {
    start: OffsetDateTime,
    count: u32,
}

/// Small fixed-window counter. Deliberately in memory: a restart only resets
/// the budget, which cannot bypass a grant.
#[derive(Debug)]
pub struct RateLimiter {
    inner: Mutex<HashMap<String, Entry>>,
    limit: u32,
    window: time::Duration,
}

impl RateLimiter {
    #[must_use]
    pub fn new(limit: u32, window: Duration) -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            limit,
            window: time::Duration::try_from(window).expect("rate window fits in time::Duration"),
        }
    }

    #[must_use]
    pub fn challenge() -> Self {
        Self::new(CHALLENGE_LIMIT, RATE_WINDOW)
    }

    /// Distinct prefixes keep the per-endpoint budgets separate
    /// (`challenge:` and `claim:`).
    pub fn allow(&self, key: &str, now: OffsetDateTime) -> bool {
        let mut entries = self.inner.lock().expect("limiter lock");
        match entries.get_mut(key) {
            Some(entry) if now - entry.start < self.window => {
                if entry.count >= self.limit {
                    false
                } else {
                    entry.count += 1;
                    true
                }
            }
            _ => {
                entries.insert(
                    key.to_owned(),
                    Entry {
                        start: now,
                        count: 1,
                    },
                );
                true
            }
        }
    }

    /// Drops stale windows.
    pub fn prune(&self, now: OffsetDateTime) {
        let mut entries = self.inner.lock().expect("limiter lock");
        entries.retain(|_, entry| now - entry.start < self.window);
    }
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::*;

    #[test]
    fn fixed_window_limits_each_key() {
        let limiter = RateLimiter::new(2, RATE_WINDOW);
        let now = datetime!(2026-11-03 12:00:00 UTC);
        assert!(limiter.allow("challenge:machine:komp", now));
        assert!(limiter.allow("challenge:machine:komp", now));
        assert!(!limiter.allow("challenge:machine:komp", now));
        assert!(limiter.allow("claim:machine:komp", now));
        assert!(limiter.allow("challenge:ip:192.0.2.1", now));
        assert!(limiter.allow("challenge:machine:komp", now + RATE_WINDOW));
    }
}
