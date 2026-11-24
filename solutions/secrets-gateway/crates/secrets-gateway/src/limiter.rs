use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use tokio::time::Instant;

pub const FAILURE_LIMIT: u32 = 5;
pub const BASE_BLOCK: Duration = Duration::from_secs(60);
pub const MAX_BLOCK: Duration = Duration::from_secs(3600);
pub const GLOBAL_FAILURE_LIMIT: u32 = 10;
pub const GLOBAL_WINDOW: Duration = Duration::from_secs(60);
pub const GLOBAL_BLOCK: Duration = Duration::from_secs(300);
const MAX_BACKOFF_EXPONENT: u32 = 6;

#[derive(Debug, Clone, Default)]
struct Attempts {
    failures: u32,
    blocked_until: Option<Instant>,
}

#[derive(Debug, Clone)]
struct GlobalAttempts {
    window_start: Instant,
    failures: u32,
    blocked_until: Option<Instant>,
}

pub struct Limiter {
    attempts: Mutex<HashMap<String, Attempts>>,
    global: Mutex<GlobalAttempts>,
}

impl Limiter {
    #[must_use]
    pub fn new() -> Self {
        Self {
            attempts: Mutex::new(HashMap::new()),
            global: Mutex::new(GlobalAttempts {
                window_start: Instant::now(),
                failures: 0,
                blocked_until: None,
            }),
        }
    }

    pub fn check(&self, key: &str, now: Instant) -> Result<(), u64> {
        let mut global = self.global.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(until) = global.blocked_until {
            if now < until {
                return Err(remaining_seconds(until, now));
            }
            global.blocked_until = None;
            global.failures = 0;
            global.window_start = now;
        }
        if now.duration_since(global.window_start) > GLOBAL_WINDOW {
            global.failures = 0;
            global.window_start = now;
        }
        drop(global);
        let mut attempts = self.attempts.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = attempts.entry(key.to_owned()).or_default();
        let Some(until) = entry.blocked_until else {
            return Ok(());
        };
        if now < until {
            return Err(remaining_seconds(until, now));
        }
        entry.blocked_until = None;
        drop(attempts);
        Ok(())
    }

    pub fn failure(&self, key: &str, now: Instant) -> Option<u64> {
        let mut global = self.global.lock().unwrap_or_else(PoisonError::into_inner);
        if now.duration_since(global.window_start) > GLOBAL_WINDOW {
            global.failures = 0;
            global.window_start = now;
        }
        global.failures = global.failures.saturating_add(1);
        if global.failures >= GLOBAL_FAILURE_LIMIT {
            let until = now + GLOBAL_BLOCK;
            global.blocked_until = Some(until);
            return Some(remaining_seconds(until, now));
        }
        drop(global);
        let mut attempts = self.attempts.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = attempts.entry(key.to_owned()).or_default();
        entry.failures = entry.failures.saturating_add(1);
        if entry.failures < FAILURE_LIMIT {
            return None;
        }
        let exponent = entry
            .failures
            .saturating_sub(FAILURE_LIMIT)
            .min(MAX_BACKOFF_EXPONENT);
        let factor = 1u32 << exponent;
        let duration = BASE_BLOCK.saturating_mul(factor).min(MAX_BLOCK);
        let until = now + duration;
        entry.blocked_until = Some(until);
        drop(attempts);
        Some(remaining_seconds(until, now))
    }

    pub fn success(&self, key: &str, now: Instant) {
        let mut attempts = self.attempts.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(entry) = attempts.get_mut(key) {
            entry.failures = 0;
            entry.blocked_until = None;
        }
        drop(attempts);
        let mut global = self.global.lock().unwrap_or_else(PoisonError::into_inner);
        global.failures = 0;
        global.window_start = now;
    }
}

impl Default for Limiter {
    fn default() -> Self {
        Self::new()
    }
}

fn remaining_seconds(until: Instant, now: Instant) -> u64 {
    until
        .checked_duration_since(now)
        .map_or(1, |duration| duration.as_secs().max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_back_off_exponentially() {
        let limiter = Limiter::new();
        let now = Instant::now();
        for _ in 0..FAILURE_LIMIT - 1 {
            assert_eq!(limiter.failure("owner", now), None);
        }
        assert_eq!(limiter.failure("owner", now), Some(60));
        let later = now + BASE_BLOCK + Duration::from_secs(1);
        assert_eq!(limiter.failure("owner", later), Some(120));
        let even_later = later + Duration::from_secs(121);
        assert_eq!(limiter.failure("owner", even_later), Some(240));
        let reset = even_later + Duration::from_secs(241);
        limiter.success("owner", reset);
        assert_eq!(limiter.failure("owner", reset), None);
    }

    #[test]
    fn the_global_limiter_blocks_every_key() {
        let limiter = Limiter::new();
        let now = Instant::now();
        for index in 0..GLOBAL_FAILURE_LIMIT {
            let key = if index % 2 == 0 { "owner" } else { "other" };
            let _ = limiter.failure(key, now);
        }
        assert_eq!(limiter.check("owner", now), Err(300));
        let later = now + GLOBAL_BLOCK + Duration::from_secs(1);
        assert_eq!(limiter.check("owner", later), Ok(()));
    }

    #[test]
    fn a_blocked_key_is_reported_until_it_expires() {
        let limiter = Limiter::new();
        let now = Instant::now();
        for _ in 0..FAILURE_LIMIT {
            let _ = limiter.failure("owner", now);
        }
        assert_eq!(limiter.check("owner", now), Err(60));
        assert_eq!(
            limiter.check("owner", now + BASE_BLOCK + Duration::from_secs(1)),
            Ok(())
        );
    }
}
