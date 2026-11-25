use axum::http::{HeaderMap, StatusCode, header};
use axum::response::Response;
use home_app_auth::{Module, Principal};
use tokio::time::Instant;

use super::audit_scope::AuditScope;
use super::reply::{Rejection, error, forbidden, rate_limited};
use super::validate::header_value;
use super::{GatewayState, STEP_UP_CHALLENGE_HEADER, STEP_UP_SIGNATURE_HEADER};
use crate::audit::Action;
use crate::path;
use crate::stepup::{Failure, Operation, now_unix};

pub(super) async fn authorized(
    state: &GatewayState,
    headers: &HeaderMap,
) -> Result<Principal, Rejection> {
    let Some(principal) = state
        .access
        .authenticate(headers.get(header::AUTHORIZATION))
        .await
    else {
        return Err(error(StatusCode::UNAUTHORIZED, "unauthenticated").into());
    };
    principal
        .require(Module::Secrets)
        .map_err(|denied| Rejection::from(forbidden(&denied)))?;
    if let Ok(Some(device_id)) = state.device.poll_revocation(Instant::now()).await {
        AuditScope::new(state, &principal, Action::DeviceRevoke).record_detail(
            None,
            "ok",
            Some(&device_id),
        );
    }
    Ok(principal)
}

pub(super) fn resolve(
    state: &GatewayState,
    scope: AuditScope<'_>,
    raw: &str,
    version: Option<u64>,
) -> Result<String, Rejection> {
    let Ok(path) = path::normalize(raw) else {
        return Err(scope.at("<invalid>").reject(
            version,
            "invalid_path",
            error(StatusCode::BAD_REQUEST, "invalid_path"),
        ));
    };
    if state.is_hidden(&path) {
        return Err(scope.at(&path).reject(
            version,
            "path_forbidden",
            error(StatusCode::FORBIDDEN, "path_forbidden"),
        ));
    }
    Ok(path)
}

pub(super) async fn require_mutation(
    state: &GatewayState,
    scope: AuditScope<'_>,
    headers: &HeaderMap,
    operation: &Operation,
    version: Option<u64>,
) -> Result<(), Rejection> {
    let challenge = header_value(headers, STEP_UP_CHALLENGE_HEADER);
    let signature = header_value(headers, STEP_UP_SIGNATURE_HEADER);
    if challenge.is_empty() || signature.is_empty() {
        return Err(scope.reject(
            version,
            "step_up_required",
            error(StatusCode::FORBIDDEN, "step_up_required"),
        ));
    }
    state
        .stepup
        .authorize_mutation(
            scope.principal(),
            operation,
            challenge,
            signature,
            now_unix(),
            Instant::now(),
        )
        .await
        .map_err(|failure| reject_step_up(scope, version, failure))
}

pub(super) fn reject_step_up(
    scope: AuditScope<'_>,
    version: Option<u64>,
    failure: Failure,
) -> Rejection {
    let (outcome, response) = step_up_failure(failure);
    scope.reject(version, outcome, response)
}

fn step_up_failure(failure: Failure) -> (&'static str, Response) {
    match failure {
        Failure::RateLimited { retry_after } => ("rate_limited", rate_limited(retry_after)),
        Failure::State => (
            "state_unavailable",
            error(StatusCode::SERVICE_UNAVAILABLE, "state_unavailable"),
        ),
        Failure::NotEnrolled => (
            "device_not_enrolled",
            error(StatusCode::FORBIDDEN, "device_not_enrolled"),
        ),
        Failure::Expired => (
            "step_up_expired",
            error(StatusCode::FORBIDDEN, "step_up_expired"),
        ),
        Failure::Invalid => (
            "step_up_invalid",
            error(StatusCode::FORBIDDEN, "step_up_invalid"),
        ),
    }
}
