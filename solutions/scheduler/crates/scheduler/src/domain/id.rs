//! Stable identifiers.
//!
//! Resource ids are time-ordered UUIDv7; event ids are UUIDv5 over a dedupe
//! identity. Both reproduce the former Go `idgen` byte for byte.

use uuid::Uuid;
use uuid::timestamp::{Timestamp, context::NoContext};

/// Deterministic UUIDv5 (DNS namespace) of a dedupe key.
///
/// `Uuid::NAMESPACE_DNS` is the RFC 4122 namespace
/// `6ba7b810-9dad-11d1-80b4-00c04fd430c8`, the same bytes the Go idgen used.
#[must_use]
pub fn event_id(dedupe_key: &str) -> String {
    Uuid::new_v5(&Uuid::NAMESPACE_DNS, dedupe_key.as_bytes()).to_string()
}

/// Time-ordered UUIDv7 for a schedule or timer created at `now_unix_millis`.
#[must_use]
pub fn resource_id(now_unix_millis: i64) -> String {
    let millis = now_unix_millis.max(0);
    let seconds = u64::try_from(millis / 1_000).unwrap_or(0);
    let nanos = u32::try_from((millis % 1_000) * 1_000_000).unwrap_or(0);
    let timestamp = Timestamp::from_unix(NoContext, seconds, nanos);
    Uuid::new_v7(timestamp).to_string()
}

/// Opaque random id for identities that must stay unique within one second.
#[must_use]
pub fn random_id() -> String {
    Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_id_is_deterministic_uuid_v5() {
        let id = event_id("trigger:1:sched-alarm:1767339000:0");
        assert_eq!(id, "688bb4d2-d91c-5751-9e62-32f4ec146479");
        assert_eq!(id, event_id("trigger:1:sched-alarm:1767339000:0"));
    }

    #[test]
    fn resource_id_is_uuid_v7_with_embedded_millis() {
        let millis = 1_767_268_800_123;
        let id = resource_id(millis);
        let parsed = Uuid::parse_str(&id).expect("valid uuid");
        assert_eq!(parsed.get_version_num(), 7);
        assert_eq!(id.len(), 36);
        let timestamp = parsed.get_timestamp().expect("uuidv7 carries a timestamp");
        assert_eq!(timestamp.to_unix().0, 1_767_268_800);
        assert_eq!(timestamp.to_unix().1, 123_000_000);

        // Time ordering is the point of v7.
        assert!(resource_id(millis) < resource_id(millis + 1));
    }
}
