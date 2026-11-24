use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use thiserror::Error;
use tokio::time::Instant;

use crate::totp::{self, Totp};

pub const SESSION_TTL: Duration = Duration::from_secs(300);
pub const FAILURE_LIMIT: u32 = 5;
pub const BASE_BLOCK: Duration = Duration::from_secs(60);
pub const MAX_BLOCK: Duration = Duration::from_secs(3600);
pub const GLOBAL_FAILURE_LIMIT: u32 = 10;
pub const GLOBAL_WINDOW: Duration = Duration::from_secs(60);
pub const GLOBAL_BLOCK: Duration = Duration::from_secs(300);
const MAX_SESSIONS: usize = 1024;
const MAX_BACKOFF_EXPONENT: u32 = 6;

#[derive(Debug, Error)]
pub enum Error {
    #[error("TOTP: {0}")]
    Totp(#[from] totp::Error),
    #[error("state: {0}")]
    State(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    Invalid,
    Replay,
    RateLimited { retry_after: u64 },
    State,
}

pub struct StepUp {
    totp: Totp,
    sessions: Mutex<HashMap<String, Session>>,
    attempts: Mutex<HashMap<String, Attempts>>,
    global: Mutex<GlobalAttempts>,
    last_step: Mutex<HashMap<String, u64>>,
    persisted_step: Mutex<u64>,
    state_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct Session {
    token_id: String,
    expires_at: Instant,
}

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

impl StepUp {
    pub fn new(secret: &str) -> Result<Self, Error> {
        Ok(Self {
            totp: Totp::new(secret)?,
            sessions: Mutex::new(HashMap::new()),
            attempts: Mutex::new(HashMap::new()),
            global: Mutex::new(GlobalAttempts {
                window_start: Instant::now(),
                failures: 0,
                blocked_until: None,
            }),
            last_step: Mutex::new(HashMap::new()),
            persisted_step: Mutex::new(0),
            state_path: None,
        })
    }

    pub fn with_state(secret: &str, path: &Path) -> Result<Self, Error> {
        let mut stepup = Self::new(secret)?;
        stepup.persisted_step = Mutex::new(read_step(path)?);
        stepup.state_path = Some(path.to_owned());
        Ok(stepup)
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
        let mut global = self.global.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(until) = global.blocked_until {
            if now < until {
                return Err(Failure::RateLimited {
                    retry_after: remaining_seconds(until, now),
                });
            }
            global.blocked_until = None;
            global.failures = 0;
            global.window_start = now;
        }
        if now.duration_since(global.window_start) > GLOBAL_WINDOW {
            global.failures = 0;
            global.window_start = now;
        }
        let mut attempts = self.attempts.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = attempts.entry(token_id.to_owned()).or_default();
        if let Some(until) = entry.blocked_until {
            if now < until {
                return Err(Failure::RateLimited {
                    retry_after: remaining_seconds(until, now),
                });
            }
            entry.blocked_until = None;
        }
        let Some(step) = self.totp.verify(code, now_unix) else {
            return Err(record_failure(entry, &mut global, now));
        };
        if replay_guard {
            let mut last = self
                .last_step
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let mut persisted = self
                .persisted_step
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let replayed =
                *persisted >= step || last.get(token_id).is_some_and(|previous| *previous >= step);
            if replayed {
                return Err(record_failure(entry, &mut global, now));
            }
            if let Some(path) = self.state_path.as_deref() {
                write_step(path, step).map_err(|_| Failure::State)?;
            }
            *persisted = step;
            last.insert(token_id.to_owned(), step);
        }
        entry.failures = 0;
        entry.blocked_until = None;
        global.failures = 0;
        global.window_start = now;
        Ok(step)
    }
}

fn record_failure(entry: &mut Attempts, global: &mut GlobalAttempts, now: Instant) -> Failure {
    entry.failures = entry.failures.saturating_add(1);
    global.failures = global.failures.saturating_add(1);
    if global.failures >= GLOBAL_FAILURE_LIMIT {
        let until = now + GLOBAL_BLOCK;
        global.blocked_until = Some(until);
        return Failure::RateLimited {
            retry_after: remaining_seconds(until, now),
        };
    }
    if entry.failures < FAILURE_LIMIT {
        return Failure::Invalid;
    }
    let exponent = entry
        .failures
        .saturating_sub(FAILURE_LIMIT)
        .min(MAX_BACKOFF_EXPONENT);
    let factor = 1u32 << exponent;
    let duration = BASE_BLOCK.saturating_mul(factor).min(MAX_BLOCK);
    let until = now + duration;
    entry.blocked_until = Some(until);
    Failure::RateLimited {
        retry_after: remaining_seconds(until, now),
    }
}

fn remaining_seconds(until: Instant, now: Instant) -> u64 {
    until
        .checked_duration_since(now)
        .map_or(1, |duration| duration.as_secs().max(1))
}

fn read_step(path: &Path) -> Result<u64, Error> {
    match std::fs::read_to_string(path) {
        Ok(text) => text.trim().parse::<u64>().map_err(|_| {
            Error::State(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "last TOTP step is not a number",
            ))
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(Error::State(error)),
    }
}

fn write_step(path: &Path, step: u64) -> std::io::Result<()> {
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, format!("{step}\n"))?;
    std::fs::rename(temporary, path)
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
    fn failures_back_off_exponentially() {
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
        let later = now + BASE_BLOCK + Duration::from_secs(1);
        assert_eq!(
            stepup.start_session("owner", "000000", 59, later),
            Err(Failure::RateLimited { retry_after: 120 })
        );
        let even_later = later + Duration::from_secs(121);
        assert_eq!(
            stepup.start_session("owner", "000000", 59, even_later),
            Err(Failure::RateLimited { retry_after: 240 })
        );
        let reset = even_later + Duration::from_secs(241);
        assert!(stepup.start_session("owner", &code(59), 59, reset).is_ok());
        assert_eq!(
            stepup.start_session("owner", "000000", 59, reset),
            Err(Failure::Invalid)
        );
    }

    #[test]
    fn the_global_limiter_blocks_every_token() {
        let stepup = stepup();
        let now = Instant::now();
        for index in 0..GLOBAL_FAILURE_LIMIT {
            let token = if index % 2 == 0 { "owner" } else { "other" };
            let _ = stepup.start_session(token, "000000", 59, now);
        }
        assert_eq!(
            stepup.start_session("owner", &code(59), 59, now),
            Err(Failure::RateLimited { retry_after: 300 })
        );
        let later = now + GLOBAL_BLOCK + Duration::from_secs(1);
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
    fn a_restart_cannot_replay_a_persisted_step() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("last-totp-step");
        let now = Instant::now();
        let first = StepUp::with_state(SECRET, &path).expect("step-up");
        assert_eq!(first.verify_mutation("owner", &code(59), 59, now), Ok(1));
        drop(first);
        let restarted = StepUp::with_state(SECRET, &path).expect("step-up");
        assert_eq!(
            restarted.verify_mutation("owner", &code(59), 59, now),
            Err(Failure::Invalid)
        );
        assert_eq!(
            restarted.verify_mutation("owner", &code(89), 89, now),
            Ok(2)
        );
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
