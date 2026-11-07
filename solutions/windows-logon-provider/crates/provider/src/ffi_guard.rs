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
    catch_unwind(AssertUnwindSafe(body)).unwrap_or_else(|_| on_panic())
}

/// Runs `body` and guarantees that `teardown` runs exactly once, even when
/// `body` panics. A panic inside `teardown` itself is swallowed, so no unwind
/// can cross the FFI boundary.
///
/// `teardown` and `on_panic` must be allocation-free: they run on a failure
/// path where allocating again could double-panic. Both closures only touch
/// atomics or already-owned state in the thread procedures.
pub fn guard_thread<T>(
    body: impl FnOnce() -> T,
    teardown: impl FnOnce(),
    on_panic: impl FnOnce(),
) -> Option<T> {
    let teardown = Teardown::new(teardown);
    let result = catch_unwind(AssertUnwindSafe(body)).ok();
    if result.is_none() {
        let _ = catch_unwind(AssertUnwindSafe(on_panic));
    }
    drop(teardown);
    result
}

/// RAII cleanup step. Its `Drop` catches a panic so a failing cleanup can
/// never unwind out of an `extern "system"` function.
struct Teardown<F: FnOnce()> {
    action: Option<F>,
}

impl<F: FnOnce()> Teardown<F> {
    fn new(action: F) -> Self {
        Self {
            action: Some(action),
        }
    }
}

impl<F: FnOnce()> Drop for Teardown<F> {
    fn drop(&mut self) {
        if let Some(action) = self.action.take() {
            let _ = catch_unwind(AssertUnwindSafe(action));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{guard, guard_thread};
    use std::cell::Cell;

    #[test]
    fn returns_the_body_value() {
        assert_eq!(guard(|| 7, || -1), 7);
    }

    #[test]
    fn converts_a_panic_into_the_fallback() {
        assert_eq!(guard(|| panic!("boom"), || -1), -1);
    }

    #[test]
    fn a_panicking_thread_body_still_runs_cleanup_exactly_once() {
        let cleanup = Cell::new(0);
        let panic_hook = Cell::new(0);
        let result = guard_thread(
            || panic!("boom"),
            || cleanup.set(cleanup.get() + 1),
            || panic_hook.set(panic_hook.get() + 1),
        );
        assert!(result.is_none());
        assert_eq!(cleanup.get(), 1);
        assert_eq!(panic_hook.get(), 1);
    }

    #[test]
    fn a_successful_thread_body_runs_cleanup_once() {
        let cleanup = Cell::new(0);
        let result = guard_thread(
            || 7,
            || cleanup.set(cleanup.get() + 1),
            || cleanup.set(cleanup.get() + 100),
        );
        assert_eq!(result, Some(7));
        assert_eq!(cleanup.get(), 1);
    }

    #[test]
    fn a_panicking_cleanup_never_escapes() {
        let result = guard_thread(|| 7, || panic!("cleanup"), || {});
        assert_eq!(result, Some(7));
        let result = guard_thread(|| panic!("body"), || panic!("cleanup"), || panic!("hook"));
        assert!(result.is_none());
    }
}
