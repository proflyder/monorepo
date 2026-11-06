//! wakegrant wire protocol shared by the service and the Windows provider.
//!
//! The contract is `packages/contracts/wakegrant/PROTOCOL.md`: both the v1
//! messages (no `purpose`) and the purpose-aware `logon`/`unlock` messages with
//! long-poll. The crate is portable — no I/O, no runtime — so it builds for the
//! Windows provider (`x86_64-pc-windows-gnu`) and for the service alike.
//!
//! Client configuration (`ServiceEndpoint`, registry values, DPAPI secret
//! loading) lives in the provider; this crate owns only the bytes on the wire.

#![forbid(unsafe_code)]

mod auth;
mod base64;
mod error;
mod machine;
mod purpose;
mod request;
mod response;
mod secret;
mod wire;

pub use auth::{
    claim_mac_message, claim_mac_message_for_purpose, constant_time_equal, grant_mac_message,
    grant_mac_message_for_purpose, hmac_sha256_b64,
};
pub use base64::{b64_decode, b64_encode};
pub use error::Error;
pub use machine::{MACHINE_MAX_LEN, is_valid_machine};
pub use purpose::Purpose;
pub use request::{challenge_request_json, claim_request_json, claim_request_json_for_purpose};
pub use response::{
    ChallengeResponse, ClaimOutcome, ClaimResult, parse_challenge_response, verify_claim_response,
    verify_claim_response_for_purpose,
};
pub use secret::{MachineSecret, parse_secret_json};
