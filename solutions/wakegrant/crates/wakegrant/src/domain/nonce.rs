use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use thiserror::Error;
use time::OffsetDateTime;
use wakegrant_protocol::b64_encode;

/// A challenge is valid for 60 s (`PROTOCOL.md`).
pub const NONCE_TTL: Duration = Duration::from_secs(60);

const NONCE_BYTES: usize = 32;
const SWEEP_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Debug, Error)]
#[error("nonce is unknown, expired, reused or bound to another machine")]
pub struct NonceError;

#[derive(Debug, Clone)]
struct Entry {
    machine: String,
    expires: OffsetDateTime,
    used: bool,
}

/// Single-use challenges kept in memory. A pod restart loses outstanding
/// nonces, which is safe: the machine asks for a new one. Grants live in
/// SQLite.
#[derive(Debug, Default)]
pub struct NonceStore {
    inner: Mutex<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    entries: HashMap<String, Entry>,
    last_sweep: Option<OffsetDateTime>,
}

impl NonceStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new 32-byte nonce bound to `machine`.
    #[allow(clippy::significant_drop_tightening)]
    pub fn issue(&self, machine: &str, now: OffsetDateTime) -> Result<String, NonceError> {
        let mut raw = [0u8; NONCE_BYTES];
        getrandom::fill(&mut raw).map_err(|_| NonceError)?;
        let nonce = b64_encode(&raw);

        let mut inner = self.inner.lock().expect("nonce lock");
        sweep(&mut inner, now);
        inner.entries.insert(
            nonce.clone(),
            Entry {
                machine: machine.to_owned(),
                expires: now + NONCE_TTL,
                used: false,
            },
        );
        Ok(nonce)
    }

    /// Validates and atomically marks a nonce used. Marking happens before the
    /// caller verifies the MAC, exactly as `PROTOCOL.md` orders it, so a forged
    /// request burns its nonce.
    #[allow(clippy::significant_drop_tightening)]
    pub fn consume(
        &self,
        machine: &str,
        nonce: &str,
        now: OffsetDateTime,
    ) -> Result<(), NonceError> {
        let mut inner = self.inner.lock().expect("nonce lock");
        let entry = inner.entries.get_mut(nonce).ok_or(NonceError)?;
        if entry.used {
            return Err(NonceError);
        }
        if now >= entry.expires {
            inner.entries.remove(nonce);
            return Err(NonceError);
        }
        if entry.machine != machine {
            return Err(NonceError);
        }
        entry.used = true;
        Ok(())
    }

    /// Number of retained nonces (tests only).
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.inner.lock().expect("nonce lock").entries.len()
    }

    /// Seeds a fixed nonce for the contract vectors (tests only).
    #[cfg(test)]
    pub(crate) fn insert_for_test(&self, machine: &str, nonce: &str, now: OffsetDateTime) {
        self.inner.lock().expect("nonce lock").entries.insert(
            nonce.to_owned(),
            Entry {
                machine: machine.to_owned(),
                expires: now + NONCE_TTL,
                used: false,
            },
        );
    }
}

/// Drops expired entries at most once a minute so an unauthenticated challenge
/// flood cannot grow the map without bound.
fn sweep(inner: &mut Inner, now: OffsetDateTime) {
    if inner.last_sweep.is_some_and(|last| {
        now - last < time::Duration::try_from(SWEEP_INTERVAL).expect("duration")
    }) {
        return;
    }
    inner.last_sweep = Some(now);
    inner.entries.retain(|_, entry| now < entry.expires);
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::*;

    #[test]
    fn nonces_are_single_use_and_machine_bound() {
        let store = NonceStore::new();
        let now = datetime!(2026-11-03 12:00:00 UTC);
        let nonce = store.issue("komp", now).expect("issue");
        assert_eq!(nonce.len(), 44);
        assert!(store.consume("desk", &nonce, now).is_err());
        store.consume("komp", &nonce, now).expect("first use");
        assert!(store.consume("komp", &nonce, now).is_err());
    }

    #[test]
    fn expired_nonces_are_rejected_and_swept() {
        let store = NonceStore::new();
        let now = datetime!(2026-11-03 12:00:00 UTC);
        let nonce = store.issue("komp", now).expect("issue");
        assert!(store.consume("komp", &nonce, now + NONCE_TTL).is_err());
        assert_eq!(store.len(), 0);
    }
}
