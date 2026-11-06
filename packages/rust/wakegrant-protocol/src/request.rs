//! Request bodies for `POST /v1/challenge` and `POST /v1/claim`.

use serde::Serialize;

use crate::purpose::Purpose;

#[derive(Serialize)]
struct Challenge<'a> {
    machine: &'a str,
}

#[derive(Serialize)]
struct Claim<'a> {
    machine: &'a str,
    nonce: &'a str,
    mac: &'a str,
}

#[derive(Serialize)]
struct PurposeClaim<'a> {
    machine: &'a str,
    nonce: &'a str,
    purpose: &'static str,
    mac: &'a str,
    #[serde(rename = "waitSeconds", skip_serializing_if = "Option::is_none")]
    wait_seconds: Option<u32>,
}

/// Field order is part of the golden request bodies in `PROTOCOL.md`.
fn to_json(value: &impl Serialize) -> String {
    serde_json::to_string(value).expect("a request body of plain strings cannot fail")
}

pub fn challenge_request_json(machine: &str) -> String {
    to_json(&Challenge { machine })
}

/// v1 claim body, no `purpose` and no long-poll.
pub fn claim_request_json(machine: &str, nonce: &str) -> String {
    to_json(&Claim {
        machine,
        nonce,
        mac: "",
    })
}

/// Purpose-aware claim body. `wait_seconds` is `Some` only for the unlock
/// long-poll; the protocol allows it exclusively together with `purpose`.
pub fn claim_request_json_for_purpose(
    machine: &str,
    nonce: &str,
    mac: &str,
    purpose: Purpose,
    wait_seconds: Option<u32>,
) -> String {
    to_json(&PurposeClaim {
        machine,
        nonce,
        purpose: purpose.as_str(),
        mac,
        wait_seconds,
    })
}
