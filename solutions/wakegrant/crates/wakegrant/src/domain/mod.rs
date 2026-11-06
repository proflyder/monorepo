//! Pure domain logic: purposes, TTL rules, nonces, rate limits and waiters.
//!
//! No database, filesystem or network access is needed here, and time comes
//! through the [`Clock`] trait so every rule is testable with fake time.

mod clock;
mod grant;
mod limiter;
mod nonce;
mod purpose;
mod waiter;

pub use clock::{Clock, SystemClock};
pub use grant::{
    DEFAULT_TTL, MAX_TTL, MAX_WAIT_SECONDS, MIN_TTL, MIN_WAIT_SECONDS, TtlError, WaitError,
    validate_ttl, validate_wait,
};
pub use limiter::{CHALLENGE_LIMIT, RATE_WINDOW, RateLimiter};
pub use nonce::{NONCE_TTL, NonceStore};
pub use purpose::{Purpose, PurposeError};
pub use waiter::{MAX_WAITERS_PER_MACHINE, Waiter, Waiters};
