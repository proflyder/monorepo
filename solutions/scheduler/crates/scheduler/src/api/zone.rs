//! Per-request client time zone.
//!
//! Every client sends `Time-Zone: <IANA>` on each request. The scheduler
//! stores UTC only and converts schedule times to and from the caller's zone
//! right here. A missing header means UTC (and is logged); an invalid zone is
//! a 400 so a broken client cannot silently shift alarms.

use connectrpc::{ConnectError, RequestContext};
use jiff::tz::TimeZone;
use tracing::warn;

use super::text;

/// Request header carrying the caller's IANA zone.
pub const HEADER: &str = "time-zone";

/// Zone name used when the header is absent.
pub const DEFAULT_ZONE: &str = "UTC";

/// A resolved client zone.
#[derive(Debug, Clone)]
pub struct ClientZone {
    name: String,
}

impl ClientZone {
    /// Resolves the header, defaulting to UTC and rejecting unknown names.
    pub fn from_request(ctx: &RequestContext) -> Result<Self, ConnectError> {
        let raw = ctx
            .header(HEADER)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let Some(name) = raw else {
            warn!(
                header = HEADER,
                "client did not send a time zone; treating times as UTC"
            );
            return Ok(Self::utc());
        };
        TimeZone::get(name).map_err(|_| {
            text::connect(
                ConnectError::invalid_argument("invalid Time-Zone header"),
                "scheduler.error.invalid_argument",
            )
        })?;
        Ok(Self {
            name: name.to_owned(),
        })
    }

    /// UTC fallback for callers without a header.
    #[must_use]
    pub fn utc() -> Self {
        Self {
            name: DEFAULT_ZONE.to_owned(),
        }
    }

    /// IANA name as sent by the client.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_is_the_default() {
        let zone = ClientZone::utc();
        assert_eq!(zone.name(), "UTC");
    }
}
