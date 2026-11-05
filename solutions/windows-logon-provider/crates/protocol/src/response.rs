//! Parsing and MAC verification of the service responses.

use serde::Deserialize;

use crate::auth::{
    constant_time_equal, grant_mac_message, grant_mac_message_for_purpose, hmac_sha256_b64,
};
use crate::purpose::Purpose;
use crate::wire::{self, present};
use crate::{Error, b64_decode};

const NONCE_BYTES: usize = 32;
const MAC_BYTES: usize = 32;

/// Tolerant reader: unknown fields are ignored. Authenticity comes from the
/// response MAC, so a new server field must not lock the machine out.
#[derive(Debug, Deserialize)]
struct ChallengeWire {
    nonce: String,
    #[serde(rename = "expiresIn", default, deserialize_with = "present")]
    expires_in: Option<i64>,
}

/// Tolerant reader like [`ChallengeWire`]: unknown fields are ignored and
/// duplicate known fields still fail.
#[derive(Debug, Deserialize)]
struct ClaimWire {
    granted: bool,
    #[serde(rename = "grantId", default, deserialize_with = "present")]
    grant_id: Option<String>,
    #[serde(default, deserialize_with = "present")]
    mac: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChallengeResponse {
    /// base64, validated to decode to 32 bytes.
    pub nonce: String,
    /// Seconds; 0 when absent.
    pub expires_in: i64,
}

pub fn parse_challenge_response(body: &str) -> Result<ChallengeResponse, Error> {
    let challenge: ChallengeWire = wire::parse(body)?;

    let decoded = b64_decode(&challenge.nonce)
        .map_err(|_| Error::new("challenge nonce is not 32 base64 bytes"))?;
    if decoded.len() != NONCE_BYTES {
        return Err(Error::new("challenge nonce is not 32 base64 bytes"));
    }
    let expires_in = match challenge.expires_in {
        None => 0,
        Some(seconds) if (1..=3600).contains(&seconds) => seconds,
        Some(_) => return Err(Error::new("challenge expiresIn out of range")),
    };
    Ok(ChallengeResponse {
        nonce: challenge.nonce,
        expires_in,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimOutcome {
    /// `granted:true` with a valid response mac.
    Granted,
    /// `granted:false` with a valid response mac.
    Denied,
    /// Missing, undecodable or wrong mac: treated as denied.
    InvalidMac,
    /// Not a well-formed claim response at all.
    Malformed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimResult {
    pub outcome: ClaimOutcome,
    pub grant_id: String,
    /// Short, log-safe reason; never contains secrets.
    pub detail: &'static str,
}

impl ClaimResult {
    fn fail(outcome: ClaimOutcome, detail: &'static str) -> Self {
        Self {
            outcome,
            grant_id: String::new(),
            detail,
        }
    }
}

/// Verifies the v1 server response for `nonce` with `key`. Only
/// [`ClaimOutcome::Granted`] may arm the provider; every other outcome is
/// fail-closed.
pub fn verify_claim_response(body: &str, key: &[u8], machine: &str, nonce: &str) -> ClaimResult {
    verify_claim_response_with(body, key, |granted, grant_id| {
        grant_mac_message(machine, nonce, granted, grant_id)
    })
}

/// Verifies a purpose-aware server response. The MAC must be computed over the
/// same purpose that was sent in the claim, so a logon grant cannot be
/// accepted for an unlock and vice versa.
pub fn verify_claim_response_for_purpose(
    body: &str,
    key: &[u8],
    machine: &str,
    nonce: &str,
    purpose: Purpose,
) -> ClaimResult {
    verify_claim_response_with(body, key, |granted, grant_id| {
        grant_mac_message_for_purpose(machine, nonce, granted, grant_id, purpose)
    })
}

fn verify_claim_response_with(
    body: &str,
    key: &[u8],
    mac_message: impl Fn(bool, &str) -> String,
) -> ClaimResult {
    let claim: ClaimWire = match wire::parse(body) {
        Ok(claim) => claim,
        Err(error) => return ClaimResult::fail(ClaimOutcome::Malformed, error.detail()),
    };

    let grant_id = if claim.granted {
        match claim.grant_id {
            Some(id) if !id.is_empty() && id.len() <= 64 => id,
            _ => {
                return ClaimResult::fail(ClaimOutcome::Malformed, "granted claim without grantId");
            }
        }
    } else {
        if claim.grant_id.is_some_and(|id| !id.is_empty()) {
            return ClaimResult::fail(ClaimOutcome::Malformed, "denied claim carries a grantId");
        }
        String::new()
    };

    let Some(mac) = claim.mac else {
        return ClaimResult::fail(ClaimOutcome::InvalidMac, "missing mac");
    };
    let received = match b64_decode(&mac) {
        Ok(received) if received.len() == MAC_BYTES => received,
        _ => return ClaimResult::fail(ClaimOutcome::InvalidMac, "mac is not 32 base64 bytes"),
    };

    // Compare the MAC as bytes; encoding the expected value and decoding it
    // again would only add a step that cannot fail.
    let expected = hmac_sha256_b64(key, &mac_message(claim.granted, &grant_id));
    let Ok(expected) = b64_decode(&expected) else {
        return ClaimResult::fail(ClaimOutcome::InvalidMac, "local mac encoding failed");
    };
    if !constant_time_equal(&received, &expected) {
        return ClaimResult::fail(ClaimOutcome::InvalidMac, "mac mismatch");
    }

    if claim.granted {
        ClaimResult {
            outcome: ClaimOutcome::Granted,
            grant_id,
            detail: "",
        }
    } else {
        ClaimResult::fail(ClaimOutcome::Denied, "")
    }
}
