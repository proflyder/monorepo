//! wakegrant v1 client-side protocol logic.
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
mod request;
mod response;
mod secret;

pub use auth::{claim_mac_message, constant_time_equal, grant_mac_message, hmac_sha256_b64};
pub use base64::{b64_decode, b64_encode};
pub use config::{
    ServiceEndpoint, endpoint_path, parse_service_url, redact_service_url, validate_config,
};
pub use error::Error;
pub use request::{challenge_request_json, claim_request_json};
pub use response::{
    ChallengeResponse, ClaimOutcome, ClaimResult, parse_challenge_response, verify_claim_response,
};
pub use secret::{MachineSecret, parse_secret_json};
