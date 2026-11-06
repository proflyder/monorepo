//! The process thread's [`Supervisor`]: every actor is spawned through it, and
//! `main` waits on the same instance at teardown.
//!
//! One per thread, so parallel `cargo test` threads and the daemon runtime each
//! get their own registry without passing a supervisor through every service
//! constructor. The daemon runs all actors on the `LocalSet` thread, so `main`
//! sees the same instance.

use std::cell::RefCell;

use home_runtime::Supervisor;

thread_local! {
    static SUPERVISOR: RefCell<Option<Supervisor>> = const { RefCell::new(None) };
}

/// The current thread's supervisor, created on first use.
#[must_use]
pub fn supervisor() -> Supervisor {
    SUPERVISOR.with_borrow_mut(|slot| slot.get_or_insert_with(Supervisor::new).clone())
}
