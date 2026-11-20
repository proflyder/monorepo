use connectrpc::{ConnectError, RequestContext};
use jiff::tz::TimeZone;
use tracing::warn;

use super::text;

pub const HEADER: &str = "time-zone";

pub const DEFAULT_ZONE: &str = "UTC";

#[derive(Debug, Clone)]
pub struct ClientZone {
    name: String,
}

impl ClientZone {
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

    #[must_use]
    pub fn utc() -> Self {
        Self {
            name: DEFAULT_ZONE.to_owned(),
        }
    }

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
