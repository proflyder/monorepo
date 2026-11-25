use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use serde_json::json;
use tokio::time::Instant;

use super::GatewayState;
use super::audit_scope::AuditScope;
use super::guard::authorized;
use super::reply::{Rejection, Reply, error, no_store, rate_limited};
use super::request::EnrollBody;
use crate::audit::Action;
use crate::device::{self, Enrollment};
use crate::stepup::now_unix;

pub(super) async fn status(State(state): State<GatewayState>, headers: HeaderMap) -> Reply {
    authorized(&state, &headers).await?;
    let device = state.device.status().await.map_err(|_| unavailable())?;
    Ok(no_store(device.map_or_else(
        || json!({"enrolled": false}),
        |device| {
            json!({
                "enrolled": true,
                "deviceId": device.device_id,
                "enrolledAt": device.enrolled_at,
                "publicKey": device.public_key,
            })
        },
    )))
}

pub(super) async fn challenge(State(state): State<GatewayState>, headers: HeaderMap) -> Reply {
    let principal = authorized(&state, &headers).await?;
    let (challenge, expires_in) = state
        .device
        .attestation_challenge(&principal.id, now_unix())
        .map_err(|_| unavailable())?;
    Ok(no_store(
        json!({"challenge": challenge, "expiresIn": expires_in}),
    ))
}

pub(super) async fn enroll(
    State(state): State<GatewayState>,
    headers: HeaderMap,
    Json(body): Json<EnrollBody>,
) -> Reply {
    let principal = authorized(&state, &headers).await?;
    let scope = AuditScope::new(&state, &principal, Action::Enroll);
    let enrollment = Enrollment {
        code: &body.code,
        public_key: &body.public_key,
        challenge: &body.attestation_challenge,
        attestation_chain: &body.attestation_chain,
    };
    let device = state
        .device
        .enroll(&principal.id, enrollment, now_unix(), Instant::now())
        .await
        .map_err(|failure| {
            let rejection = EnrollRejection::from(failure);
            scope.record_detail(None, rejection.outcome, rejection.detail);
            Rejection::from(rejection.response)
        })?;
    scope.record(None, "ok");
    Ok(no_store(json!({
        "enrolled": true,
        "deviceId": device.device_id,
        "enrolledAt": device.enrolled_at,
    })))
}

fn unavailable() -> Rejection {
    error(StatusCode::SERVICE_UNAVAILABLE, "device_unavailable").into()
}

struct EnrollRejection {
    outcome: &'static str,
    detail: Option<&'static str>,
    response: Response,
}

impl EnrollRejection {
    fn invalid(detail: &'static str) -> Self {
        Self {
            outcome: "enrollment_invalid",
            detail: Some(detail),
            response: error(StatusCode::FORBIDDEN, "enrollment_invalid"),
        }
    }

    fn plain(outcome: &'static str, status: StatusCode) -> Self {
        Self {
            outcome,
            detail: None,
            response: error(status, outcome),
        }
    }
}

impl From<device::Error> for EnrollRejection {
    fn from(failure: device::Error) -> Self {
        match failure {
            device::Error::EnrollmentClosed => Self::invalid("enrollment_closed"),
            device::Error::CodeExpired => Self::invalid("enrollment_expired"),
            device::Error::CodeInvalid => Self::invalid("enrollment_invalid"),
            device::Error::ChallengeInvalid => Self::invalid("attestation_challenge_invalid"),
            device::Error::KeyInvalid => Self::invalid("public_key_invalid"),
            device::Error::AttestationInvalid => Self::invalid("attestation_invalid"),
            device::Error::Conflict => Self::plain("enrollment_conflict", StatusCode::CONFLICT),
            device::Error::RateLimited { retry_after } => Self {
                outcome: "rate_limited",
                detail: None,
                response: rate_limited(retry_after),
            },
            device::Error::NotEnrolled => Self::plain("device_not_enrolled", StatusCode::FORBIDDEN),
            device::Error::Unavailable => {
                Self::plain("device_unavailable", StatusCode::SERVICE_UNAVAILABLE)
            }
        }
    }
}
