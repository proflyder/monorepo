//! wakegrant v1 client-side protocol logic.
//!
//! Portable: no Windows headers, no sockets, no registry. The logon provider
//! and the host-side tests compile this same crate, so the wire format is
//! exercised on macOS/Linux and used unchanged on Windows.
//!
//! The contract lives in `packages/contracts/wakegrant/PROTOCOL.md`.
//!
//! # Layout
//!
//! - [`b64_encode`]/[`b64_decode`] — standard base64 with padding; the decoder
//!   is strict (canonical padding placement, no whitespace, no foreign
//!   characters) and keeps the C++ reference implementation's tolerance of
//!   non-canonical trailing bits (`"ZZ=="` decodes to `0x65`).
//! - [`hmac_sha256_b64`], [`claim_mac_message`], [`grant_mac_message`] — the
//!   authenticated message shapes.
//! - [`parse_challenge_response`], [`verify_claim_response`] — response
//!   parsing and MAC verification. Only [`ClaimOutcome::Granted`] may arm the
//!   provider; everything else is fail-closed.
//! - [`parse_service_url`], [`validate_config`], [`redact_service_url`],
//!   [`endpoint_path`] — machine configuration.
//! - [`parse_secret_json`] — the DPAPI plaintext payload, with zeroizing
//!   secrets and a redacted `Debug`.

// Pure safe Rust: no FFI, no platform code. The Windows layer lives in the
// provider crate.
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
