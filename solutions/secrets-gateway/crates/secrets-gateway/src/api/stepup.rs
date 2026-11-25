use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use home_app_auth::Principal;
use serde_json::json;
use tokio::time::Instant;

use super::GatewayState;
use super::audit_scope::AuditScope;
use super::guard::{authorized, reject_step_up, resolve};
use super::reply::{Rejection, Reply, error, no_store};
use super::request::{ChallengeBody, StepUpBody};
use super::validate::{audit_key, valid_hash};
use crate::audit::Action;
use crate::stepup::{Operation, Purpose, now_unix};

pub(super) async fn challenge(
    State(state): State<GatewayState>,
    headers: HeaderMap,
    Json(body): Json<ChallengeBody>,
) -> Reply {
    let principal = authorized(&state, &headers).await?;
    let purpose = Purpose::parse(&body.action)
        .ok_or_else(|| Rejection::from(error(StatusCode::BAD_REQUEST, "invalid_action")))?;
    let operation = operation_from(&state, &principal, purpose, &body)?;
    let view = state
        .stepup
        .issue(&principal.id, operation, now_unix(), Instant::now())
        .await
        .map_err(|failure| {
            reject_step_up(
                AuditScope::new(&state, &principal, Action::StepUp),
                None,
                failure,
            )
        })?;
    Ok(no_store(json!({
        "challenge": view.challenge,
        "payload": view.payload,
        "expiresIn": view.expires_in,
    })))
}

pub(super) async fn open_session(
    State(state): State<GatewayState>,
    headers: HeaderMap,
    Json(body): Json<StepUpBody>,
) -> Reply {
    let principal = authorized(&state, &headers).await?;
    let scope = AuditScope::new(&state, &principal, Action::StepUp);
    let (session, expires_in) = state
        .stepup
        .open_session(
            &principal.id,
            &body.challenge,
            &body.signature,
            now_unix(),
            Instant::now(),
        )
        .await
        .map_err(|failure| reject_step_up(scope, None, failure))?;
    scope.record(None, "ok");
    Ok(no_store(
        json!({"session": session, "expiresIn": expires_in}),
    ))
}

fn operation_from(
    state: &GatewayState,
    principal: &Principal,
    purpose: Purpose,
    body: &ChallengeBody,
) -> Result<Operation, Rejection> {
    match purpose {
        Purpose::Session => session_operation(body),
        Purpose::Update => update_operation(state, principal, body),
        Purpose::Rollback => rollback_operation(state, principal, body),
        Purpose::Delete => delete_operation(state, principal, body),
    }
}

fn invalid_challenge() -> Rejection {
    error(StatusCode::BAD_REQUEST, "invalid_challenge").into()
}

fn session_operation(body: &ChallengeBody) -> Result<Operation, Rejection> {
    let scoped =
        body.path.is_some() || body.key.is_some() || body.version.is_some() || body.cas.is_some();
    if scoped {
        return Err(invalid_challenge());
    }
    Ok(Operation::session())
}

fn update_operation(
    state: &GatewayState,
    principal: &Principal,
    body: &ChallengeBody,
) -> Result<Operation, Rejection> {
    let (Some(raw), Some(key), Some(cas), None) = (
        body.path.as_deref(),
        body.key.as_deref(),
        body.cas,
        body.version,
    ) else {
        return Err(invalid_challenge());
    };
    let value_hash = body
        .value_hash
        .as_deref()
        .filter(|hash| valid_hash(hash))
        .ok_or_else(invalid_challenge)?;
    let scope = AuditScope::new(state, principal, Action::Update).with_key(Some(audit_key(key)));
    let path = resolve(state, scope, raw, None)?;
    Ok(Operation {
        purpose: Purpose::Update,
        path: Some(path),
        key: Some(key.to_owned()),
        version: None,
        cas: Some(cas),
        value_hash: Some(value_hash.to_ascii_lowercase()),
    })
}

fn rollback_operation(
    state: &GatewayState,
    principal: &Principal,
    body: &ChallengeBody,
) -> Result<Operation, Rejection> {
    let (Some(raw), Some(version), Some(cas), None) = (
        body.path.as_deref(),
        body.version,
        body.cas,
        body.key.as_deref(),
    ) else {
        return Err(invalid_challenge());
    };
    let scope = AuditScope::new(state, principal, Action::Rollback);
    let path = resolve(state, scope, raw, Some(version))?;
    Ok(Operation {
        purpose: Purpose::Rollback,
        path: Some(path),
        key: None,
        version: Some(version),
        cas: Some(cas),
        value_hash: None,
    })
}

fn delete_operation(
    state: &GatewayState,
    principal: &Principal,
    body: &ChallengeBody,
) -> Result<Operation, Rejection> {
    let (Some(raw), Some(cas), None, None) = (
        body.path.as_deref(),
        body.cas,
        body.key.as_deref(),
        body.value_hash.as_deref(),
    ) else {
        return Err(invalid_challenge());
    };
    let scope = AuditScope::new(state, principal, Action::Delete);
    let path = resolve(state, scope, raw, body.version)?;
    Ok(Operation {
        purpose: Purpose::Delete,
        path: Some(path),
        key: None,
        version: body.version,
        cas: Some(cas),
        value_hash: None,
    })
}
