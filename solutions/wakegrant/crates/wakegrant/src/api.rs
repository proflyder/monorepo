//! axum HTTP API for the wakegrant protocol.
//!
//! Routes, auth, error shapes and long-poll behaviour follow
//! `packages/contracts/wakegrant/PROTOCOL.md`; the bearer check, body limit,
//! request timeout and health route come from `home-http`.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{ConnectInfo, Extension, Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use home_http::{healthz, with_bearer, with_body_limit, with_timeout};
use home_secret::Token;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tracing::{info, warn};
use wakegrant_protocol::{
    Purpose as WirePurpose, claim_mac_message, claim_mac_message_for_purpose, constant_time_equal,
    grant_mac_message, grant_mac_message_for_purpose, hmac_sha256_b64, is_valid_machine,
};

use crate::config::Keys;
use crate::domain::{
    Clock, NonceStore, Purpose, RateLimiter, Waiters, validate_ttl, validate_wait,
};
use crate::store::{Grant, Store};

/// Maximum request body: the protocol only carries short JSON objects.
pub const MAX_BODY_BYTES: usize = 4 * 1024;
/// Long-poll claims may wait 25 s, so the request timeout comfortably exceeds
/// it, matching the Go service's 40 s write deadline.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(40);
/// Health check budget.
pub const HEALTH_TIMEOUT: Duration = Duration::from_secs(2);

/// Peer address captured by [`peer_ip`]; `None` in tests that call the router
/// directly without `ConnectInfo`.
#[derive(Debug, Clone, Copy)]
pub struct PeerIp(pub Option<IpAddr>);

async fn peer_ip(mut request: Request, next: Next) -> Response {
    let ip = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(address)| address.ip());
    request.extensions_mut().insert(PeerIp(ip));
    next.run(request).await
}

/// Construction options.
pub struct Options {
    pub keys: Keys,
    pub issuer_token: Token,
    pub store: Arc<Store>,
    pub max_ttl: Duration,
    pub clock: Arc<dyn Clock>,
}

#[derive(Debug, Error)]
enum LongPollError {
    #[error("too many concurrent waiters")]
    TooManyWaiters,
    #[error("storage error")]
    Storage,
}

/// The wakegrant HTTP service.
pub struct Service {
    keys: Keys,
    token: Token,
    store: Arc<Store>,
    max_ttl: Duration,
    clock: Arc<dyn Clock>,
    nonces: NonceStore,
    limiter: RateLimiter,
    waiters: Arc<Waiters>,
}

impl Service {
    #[must_use]
    pub fn new(options: Options) -> Self {
        Self {
            keys: options.keys,
            token: options.issuer_token,
            store: options.store,
            max_ttl: options.max_ttl,
            clock: options.clock,
            nonces: NonceStore::new(),
            limiter: RateLimiter::challenge(),
            waiters: Arc::new(Waiters::new()),
        }
    }

    #[cfg(test)]
    pub(crate) fn nonces(&self) -> &NonceStore {
        &self.nonces
    }

    /// Full router with health, body limit and timeout.
    pub fn router(self: &Arc<Self>) -> Router {
        let grants = Router::new()
            .route("/v1/grants", post(handle_grant))
            .with_state(Arc::clone(self));
        let grants = with_bearer(grants, self.token.clone());
        let open = Router::new()
            .route("/v1/challenge", post(handle_challenge))
            .route("/v1/claim", post(handle_claim))
            .route("/healthz", get(handle_health))
            .with_state(Arc::clone(self));
        let router = open.merge(grants).layer(middleware::from_fn(peer_ip));
        let router = with_body_limit(router, MAX_BODY_BYTES);
        with_timeout(router, REQUEST_TIMEOUT)
    }

    fn now(&self) -> OffsetDateTime {
        self.clock.now()
    }

    fn limit(&self, prefix: &str, machine: &str, ip: Option<IpAddr>, now: OffsetDateTime) -> bool {
        self.limiter.prune(now);
        if !self
            .limiter
            .allow(&format!("{prefix}machine:{machine}"), now)
        {
            return false;
        }
        ip.is_none_or(|ip| self.limiter.allow(&format!("{prefix}ip:{ip}"), now))
    }

    /// Emits exactly event, outcome, machine and grantId — never nonces, MACs,
    /// tokens, IPs or keys.
    fn log_outcome(&self, event: &str, machine: &str, outcome: &str, grant_id: &str) {
        let valid_machine = is_valid_machine(machine);
        match (valid_machine, grant_id.is_empty()) {
            (true, true) => info!(event, outcome, machine, "wakegrant"),
            (true, false) => info!(event, outcome, machine, grantId = grant_id, "wakegrant"),
            (false, true) => info!(event, outcome, "wakegrant"),
            (false, false) => info!(event, outcome, grantId = grant_id, "wakegrant"),
        }
    }

    async fn long_poll(
        &self,
        machine: &str,
        purpose: Purpose,
        wait_seconds: u32,
    ) -> Result<Option<String>, LongPollError> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(u64::from(wait_seconds));
        loop {
            let waiter = self
                .waiters
                .acquire(machine, purpose)
                .ok_or(LongPollError::TooManyWaiters)?;
            let notified = waiter.notified();
            tokio::pin!(notified);
            // Register before re-reading the store so a grant committed
            // between the first lookup and this point cannot be missed.
            notified.as_mut().enable();

            match self
                .store
                .consume_newest(machine, purpose, self.now())
                .await
            {
                Ok(Some(grant_id)) => return Ok(Some(grant_id)),
                Ok(None) => {}
                Err(cause) => {
                    warn!(%cause, "wakegrant storage error while waiting");
                    return Err(LongPollError::Storage);
                }
            }

            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }
            tokio::select! {
                () = &mut notified => {}
                () = tokio::time::sleep(remaining) => return Ok(None),
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct GrantRequest {
    #[serde(default)]
    machine: String,
    #[serde(rename = "ttlSeconds", default)]
    ttl_seconds: Option<i64>,
    #[serde(default)]
    purpose: Option<String>,
}

#[derive(Debug, Serialize)]
struct GrantResponse {
    #[serde(rename = "grantId")]
    grant_id: String,
    #[serde(rename = "expiresAt")]
    expires_at: String,
}

#[derive(Debug, Deserialize)]
struct ChallengeRequest {
    #[serde(default)]
    machine: String,
}

#[derive(Debug, Serialize)]
struct ChallengeResponse {
    nonce: String,
    #[serde(rename = "expiresIn")]
    expires_in: u64,
}

#[derive(Debug, Deserialize)]
struct ClaimRequest {
    #[serde(default)]
    machine: String,
    #[serde(default)]
    nonce: String,
    #[serde(default)]
    mac: String,
    #[serde(default)]
    purpose: Option<String>,
    #[serde(rename = "waitSeconds", default)]
    wait_seconds: Option<i64>,
}

#[derive(Debug, Serialize)]
struct ClaimResponse {
    granted: bool,
    #[serde(rename = "grantId", skip_serializing_if = "Option::is_none")]
    grant_id: Option<String>,
    mac: String,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: &'static str,
}

fn error(status: StatusCode) -> Response {
    (
        status,
        Json(ErrorResponse {
            error: status.canonical_reason().unwrap_or("Error"),
        }),
    )
        .into_response()
}

fn decode<T: DeserializeOwned>(body: &Bytes) -> Option<T> {
    serde_json::from_slice(body).ok()
}

fn wire_purpose(purpose: Purpose) -> WirePurpose {
    match purpose {
        Purpose::Logon => WirePurpose::Logon,
        Purpose::Unlock => WirePurpose::Unlock,
    }
}

fn parse_purpose(value: Option<&str>) -> Result<(Purpose, bool), ()> {
    value.map_or(Ok((Purpose::Logon, false)), |text| {
        Purpose::parse(text)
            .map(|purpose| (purpose, true))
            .map_err(|_| ())
    })
}

async fn handle_grant(State(service): State<Arc<Service>>, body: Bytes) -> Response {
    let now = service.now();
    let Some(request) = decode::<GrantRequest>(&body) else {
        service.log_outcome("grant", "", "bad-request", "");
        return error(StatusCode::BAD_REQUEST);
    };
    if !is_valid_machine(&request.machine) || !service.keys.contains_key(&request.machine) {
        service.log_outcome("grant", &request.machine, "unknown-machine", "");
        return error(StatusCode::BAD_REQUEST);
    }
    let Ok((purpose, _)) = parse_purpose(request.purpose.as_deref()) else {
        service.log_outcome("grant", &request.machine, "bad-purpose", "");
        return error(StatusCode::BAD_REQUEST);
    };
    let Ok(ttl) = validate_ttl(request.ttl_seconds, service.max_ttl) else {
        service.log_outcome("grant", &request.machine, "bad-ttl", "");
        return error(StatusCode::BAD_REQUEST);
    };
    let grant: Grant = match service
        .store
        .create_grant(&request.machine, purpose, now, ttl)
        .await
    {
        Ok(grant) => grant,
        Err(cause) => {
            warn!(%cause, "wakegrant storage error");
            service.log_outcome("grant", &request.machine, "storage-error", "");
            return error(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    // Wake long-poll claims only after the grant is committed, so a waiter
    // that re-reads the store is guaranteed to see it.
    service.waiters.broadcast(&request.machine, purpose);
    service.log_outcome("grant", &request.machine, "issued", &grant.id);
    let Ok(expires_at) = grant.expires_at.format(&Rfc3339) else {
        return error(StatusCode::INTERNAL_SERVER_ERROR);
    };
    (
        StatusCode::CREATED,
        Json(GrantResponse {
            grant_id: grant.id,
            expires_at,
        }),
    )
        .into_response()
}

async fn handle_challenge(
    State(service): State<Arc<Service>>,
    peer: Extension<PeerIp>,
    body: Bytes,
) -> Response {
    let now = service.now();
    let Some(request) = decode::<ChallengeRequest>(&body) else {
        service.log_outcome("challenge", "", "bad-request", "");
        return error(StatusCode::BAD_REQUEST);
    };
    if !is_valid_machine(&request.machine) || !service.keys.contains_key(&request.machine) {
        service.log_outcome("challenge", &request.machine, "unknown-machine", "");
        return error(StatusCode::BAD_REQUEST);
    }
    if !service.limit("challenge:", &request.machine, peer.0.0, now) {
        service.log_outcome("challenge", &request.machine, "rate-limited", "");
        return error(StatusCode::TOO_MANY_REQUESTS);
    }
    let nonce = match service.nonces.issue(&request.machine, now) {
        Ok(nonce) => nonce,
        Err(cause) => {
            warn!(%cause, "wakegrant nonce error");
            service.log_outcome("challenge", &request.machine, "internal-error", "");
            return error(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    service.log_outcome("challenge", &request.machine, "issued", "");
    Json(ChallengeResponse {
        nonce,
        expires_in: crate::domain::NONCE_TTL.as_secs(),
    })
    .into_response()
}

async fn handle_claim(
    State(service): State<Arc<Service>>,
    peer: Extension<PeerIp>,
    body: Bytes,
) -> Response {
    let now = service.now();
    let Some(request) = decode::<ClaimRequest>(&body) else {
        service.log_outcome("claim", "", "bad-request", "");
        return error(StatusCode::BAD_REQUEST);
    };
    if !is_valid_machine(&request.machine) || request.nonce.is_empty() || request.mac.is_empty() {
        service.log_outcome("claim", "", "bad-request", "");
        return error(StatusCode::BAD_REQUEST);
    }
    let Ok((purpose, purpose_aware)) = parse_purpose(request.purpose.as_deref()) else {
        service.log_outcome("claim", &request.machine, "bad-purpose", "");
        return error(StatusCode::BAD_REQUEST);
    };
    let Ok(wait) = validate_wait(request.wait_seconds, purpose_aware) else {
        service.log_outcome("claim", &request.machine, "bad-wait", "");
        return error(StatusCode::BAD_REQUEST);
    };
    let Some(key) = service.keys.get(&request.machine) else {
        // Never reveal whether the machine exists.
        service.log_outcome("claim", "", "unauthorized", "");
        return error(StatusCode::UNAUTHORIZED);
    };
    if !service.limit("claim:", &request.machine, peer.0.0, now) {
        service.log_outcome("claim", &request.machine, "rate-limited", "");
        return error(StatusCode::TOO_MANY_REQUESTS);
    }

    // The nonce is consumed first: a forged MAC cannot be retried with it.
    if service
        .nonces
        .consume(&request.machine, &request.nonce, now)
        .is_err()
    {
        service.log_outcome("claim", &request.machine, "bad-nonce", "");
        return error(StatusCode::UNAUTHORIZED);
    }
    let message = if purpose_aware {
        claim_mac_message_for_purpose(&request.machine, &request.nonce, wire_purpose(purpose))
    } else {
        claim_mac_message(&request.machine, &request.nonce)
    };
    let expected = hmac_sha256_b64(key, &message);
    if !constant_time_equal(expected.as_bytes(), request.mac.as_bytes()) {
        service.log_outcome("claim", &request.machine, "bad-mac", "");
        return error(StatusCode::UNAUTHORIZED);
    }

    let grant_id = match service
        .store
        .consume_newest(&request.machine, purpose, now)
        .await
    {
        Ok(grant_id) => grant_id,
        Err(cause) => {
            warn!(%cause, "wakegrant storage error");
            service.log_outcome("claim", &request.machine, "storage-error", "");
            return error(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    let grant_id = match (grant_id, wait) {
        (Some(grant_id), _) => Some(grant_id),
        (None, Some(wait)) => match service.long_poll(&request.machine, purpose, wait).await {
            Ok(grant_id) => grant_id,
            Err(LongPollError::TooManyWaiters) => {
                service.log_outcome("claim", &request.machine, "too-many-waiters", "");
                return error(StatusCode::TOO_MANY_REQUESTS);
            }
            Err(LongPollError::Storage) => {
                service.log_outcome("claim", &request.machine, "storage-error", "");
                return error(StatusCode::INTERNAL_SERVER_ERROR);
            }
        },
        (None, None) => None,
    };

    if let Some(grant_id) = grant_id {
        service.log_outcome("claim", &request.machine, "granted", &grant_id);
        let mac = if purpose_aware {
            granted_mac(&request.machine, &request.nonce, &grant_id, purpose, key)
        } else {
            hmac_sha256_b64(
                key,
                &grant_mac_message(&request.machine, &request.nonce, true, &grant_id),
            )
        };
        return Json(ClaimResponse {
            granted: true,
            grant_id: Some(grant_id),
            mac,
        })
        .into_response();
    }

    service.log_outcome("claim", &request.machine, "denied", "");
    let mac = if purpose_aware {
        denied_mac(&request.machine, &request.nonce, purpose, key)
    } else {
        hmac_sha256_b64(
            key,
            &grant_mac_message(&request.machine, &request.nonce, false, ""),
        )
    };
    Json(ClaimResponse {
        granted: false,
        grant_id: None,
        mac,
    })
    .into_response()
}

fn granted_mac(machine: &str, nonce: &str, grant_id: &str, purpose: Purpose, key: &[u8]) -> String {
    hmac_sha256_b64(
        key,
        &grant_mac_message_for_purpose(machine, nonce, true, grant_id, wire_purpose(purpose)),
    )
}

fn denied_mac(machine: &str, nonce: &str, purpose: Purpose, key: &[u8]) -> String {
    hmac_sha256_b64(
        key,
        &grant_mac_message_for_purpose(machine, nonce, false, "", wire_purpose(purpose)),
    )
}

async fn handle_health(State(service): State<Arc<Service>>) -> Response {
    match tokio::time::timeout(HEALTH_TIMEOUT, service.store.ping()).await {
        Ok(Ok(())) => healthz().await.into_response(),
        _ => (
            StatusCode::SERVICE_UNAVAILABLE,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            "unavailable\n",
        )
            .into_response(),
    }
}
