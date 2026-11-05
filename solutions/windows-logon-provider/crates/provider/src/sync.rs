//! Poison-tolerant mutex access.
//!
//! A panic on a LogonUI thread leaves the provider fail-closed (no armed
//! session, no tile), and the shared state behind the lock is structurally
//! sound, so continuing with the inner value is safe and simpler than a
//! second failure mode.

use std::sync::{Mutex, MutexGuard, PoisonError};

pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
