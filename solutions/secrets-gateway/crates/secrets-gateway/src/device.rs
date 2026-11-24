use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::{URL_SAFE_NO_PAD, URL_SAFE_NO_PAD_INDIFFERENT};
use p256::ecdsa::VerifyingKey;
use p256::pkcs8::DecodePublicKey;
use serde::Serialize;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use thiserror::Error;
use tokio::time::Instant;

use crate::attestation::{self, Roots};
use crate::limiter::Limiter;
use crate::openbao::{self, OpenBao};

pub const DEVICE_PATH: &str = "k8s/home-assistant/secrets-gateway-device";
pub const ENROLLMENT_TTL_SECONDS: u64 = 900;
pub const ATTESTATION_TTL_SECONDS: u64 = 900;
pub const MAX_PUBLIC_KEY_BYTES: usize = 512;
pub const MAX_ENROLLMENT_CODE_CHARS: usize = 64;
pub const MIN_ENROLLMENT_CODE_CHARS: usize = 8;
const MAX_ATTESTATIONS: usize = 32;
const REVOCATION_POLL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum Error {
    #[error("device is not enrolled")]
    NotEnrolled,
    #[error("enrollment is not open")]
    EnrollmentClosed,
    #[error("enrollment code has expired")]
    CodeExpired,
    #[error("enrollment code is invalid")]
    CodeInvalid,
    #[error("attestation challenge is invalid")]
    ChallengeInvalid,
    #[error("public key is invalid")]
    KeyInvalid,
    #[error("attestation chain is invalid")]
    AttestationInvalid,
    #[error("device state changed concurrently")]
    Conflict,
    #[error("device state is unavailable")]
    Unavailable,
    #[error("too many attempts")]
    RateLimited { retry_after: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub device_id: String,
    pub enrolled_at: u64,
    pub public_key: String,
}

#[derive(Clone)]
pub struct DeviceKey {
    pub device_id: String,
    pub fingerprint: String,
    pub verifying: VerifyingKey,
}

impl std::fmt::Debug for DeviceKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DeviceKey")
            .field("device_id", &self.device_id)
            .field("fingerprint", &self.fingerprint)
            .finish_non_exhaustive()
    }
}

pub struct Enrollment<'a> {
    pub code: &'a str,
    pub public_key: &'a str,
    pub challenge: &'a str,
    pub attestation_chain: &'a [String],
}

pub struct DeviceStore {
    openbao: Arc<OpenBao>,
    path: String,
    limiter: Limiter,
    attestations: Mutex<HashMap<String, PendingChallenge>>,
    revoked_poll: Mutex<Option<Instant>>,
    roots: Roots,
    fixed_challenge: Option<String>,
}

struct PendingChallenge {
    nonce: String,
    expires_at: u64,
}

struct Record {
    data: BTreeMap<String, String>,
    version: u64,
}

impl DeviceStore {
    #[must_use]
    pub fn new(openbao: Arc<OpenBao>, path: &str) -> Self {
        Self::with_roots(openbao, path, Roots::pinned(), None)
    }

    #[must_use]
    pub fn with_roots(
        openbao: Arc<OpenBao>,
        path: &str,
        roots: Roots,
        fixed_challenge: Option<String>,
    ) -> Self {
        Self {
            openbao,
            path: path.to_owned(),
            limiter: Limiter::new(),
            attestations: Mutex::new(HashMap::new()),
            revoked_poll: Mutex::new(None),
            roots,
            fixed_challenge,
        }
    }

    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    pub async fn status(&self) -> Result<Option<Device>, Error> {
        let record = self.record().await?;
        Ok(device_from(&record.data))
    }

    pub async fn poll_revocation(&self, now: Instant) -> Result<Option<String>, Error> {
        {
            let mut polled = self
                .revoked_poll
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if polled.is_some_and(|last| now.duration_since(last) < REVOCATION_POLL) {
                return Ok(None);
            }
            *polled = Some(now);
        }
        let record = self.record().await?;
        let Some(device_id) = record.data.get("device_revoke_device_id").cloned() else {
            return Ok(None);
        };
        let mut data = record.data;
        data.remove("device_revoke_device_id");
        data.remove("device_revoked_at");
        match self.openbao.write(&self.path, &data, record.version).await {
            Ok(_) => Ok(Some(device_id)),
            Err(_) => Ok(None),
        }
    }

    pub async fn key(&self) -> Result<Option<DeviceKey>, Error> {
        let record = self.record().await?;
        let Some(encoded) = record.data.get("device_public_key") else {
            return Ok(None);
        };
        let device_id = record
            .data
            .get("device_id")
            .cloned()
            .ok_or(Error::Unavailable)?;
        let verifying = decode_public_key(encoded)?;
        Ok(Some(DeviceKey {
            device_id,
            fingerprint: fingerprint(encoded),
            verifying,
        }))
    }

    pub fn attestation_challenge(
        &self,
        principal: &str,
        now_unix: u64,
    ) -> Result<(String, u64), Error> {
        let nonce = match &self.fixed_challenge {
            Some(challenge) => challenge.clone(),
            None => home_secret::hex(32)
                .map_err(|_| Error::Unavailable)?
                .expose()
                .to_owned(),
        };
        let mut attestations = self
            .attestations
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        attestations.retain(|_, item| item.expires_at > now_unix);
        if attestations.len() >= MAX_ATTESTATIONS {
            attestations.clear();
        }
        attestations.insert(
            principal.to_owned(),
            PendingChallenge {
                nonce: nonce.clone(),
                expires_at: now_unix + ATTESTATION_TTL_SECONDS,
            },
        );
        drop(attestations);
        Ok((nonce, ATTESTATION_TTL_SECONDS))
    }

    pub async fn enroll(
        &self,
        principal: &str,
        request: Enrollment<'_>,
        now_unix: u64,
        now: Instant,
    ) -> Result<Device, Error> {
        self.limiter
            .check(principal, now)
            .map_err(|retry_after| Error::RateLimited { retry_after })?;
        match self.enroll_inner(principal, &request, now_unix).await {
            Ok(device) => {
                self.limiter.success(principal, now);
                Ok(device)
            }
            Err(error @ (Error::Unavailable | Error::Conflict)) => Err(error),
            Err(error) => {
                if let Some(retry_after) = self.limiter.failure(principal, now) {
                    return Err(Error::RateLimited { retry_after });
                }
                Err(error)
            }
        }
    }

    async fn enroll_inner(
        &self,
        principal: &str,
        request: &Enrollment<'_>,
        now_unix: u64,
    ) -> Result<Device, Error> {
        let record = self.record().await?;
        let stored_hash = record
            .data
            .get("enrollment_code_hash")
            .ok_or(Error::EnrollmentClosed)?;
        let expires = record
            .data
            .get("enrollment_expires_at")
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or(Error::EnrollmentClosed)?;
        if now_unix > expires {
            return Err(Error::CodeExpired);
        }
        let code = normalize_code(request.code);
        if !valid_code(&code) {
            return Err(Error::CodeInvalid);
        }
        if !hashes_match(&hash_code(&code), stored_hash) {
            return Err(Error::CodeInvalid);
        }
        self.consume_attestation(principal, request.challenge, now_unix)?;
        let public_key_der = public_key_der(request.public_key)?;
        let attestation = attestation::verify(
            request.attestation_chain,
            request.challenge,
            &public_key_der,
            &self.roots,
            now_unix,
        )
        .map_err(|error| {
            tracing::warn!(error = %error, "key attestation rejected");
            Error::AttestationInvalid
        })?;
        let device_id = home_secret::hex(16)
            .map_err(|_| Error::Unavailable)?
            .expose()
            .to_owned();
        let mut data = record.data;
        data.remove("enrollment_code_hash");
        data.remove("enrollment_expires_at");
        data.insert("device_id".to_owned(), device_id.clone());
        data.insert(
            "device_public_key".to_owned(),
            request.public_key.to_owned(),
        );
        data.insert("device_enrolled_at".to_owned(), now_unix.to_string());
        data.insert(
            "device_attestation_sha256".to_owned(),
            attestation.fingerprint,
        );
        data.insert(
            "device_security_level".to_owned(),
            attestation.security_level.to_string(),
        );
        data.insert("device_verified_boot".to_owned(), attestation.verified_boot);
        let fingerprint = fingerprint(request.public_key);
        data.insert("device_fingerprint".to_owned(), fingerprint);
        self.openbao
            .write(&self.path, &data, record.version)
            .await
            .map_err(map_write)?;
        Ok(Device {
            device_id,
            enrolled_at: now_unix,
            public_key: request.public_key.to_owned(),
        })
    }

    fn consume_attestation(
        &self,
        principal: &str,
        challenge: &str,
        now_unix: u64,
    ) -> Result<(), Error> {
        let item = {
            let mut attestations = self
                .attestations
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            attestations.retain(|_, item| item.expires_at > now_unix);
            attestations.remove(principal)
        };
        let Some(item) = item else {
            return Err(Error::ChallengeInvalid);
        };
        if item.nonce.as_bytes().ct_eq(challenge.as_bytes()).into() {
            Ok(())
        } else {
            Err(Error::ChallengeInvalid)
        }
    }

    async fn record(&self) -> Result<Record, Error> {
        match self.openbao.read(&self.path, None).await {
            Ok(secret) => Ok(Record {
                data: secret.data,
                version: secret.version,
            }),
            Err(openbao::Error::NotFound) => Ok(Record {
                data: BTreeMap::new(),
                version: 0,
            }),
            Err(_) => Err(Error::Unavailable),
        }
    }
}

fn device_from(data: &BTreeMap<String, String>) -> Option<Device> {
    let device_id = data.get("device_id")?.clone();
    let enrolled_at = data
        .get("device_enrolled_at")
        .and_then(|value| value.parse::<u64>().ok())?;
    data.get("device_public_key")?;
    let public_key = data.get("device_public_key")?.clone();
    Some(Device {
        device_id,
        enrolled_at,
        public_key,
    })
}

#[must_use]
pub fn normalize_code(raw: &str) -> String {
    raw.chars()
        .filter(|character| !character.is_whitespace() && *character != '-')
        .map(|character| character.to_ascii_uppercase())
        .collect()
}

#[must_use]
pub fn hash_code(code: &str) -> String {
    hex::encode(Sha256::digest(code.as_bytes()))
}

fn valid_code(code: &str) -> bool {
    code.len() >= MIN_ENROLLMENT_CODE_CHARS
        && code.len() <= MAX_ENROLLMENT_CODE_CHARS
        && code
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
}

fn hashes_match(provided: &str, stored: &str) -> bool {
    if provided.len() != stored.len() {
        return false;
    }
    provided.as_bytes().ct_eq(stored.as_bytes()).into()
}

fn public_key_der(encoded: &str) -> Result<Vec<u8>, Error> {
    if encoded.is_empty() || encoded.len() > MAX_PUBLIC_KEY_BYTES * 2 {
        return Err(Error::KeyInvalid);
    }
    let der = URL_SAFE_NO_PAD_INDIFFERENT
        .decode(encoded)
        .map_err(|_| Error::KeyInvalid)?;
    if der.len() > MAX_PUBLIC_KEY_BYTES {
        return Err(Error::KeyInvalid);
    }
    Ok(der)
}

fn decode_public_key(encoded: &str) -> Result<VerifyingKey, Error> {
    let der = public_key_der(encoded)?;
    VerifyingKey::from_public_key_der(&der).map_err(|_| Error::KeyInvalid)
}

fn fingerprint(encoded: &str) -> String {
    hex::encode(Sha256::digest(encoded.as_bytes()))
}

fn map_write(error: openbao::Error) -> Error {
    match error {
        openbao::Error::Conflict => Error::Conflict,
        openbao::Error::NotFound => Error::EnrollmentClosed,
        _ => Error::Unavailable,
    }
}

#[must_use]
pub fn encode_base64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

#[must_use]
pub fn decode_base64(value: &str) -> Option<Vec<u8>> {
    URL_SAFE_NO_PAD_INDIFFERENT.decode(value).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_normalized_and_bounded() {
        assert_eq!(normalize_code(" abcd-efgh \n"), "ABCDEFGH");
        assert!(valid_code("ABCDEFGH23456789"));
        assert!(!valid_code("short"));
        assert!(!valid_code("ABCD EFGH"));
        assert!(!valid_code(&"A".repeat(MAX_ENROLLMENT_CODE_CHARS + 1)));
    }

    #[test]
    fn the_hash_is_stable_and_constant_time_compared() {
        let code = normalize_code("abcd efgh");
        let hash = hash_code(&code);
        assert_eq!(hash.len(), 64);
        assert!(hashes_match(&hash, &hash));
        assert!(!hashes_match(&hash, &hash_code("OTHER")));
        assert!(!hashes_match("abc", &hash));
    }
}
