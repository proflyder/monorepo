use std::time::Duration;

use thiserror::Error;

/// Protocol TTL bounds from `PROTOCOL.md`.
pub const MIN_TTL: Duration = Duration::from_secs(30);
pub const MAX_TTL: Duration = Duration::from_secs(600);
pub const DEFAULT_TTL: Duration = Duration::from_secs(180);

/// Long-poll bounds for a purpose-aware claim.
pub const MIN_WAIT_SECONDS: u32 = 1;
pub const MAX_WAIT_SECONDS: u32 = 25;

/// Why a requested TTL cannot be issued.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TtlError {
    #[error("ttlSeconds must be between {min} and {max} seconds")]
    OutOfRange { min: u64, max: u64 },
    #[error("ttlSeconds must not exceed the operator maximum")]
    AboveOperatorMaximum,
}

/// Validates an optional `ttlSeconds` against the protocol bounds and the
/// operator's `WAKE_GRANT_MAX_TTL_SECONDS`. A negative or non-integer value is
/// rejected by the caller's deserializer, exactly like Go's `*int`.
pub fn validate_ttl(requested: Option<i64>, operator_max: Duration) -> Result<Duration, TtlError> {
    let ttl = match requested {
        None => DEFAULT_TTL,
        Some(seconds) if seconds < 0 => Duration::ZERO,
        Some(seconds) => Duration::from_secs(seconds as u64),
    };
    let max = MAX_TTL.min(operator_max);
    if ttl < MIN_TTL || ttl > max {
        return Err(TtlError::OutOfRange {
            min: MIN_TTL.as_secs(),
            max: max.as_secs(),
        });
    }
    Ok(ttl)
}

/// Why a long-poll request cannot be served.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum WaitError {
    #[error("waitSeconds requires an explicit purpose")]
    WithoutPurpose,
    #[error("waitSeconds must be between {MIN_WAIT_SECONDS} and {MAX_WAIT_SECONDS}")]
    OutOfRange,
}

/// `waitSeconds` is only defined together with an explicit purpose; omitting
/// `purpose` keeps the v1 request byte for byte and is never delayed.
pub fn validate_wait(
    wait_seconds: Option<i64>,
    purpose_aware: bool,
) -> Result<Option<u32>, WaitError> {
    let Some(seconds) = wait_seconds else {
        return Ok(None);
    };
    if !purpose_aware {
        return Err(WaitError::WithoutPurpose);
    }
    if !(i64::from(MIN_WAIT_SECONDS)..=i64::from(MAX_WAIT_SECONDS)).contains(&seconds) {
        return Err(WaitError::OutOfRange);
    }
    Ok(Some(seconds as u32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ttl_defaults_and_bounds() {
        assert_eq!(validate_ttl(None, MAX_TTL).expect("default"), DEFAULT_TTL);
        assert_eq!(validate_ttl(Some(30), MAX_TTL).expect("minimum"), MIN_TTL);
        assert_eq!(validate_ttl(Some(600), MAX_TTL).expect("maximum"), MAX_TTL);
        for invalid in [Some(29), Some(601), Some(0), Some(-1)] {
            assert!(validate_ttl(invalid, MAX_TTL).is_err(), "{invalid:?}");
        }
        assert_eq!(
            validate_ttl(Some(180), Duration::from_secs(120)).expect_err("operator cap"),
            TtlError::OutOfRange { min: 30, max: 120 }
        );
    }

    #[test]
    fn wait_requires_a_purpose_and_stays_in_range() {
        assert_eq!(validate_wait(None, false).expect("none"), None);
        assert_eq!(validate_wait(Some(25), true).expect("valid"), Some(25));
        assert_eq!(
            validate_wait(Some(5), false).expect_err("no purpose"),
            WaitError::WithoutPurpose
        );
        for invalid in [Some(0), Some(26), Some(-1)] {
            assert_eq!(
                validate_wait(invalid, true).expect_err("range"),
                WaitError::OutOfRange
            );
        }
    }
}
