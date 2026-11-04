//! wakegrant client-side protocol logic: the v1 messages and the purpose-aware
//! `logon`/`unlock` messages with long-poll.
//!
//! Portable: no Windows headers, sockets or registry. The contract is
//! `packages/contracts/wakegrant/PROTOCOL.md`; only [`ClaimOutcome::Granted`]
//! may arm the provider.

#![forbid(unsafe_code)]

mod auth;
mod base64;
mod config;
mod error;
mod json;
mod purpose;
mod request;
mod response;
mod secret;

pub use auth::{
    claim_mac_message, claim_mac_message_for_purpose, constant_time_equal, grant_mac_message,
    grant_mac_message_for_purpose, hmac_sha256_b64,
};
pub use base64::{b64_decode, b64_encode};
pub use config::{
    ServiceEndpoint, endpoint_path, parse_service_url, redact_service_url, validate_config,
};
pub use error::Error;
pub use purpose::Purpose;
pub use request::{challenge_request_json, claim_request_json, claim_request_json_for_purpose};
pub use response::{
    ChallengeResponse, ClaimOutcome, ClaimResult, parse_challenge_response, verify_claim_response,
    verify_claim_response_for_purpose,
};
pub use secret::{MachineSecret, parse_secret_json};
