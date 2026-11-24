use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD_INDIFFERENT;
use p256::ecdsa::{Signature, signature::Verifier};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::time::Instant;

use crate::device::{DeviceKey, DeviceStore};
use crate::limiter::Limiter;

pub const SESSION_TTL: Duration = Duration::from_secs(300);
pub const CHALLENGE_TTL_SECONDS: u64 = 60;
pub const MAX_CHALLENGES: usize = 64;
pub const MAX_CHALLENGES_PER_PRINCIPAL: usize = 8;
const MAX_SESSIONS: usize = 1024;
const MAX_SIGNATURE_CHARS: usize = 512;

#[derive(Debug, Error)]
pub enum Error {
    #[error("state: {0}")]
    State(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    Invalid,
    Expired,
    NotEnrolled,
    RateLimited { retry_after: u64 },
    State,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    Session,
    Update,
    Rollback,
    Delete,
}

impl Purpose {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Update => "update",
            Self::Rollback => "rollback",
            Self::Delete => "delete",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "session" => Some(Self::Session),
            "update" => Some(Self::Update),
            "rollback" => Some(Self::Rollback),
            "delete" => Some(Self::Delete),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operation {
    pub purpose: Purpose,
    pub path: Option<String>,
    pub key: Option<String>,
    pub version: Option<u64>,
    pub cas: Option<u64>,
}

impl Operation {
    #[must_use]
    pub fn session() -> Self {
        Self {
            purpose: Purpose::Session,
            path: None,
            key: None,
            version: None,
            cas: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChallengeView {
    pub challenge: String,
    pub payload: String,
    pub expires_in: u64,
}

pub struct StepUp {
    device: Arc<DeviceStore>,
    sessions: Mutex<HashMap<String, Session>>,
    challenges: Mutex<Challenges>,
    limiter: Limiter,
    state_path: PathBuf,
}

#[derive(Debug, Clone)]
struct Session {
    token_id: String,
    fingerprint: String,
    expires_at: Instant,
}

#[derive(Debug, Clone)]
struct ChallengeRecord {
    principal: String,
    purpose: Purpose,
    path: Option<String>,
    key: Option<String>,
    version: Option<u64>,
    cas: Option<u64>,
    payload: String,
    expires_at: Instant,
    fingerprint: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Challenges {
    #[serde(default)]
    consumed: BTreeMap<String, u64>,
    #[serde(skip)]
    entries: BTreeMap<String, ChallengeRecord>,
}

impl StepUp {
    pub fn new(device: Arc<DeviceStore>) -> Self {
        Self {
            device,
            sessions: Mutex::new(HashMap::new()),
            challenges: Mutex::new(Challenges::default()),
            limiter: Limiter::new(),
            state_path: PathBuf::new(),
        }
    }

    pub fn with_state(device: Arc<DeviceStore>, path: &Path) -> Result<Self, Error> {
        let challenges = read_challenges(path)?;
        Ok(Self {
            device,
            sessions: Mutex::new(HashMap::new()),
            challenges: Mutex::new(challenges),
            limiter: Limiter::new(),
            state_path: path.to_owned(),
        })
    }

    pub async fn issue(
        &self,
        principal: &str,
        operation: Operation,
        now_unix: u64,
        now: Instant,
    ) -> Result<ChallengeView, Failure> {
        self.limiter
            .check(principal, now)
            .map_err(|retry_after| Failure::RateLimited { retry_after })?;
        let key = self.device.key().await.map_err(|_| Failure::State)?;
        let Some(key) = key else {
            return Err(Failure::NotEnrolled);
        };
        let challenge = home_secret::hex(32)
            .map_err(|_| Failure::State)?
            .expose()
            .to_owned();
        let expires_at = now_unix + CHALLENGE_TTL_SECONDS;
        let payload = canonical(&challenge, principal, &operation, expires_at);
        let mut challenges = self
            .challenges
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        challenges.consumed.retain(|_, expires| *expires > now_unix);
        let per_principal = challenges
            .entries
            .values()
            .filter(|item| item.principal == principal)
            .count();
        if challenges.entries.len() >= MAX_CHALLENGES
            || per_principal >= MAX_CHALLENGES_PER_PRINCIPAL
        {
            return Err(Failure::RateLimited {
                retry_after: CHALLENGE_TTL_SECONDS,
            });
        }
        challenges.entries.insert(
            challenge.clone(),
            ChallengeRecord {
                principal: principal.to_owned(),
                purpose: operation.purpose,
                path: operation.path,
                key: operation.key,
                version: operation.version,
                cas: operation.cas,
                payload: payload.clone(),
                expires_at: now + Duration::from_secs(CHALLENGE_TTL_SECONDS),
                fingerprint: key.fingerprint,
            },
        );
        write_challenges(&self.state_path, &challenges).map_err(|_| Failure::State)?;
        drop(challenges);
        Ok(ChallengeView {
            challenge,
            payload,
            expires_in: CHALLENGE_TTL_SECONDS,
        })
    }

    pub async fn open_session(
        &self,
        principal: &str,
        challenge: &str,
        signature: &str,
        now_unix: u64,
        now: Instant,
    ) -> Result<(String, u64), Failure> {
        let record = self.consume(principal, Purpose::Session, challenge, now_unix, now, None)?;
        let key = self.device.key().await.map_err(|_| Failure::State)?;
        let Some(key) = key else {
            return Err(Failure::NotEnrolled);
        };
        if key.fingerprint != record.fingerprint
            || !verify_signature(&record.payload, signature, &key)
        {
            return Err(self.record_failure(principal, now));
        }
        self.limiter.success(principal, now);
        let session = home_secret::hex(32)
            .map_err(|_| Failure::State)?
            .expose()
            .to_owned();
        let mut sessions = self.sessions.lock().unwrap_or_else(PoisonError::into_inner);
        sessions.retain(|_, item| item.expires_at > now);
        if sessions.len() >= MAX_SESSIONS {
            let oldest = sessions
                .iter()
                .min_by_key(|(_, item)| item.expires_at)
                .map(|(key, _)| key.clone());
            if let Some(oldest) = oldest {
                sessions.remove(&oldest);
            }
        }
        sessions.insert(
            session.clone(),
            Session {
                token_id: principal.to_owned(),
                fingerprint: key.fingerprint,
                expires_at: now + SESSION_TTL,
            },
        );
        drop(sessions);
        Ok((session, SESSION_TTL.as_secs()))
    }

    pub async fn verify_session(&self, token_id: &str, session: &str, now: Instant) -> bool {
        let fingerprint = {
            let mut sessions = self.sessions.lock().unwrap_or_else(PoisonError::into_inner);
            let Some(found) = sessions.get(session) else {
                return false;
            };
            let valid = found.token_id == token_id && found.expires_at > now;
            let fingerprint = found.fingerprint.clone();
            if !valid {
                sessions.remove(session);
                return false;
            }
            drop(sessions);
            fingerprint
        };
        let current = self.device.key().await;
        let Ok(Some(key)) = current else {
            return false;
        };
        if key.fingerprint != fingerprint {
            let mut sessions = self.sessions.lock().unwrap_or_else(PoisonError::into_inner);
            sessions.remove(session);
            drop(sessions);
            return false;
        }
        true
    }

    pub async fn authorize_mutation(
        &self,
        principal: &str,
        expected: &Operation,
        challenge: &str,
        signature: &str,
        now_unix: u64,
        now: Instant,
    ) -> Result<(), Failure> {
        let record = self.consume(
            principal,
            expected.purpose,
            challenge,
            now_unix,
            now,
            Some(expected),
        )?;
        let key = self.device.key().await.map_err(|_| Failure::State)?;
        let Some(key) = key else {
            return Err(Failure::NotEnrolled);
        };
        if key.fingerprint != record.fingerprint
            || !verify_signature(&record.payload, signature, &key)
        {
            return Err(self.record_failure(principal, now));
        }
        self.limiter.success(principal, now);
        Ok(())
    }

    fn consume(
        &self,
        principal: &str,
        purpose: Purpose,
        challenge: &str,
        now_unix: u64,
        now: Instant,
        expected: Option<&Operation>,
    ) -> Result<ChallengeRecord, Failure> {
        let mut challenges = self
            .challenges
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        challenges.consumed.retain(|_, expires| *expires > now_unix);
        if challenges.consumed.contains_key(challenge) {
            return Err(Failure::Invalid);
        }
        let Some(record) = challenges.entries.remove(challenge) else {
            return Err(Failure::Invalid);
        };
        challenges
            .consumed
            .insert(challenge.to_owned(), now_unix + CHALLENGE_TTL_SECONDS);
        write_challenges(&self.state_path, &challenges).map_err(|_| Failure::State)?;
        drop(challenges);
        if record.expires_at <= now {
            return Err(Failure::Expired);
        }
        if record.principal != principal || record.purpose != purpose {
            return Err(Failure::Invalid);
        }
        if let Some(expected) = expected
            && !matches(&record, expected)
        {
            return Err(Failure::Invalid);
        }
        Ok(record)
    }

    fn record_failure(&self, principal: &str, now: Instant) -> Failure {
        self.limiter
            .failure(principal, now)
            .map_or(Failure::Invalid, |retry_after| Failure::RateLimited {
                retry_after,
            })
    }
}

fn matches(record: &ChallengeRecord, expected: &Operation) -> bool {
    record.purpose == expected.purpose
        && record.path == expected.path
        && record.key == expected.key
        && record.version == expected.version
        && record.cas == expected.cas
}

fn verify_signature(payload: &str, signature: &str, key: &DeviceKey) -> bool {
    if signature.is_empty() || signature.len() > MAX_SIGNATURE_CHARS {
        return false;
    }
    let Ok(bytes) = URL_SAFE_NO_PAD_INDIFFERENT.decode(signature) else {
        return false;
    };
    let Ok(signature) = Signature::from_der(&bytes) else {
        return false;
    };
    key.verifying.verify(payload.as_bytes(), &signature).is_ok()
}

#[must_use]
pub fn canonical(
    challenge: &str,
    principal: &str,
    operation: &Operation,
    expires_at: u64,
) -> String {
    format!(
        "home-secrets-step-up/1\nchallenge={challenge}\nprincipal={principal}\naction={}\npath={}\nkey={}\nversion={}\ncas={}\nexpires={expires_at}\n",
        operation.purpose.as_str(),
        text(operation.path.as_deref()),
        text(operation.key.as_deref()),
        number(operation.version),
        number(operation.cas),
    )
}

fn text(value: Option<&str>) -> &str {
    value.unwrap_or_default()
}

fn number(value: Option<u64>) -> String {
    value.map_or_else(String::new, |value| value.to_string())
}

#[must_use]
pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn read_challenges(path: &Path) -> Result<Challenges, Error> {
    match std::fs::read_to_string(path) {
        Ok(text) if text.trim().is_empty() => Ok(Challenges::default()),
        Ok(text) => serde_json::from_str(&text).map_err(|_| {
            Error::State(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "step-up state is not JSON",
            ))
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Challenges::default()),
        Err(error) => Err(Error::State(error)),
    }
}

fn write_challenges(path: &Path, challenges: &Challenges) -> std::io::Result<()> {
    if path.as_os_str().is_empty() {
        return Ok(());
    }
    let body = serde_json::to_vec(challenges)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, body)?;
    std::fs::rename(temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn purposes_round_trip_and_reject_unknown_values() {
        for purpose in [
            Purpose::Session,
            Purpose::Update,
            Purpose::Rollback,
            Purpose::Delete,
        ] {
            assert_eq!(Purpose::parse(purpose.as_str()), Some(purpose));
        }
        assert_eq!(Purpose::parse("destroy"), None);
    }

    #[test]
    fn a_payload_binds_every_operation_field() {
        let operation = Operation {
            purpose: Purpose::Update,
            path: Some("k8s/test".to_owned()),
            key: Some("password".to_owned()),
            version: None,
            cas: Some(3),
        };
        let payload = canonical("challenge", "owner", &operation, 42);
        assert!(payload.contains("action=update"));
        assert!(payload.contains("path=k8s/test"));
        assert!(payload.contains("key=password"));
        assert!(payload.contains("version=\n"));
        assert!(payload.contains("cas=3"));
        assert!(payload.contains("expires=42"));
        let other = canonical("challenge", "owner", &Operation::session(), 42);
        assert_ne!(payload, other);
    }

    fn device_store() -> Arc<DeviceStore> {
        let openbao = Arc::new(
            crate::openbao::OpenBao::new(
                "http://127.0.0.1:1",
                "role",
                home_secret::Token::new("jwt"),
                Duration::from_secs(1),
            )
            .expect("openbao"),
        );
        Arc::new(DeviceStore::new(openbao, "device/test"))
    }

    fn challenge(expires_at: Instant) -> ChallengeRecord {
        ChallengeRecord {
            principal: "owner".to_owned(),
            purpose: Purpose::Session,
            path: None,
            key: None,
            version: None,
            cas: None,
            payload: "payload".to_owned(),
            expires_at,
            fingerprint: "fingerprint".to_owned(),
        }
    }

    #[test]
    fn a_malformed_signature_is_rejected_without_a_panic() {
        use p256::pkcs8::EncodePublicKey as _;
        let key = p256::ecdsa::SigningKey::from_slice(&[7u8; 32]).expect("key");
        let encoded = key.verifying_key().to_public_key_der().expect("der");
        let device = DeviceKey {
            device_id: "device".to_owned(),
            fingerprint: "fingerprint".to_owned(),
            verifying: *key.verifying_key(),
        };
        assert!(!verify_signature("payload", "not-base64!", &device));
        assert!(!verify_signature("payload", "", &device));
        assert!(!verify_signature(
            "payload",
            &crate::device::encode_base64(encoded.as_bytes()),
            &device
        ));
    }

    #[test]
    fn an_expired_challenge_is_rejected() {
        let stepup = StepUp::new(device_store());
        let now = Instant::now();
        stepup.challenges.lock().expect("lock").entries.insert(
            "expired".to_owned(),
            challenge(now - Duration::from_secs(1)),
        );
        assert_eq!(
            stepup
                .consume("owner", Purpose::Session, "expired", 0, now, None)
                .err(),
            Some(Failure::Expired)
        );
    }

    #[test]
    fn a_consumed_challenge_survives_a_restart() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("step-up.json");
        let now = Instant::now();
        let first = StepUp::with_state(device_store(), &path).expect("step-up");
        first
            .challenges
            .lock()
            .expect("lock")
            .entries
            .insert("once".to_owned(), challenge(now + Duration::from_secs(60)));
        assert!(
            first
                .consume("owner", Purpose::Session, "once", 1_000, now, None)
                .is_ok()
        );
        drop(first);
        let restarted = StepUp::with_state(device_store(), &path).expect("step-up");
        assert_eq!(
            restarted
                .consume("owner", Purpose::Session, "once", 1_000, now, None)
                .err(),
            Some(Failure::Invalid)
        );
    }
}
