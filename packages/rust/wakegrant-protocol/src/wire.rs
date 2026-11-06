//! Size-limited typed JSON decoding shared by every wire message.
//!
//! Bodies over 64 KiB are refused before `serde_json` sees them. Server
//! responses are tolerant readers (unknown fields ignored) because their
//! authenticity is the MAC, while the local secret keeps
//! `deny_unknown_fields`. In both cases duplicate known fields, wrong types
//! and nesting inside known fields fail.

use serde::Deserialize;
use serde::de::Deserializer;

use crate::Error;

/// Bodies above this size are rejected before parsing.
pub const MAX_JSON_BYTES: usize = 64 * 1024;

pub fn parse<T: for<'de> Deserialize<'de>>(body: &str) -> Result<T, Error> {
    if body.len() > MAX_JSON_BYTES {
        return Err(Error::new("body too large"));
    }
    serde_json::from_str(body).map_err(|_| Error::new("malformed json"))
}

/// Accepts a present value of type `T` but not JSON `null`; an absent field
/// stays `None` through `#[serde(default)]`. The protocol has no nullable
/// fields, so `null` must remain a malformed message rather than "absent".
pub fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}
