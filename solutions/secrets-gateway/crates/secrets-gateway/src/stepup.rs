use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use tokio::time::Instant;

use crate::totp::{self, Totp};

pub const SESSION_TTL: Duration = Duration::from_secs(300);
pub const FAILURE_LIMIT: u32 = 5;
pub const FAILURE_WINDOW: Duration = Duration::from_secs(60);
pub const BLOCK_DURATION: Duration = Duration::from_secs(60);
const MAX_SESSIONS: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    Invalid,
    Replay,
    RateLimited { retry_after: u64 },
}

pub struct StepUp {
    totp: Totp,
    sessions: Mutex<HashMap<String, Session>>,
    attempts: Mutex<HashMap<String, Attempts>>,
    last_step: Mutex<HashMap<String, u64>>,
}

#[derive(Debug, Clone)]
struct Session {
    token_id: String,
    expires_at: Instant,
}

#[derive(Debug, Clone)]
struct Attempts {
    window_start: Instant,
    failures: u32,
    blocked_until: Option<Instant>,
}

impl StepUp {
    pub fn new(secret: &str) -> Result<Self, totp::Error> {
        Ok(Self {
            totp: Totp::new(secret)?,
            sessions: Mutex::new(HashMap::new()),
            attempts: Mutex::new(HashMap::new()),
            last_step: Mutex::new(HashMap::new()),
        })
    }

    pub fn start_session(
        &self,
        token_id: &str,
        code: &str,
        now_unix: u64,
        now: Instant,
    ) -> Result<(String, u64), Failure> {
        self.check_code(token_id, code, now_unix, now, false)?;
        let session = home_secret::hex(32).map_err(|_| Failure::Invalid)?;
        let session = session.expose().to_owned();
        let mut sessions = self.sessions.lock().unwrap_or_else(PoisonError::into_inner);
        sessions.retain(|_, item| item.expires_at > now);
        if sessions.len() >= MAX_SESSIONS {
            let oldest = sessions
                .iter()
                .min_by_key(|(_, item)| item.expires_at)
                .map(|(key, _)| key.clone());
            if let Some(oldest) = oldest {
                sessions.remove(&oldest);
            }
        }
        sessions.insert(
            session.clone(),
            Session {
                token_id: token_id.to_owned(),
                expires_at: now + SESSION_TTL,
            },
        );
        drop(sessions);
        Ok((session, SESSION_TTL.as_secs()))
    }

    #[must_use]
    pub fn verify_session(&self, token_id: &str, session: &str, now: Instant) -> bool {
        let mut sessions = self.sessions.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(found) = sessions.get(session) else {
            return false;
        };
        let valid = found.token_id == token_id && found.expires_at > now;
        if !valid {
            sessions.remove(session);
        }
        valid
    }

    pub fn verify_mutation(
        &self,
        token_id: &str,
        code: &str,
        now_unix: u64,
        now: Instant,
    ) -> Result<u64, Failure> {
        self.check_code(token_id, code, now_unix, now, true)
    }

    #[allow(clippy::significant_drop_tightening)]
    fn check_code(
        &self,
        token_id: &str,
        code: &str,
        now_unix: u64,
        now: Instant,
        replay_guard: bool,
    ) -> Result<u64, Failure> {
        let mut attempts = self.attempts.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = attempts
            .entry(token_id.to_owned())
            .or_insert_with(|| Attempts {
                window_start: now,
                failures: 0,
                blocked_until: None,
            });
        if let Some(until) = entry.blocked_until {
            if now < until {
                return Err(Failure::RateLimited {
                    retry_after: remaining_seconds(until, now),
                });
            }
            entry.blocked_until = None;
            entry.failures = 0;
            entry.window_start = now;
        }
        if now.duration_since(entry.window_start) > FAILURE_WINDOW {
            entry.failures = 0;
            entry.window_start = now;
        }
        let Some(step) = self.totp.verify(code, now_unix) else {
            return Err(record_failure(entry, now));
        };
        if replay_guard {
            let mut last = self
                .last_step
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if last.get(token_id).is_some_and(|previous| *previous >= step) {
                return Err(record_failure(entry, now));
            }
            last.insert(token_id.to_owned(), step);
        }
        entry.failures = 0;
        entry.window_start = now;
        Ok(step)
    }
}

fn record_failure(entry: &mut Attempts, now: Instant) -> Failure {
    entry.failures += 1;
    if entry.failures >= FAILURE_LIMIT {
        let until = now + BLOCK_DURATION;
        entry.blocked_until = Some(until);
        return Failure::RateLimited {
            retry_after: remaining_seconds(until, now),
        };
    }
    Failure::Invalid
}

fn remaining_seconds(until: Instant, now: Instant) -> u64 {
    until
        .checked_duration_since(now)
        .map_or(1, |duration| duration.as_secs().max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    fn stepup() -> StepUp {
        StepUp::new(SECRET).expect("step-up")
    }

    fn code(unix: u64) -> String {
        Totp::new(SECRET).expect("totp").code(unix)
    }

    #[test]
    fn a_session_expires_after_five_minutes() {
        let stepup = stepup();
        let now = Instant::now();
        let (session, ttl) = stepup
            .start_session("owner", &code(59), 59, now)
            .expect("session");
        assert_eq!(ttl, 300);
        assert!(stepup.verify_session("owner", &session, now + Duration::from_secs(299)));
        assert!(!stepup.verify_session("owner", &session, now + Duration::from_secs(301)));
        assert!(!stepup.verify_session("owner", &session, now + Duration::from_secs(299)));
    }

    #[test]
    fn a_session_belongs_to_the_token_that_started_it() {
        let stepup = stepup();
        let now = Instant::now();
        let (session, _) = stepup
            .start_session("owner", &code(59), 59, now)
            .expect("session");
        assert!(!stepup.verify_session("member", &session, now));
        assert!(!stepup.verify_session("owner", "wrong", now));
    }

    #[test]
    fn five_failures_block_the_token_for_a_minute() {
        let stepup = stepup();
        let now = Instant::now();
        for _ in 0..FAILURE_LIMIT - 1 {
            assert_eq!(
                stepup.start_session("owner", "000000", 59, now),
                Err(Failure::Invalid)
            );
        }
        assert_eq!(
            stepup.start_session("owner", "000000", 59, now),
            Err(Failure::RateLimited { retry_after: 60 })
        );
        assert_eq!(
            stepup.start_session("owner", &code(59), 59, now),
            Err(Failure::RateLimited { retry_after: 60 })
        );
        let later = now + BLOCK_DURATION + Duration::from_secs(1);
        assert!(stepup.start_session("owner", &code(59), 59, later).is_ok());
    }

    #[test]
    fn a_mutation_code_cannot_be_replayed_in_the_same_window() {
        let stepup = stepup();
        let now = Instant::now();
        assert_eq!(stepup.verify_mutation("owner", &code(59), 59, now), Ok(1));
        assert_eq!(
            stepup.verify_mutation("owner", &code(59), 59, now),
            Err(Failure::Invalid)
        );
        assert_eq!(stepup.verify_mutation("owner", &code(89), 89, now), Ok(2));
    }

    #[test]
    fn a_session_start_and_a_mutation_can_share_one_code() {
        let stepup = stepup();
        let now = Instant::now();
        assert!(stepup.start_session("owner", &code(59), 59, now).is_ok());
        assert_eq!(stepup.verify_mutation("owner", &code(59), 59, now), Ok(1));
    }

    #[test]
    fn a_bad_secret_is_rejected() {
        assert!(StepUp::new("not-base32!").is_err());
    }
}
