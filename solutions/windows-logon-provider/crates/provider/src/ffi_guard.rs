//! `catch_unwind` for every `extern "system"` boundary.
//!
//! A Rust panic must not unwind into LogonUI: the COM ABI is not
//! unwind-aware. Platform independent so the panic path is host-tested.

use std::panic::{AssertUnwindSafe, catch_unwind};

///
/// `AssertUnwindSafe` is deliberate: a panicking COM call leaves the provider
/// fail-closed (no tile, no armed session) and the shared locks tolerate
/// poisoning, so continuing after the unwind is safe.
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
