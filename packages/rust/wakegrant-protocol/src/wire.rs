use serde::Deserialize;
use serde::de::Deserializer;

use crate::Error;

pub const MAX_JSON_BYTES: usize = 64 * 1024;

pub fn parse<T: for<'de> Deserialize<'de>>(body: &str) -> Result<T, Error> {
    if body.len() > MAX_JSON_BYTES {
        return Err(Error::new("body too large"));
    }
    serde_json::from_str(body).map_err(|_| Error::new("malformed json"))
}

pub fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}
