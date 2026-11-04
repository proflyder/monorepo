//! `catch_unwind` helpers for every `extern "system"` boundary.
//!
//! Neither the two DLL exports nor any COM method may let a Rust panic unwind
//! into LogonUI: the COM ABI is not unwind-aware and the process would abort or
//! corrupt state. Each entry point wraps its body in [`guard`], which converts a
//! panic into the value the caller expects on failure (`E_FAIL`, `S_FALSE`, a
//! null pointer, ...).
//!
//! The helper is platform independent so the panic path itself is tested on the
//! build host.

use std::panic::{AssertUnwindSafe, catch_unwind};

/// Runs `body`, returning `on_panic()` if it panics.
///
/// `AssertUnwindSafe` is deliberate: a panicking COM call leaves the provider in
/// the same fail-closed state the `on_panic` branch reports (no tile, no armed
/// session), and the shared state uses poisoning-tolerant locks, so continuing
/// after the unwind is safe.
pub fn guard<T>(body: impl FnOnce() -> T, on_panic: impl FnOnce() -> T) -> T {
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(value) => value,
        Err(_) => on_panic(),
    }
}

#[cfg(test)]
mod tests {
    use super::guard;

    #[test]
    fn returns_the_body_value() {
        assert_eq!(guard(|| 7, || -1), 7);
    }

    #[test]
    fn converts_a_panic_into_the_fallback() {
        assert_eq!(guard(|| panic!("boom"), || -1), -1);
    }
}
