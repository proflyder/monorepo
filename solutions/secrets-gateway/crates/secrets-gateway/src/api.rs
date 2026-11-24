use std::collections::BTreeMap;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use home_app_auth::{AccessClient, Forbidden, Module, Principal};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::time::Instant;

use crate::audit::{Action, Auditor, Event};
use crate::catalog::Catalog;
use crate::deny;
use crate::openbao::{self, OpenBao};
use crate::path;
use crate::stepup::{Failure, StepUp};
use crate::totp;

pub const STEP_UP_SESSION_HEADER: &str = "x-step-up-session";
pub const STEP_UP_TOTP_HEADER: &str = "x-step-up-totp";
pub const MAX_KEY_CHARS: usize = 128;
pub const MAX_VALUE_BYTES: usize = 32 * 1024;

#[derive(Clone)]
pub struct GatewayState {
    pub access: Arc<AccessClient>,
    pub openbao: Arc<OpenBao>,
    pub catalog: Arc<Catalog>,
    pub stepup: Arc<StepUp>,
    pub audit: Arc<Auditor>,
    pub max_list_entries: usize,
}

impl std::fmt::Debug for GatewayState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GatewayState")
            .field("max_list_entries", &self.max_list_entries)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EntryView {
    path: String,
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    consumer: Option<String>,
    live: bool,
    in_catalog: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RevealBody {
    key: String,
    #[serde(default)]
    version: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UpdateBody {
    key: String,
    value: String,
    cas: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RollbackBody {
    version: u64,
    cas: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StepUpBody {
    totp: String,
}

pub fn router(state: GatewayState) -> Router {
    Router::new()
        .route("/v1/secrets", get(list))
        .route("/v1/secrets/meta/{*path}", get(metadata))
        .route("/v1/secrets/reveal/{*path}", post(reveal))
        .route("/v1/secrets/update/{*path}", post(update))
        .route("/v1/secrets/rollback/{*path}", post(rollback))
        .route("/v1/secrets/{*path}", axum::routing::delete(delete_latest))
        .route("/v1/step-up", post(step_up))
        .with_state(state)
}

#[allow(clippy::result_large_err)]
async fn authorized(state: &GatewayState, headers: &HeaderMap) -> Result<Principal, Response> {
    let Some(principal) = state
        .access
        .authenticate(headers.get(header::AUTHORIZATION))
        .await
    else {
        return Err(error(StatusCode::UNAUTHORIZED, "unauthenticated"));
    };
    match principal.require(Module::Secrets) {
        Ok(()) => Ok(principal),
        Err(forbidden) => Err(forbidden_response(forbidden)),
    }
}

async fn list(State(state): State<GatewayState>, headers: HeaderMap) -> Response {
    let principal = match authorized(&state, &headers).await {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let live = match state.openbao.list_all(state.max_list_entries).await {
        Ok(paths) => paths,
        Err(failure) => {
            state.audit.record(&Event {
                action: Action::List,
                principal: &principal.id,
                path: None,
                key: None,
                version: None,
                outcome: "error",
                detail: Some("openbao"),
            });
            return openbao_error(failure);
        }
    };
    let mut entries: BTreeMap<String, EntryView> = BTreeMap::new();
    for (path, catalog) in state.catalog.iter() {
        if deny::is_denied(path) {
            continue;
        }
        if path::normalize(path).is_err() {
            continue;
        }
        let key = path.to_owned();
        entries.insert(
            key.clone(),
            EntryView {
                path: key,
                description: catalog.purpose.clone(),
                consumer: catalog.consumer.clone(),
                live: false,
                in_catalog: true,
            },
        );
    }
    for path in live {
        if deny::is_denied(&path) {
            continue;
        }
        if path::normalize(&path).is_err() {
            continue;
        }
        let entry = entries.entry(path.clone()).or_insert_with(|| EntryView {
            path: path.clone(),
            description: String::new(),
            consumer: None,
            live: false,
            in_catalog: false,
        });
        entry.live = true;
    }
    state.audit.record(&Event {
        action: Action::List,
        principal: &principal.id,
        path: None,
        key: None,
        version: None,
        outcome: "ok",
        detail: None,
    });
    Json(json!({"entries": entries.into_values().collect::<Vec<_>>()})).into_response()
}

async fn metadata(
    State(state): State<GatewayState>,
    Path(raw): Path<String>,
    headers: HeaderMap,
) -> Response {
    let principal = match authorized(&state, &headers).await {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let path = match resolve(&raw) {
        Ok(path) => path,
        Err(response) => return response,
    };
    let meta = match state.openbao.metadata(&path).await {
        Ok(meta) => meta,
        Err(failure) => {
            audit_outcome(
                &state,
                &principal,
                Action::Metadata,
                &path,
                None,
                None,
                "error",
            );
            return openbao_error(failure);
        }
    };
    let keys = match meta
        .versions
        .iter()
        .find(|item| item.version == meta.current_version)
    {
        Some(item) if !item.destroyed && item.deletion_time.is_empty() => {
            match state.openbao.read(&path, Some(meta.current_version)).await {
                Ok(secret) => secret.data.keys().cloned().collect::<Vec<_>>(),
                Err(openbao::Error::NotFound) => Vec::new(),
                Err(failure) => {
                    audit_outcome(
                        &state,
                        &principal,
                        Action::Metadata,
                        &path,
                        None,
                        Some(meta.current_version),
                        "error",
                    );
                    return openbao_error(failure);
                }
            }
        }
        _ => Vec::new(),
    };
    state.audit.record(&Event {
        action: Action::Metadata,
        principal: &principal.id,
        path: Some(&path),
        key: None,
        version: Some(meta.current_version),
        outcome: "ok",
        detail: None,
    });
    Json(json!({
        "path": path,
        "currentVersion": meta.current_version,
        "oldestVersion": meta.oldest_version,
        "versions": meta.versions,
        "keys": keys,
    }))
    .into_response()
}

async fn reveal(
    State(state): State<GatewayState>,
    Path(raw): Path<String>,
    headers: HeaderMap,
    Json(body): Json<RevealBody>,
) -> Response {
    let principal = match authorized(&state, &headers).await {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let path = match resolve(&raw) {
        Ok(path) => path,
        Err(response) => return response,
    };
    let session = header_value(&headers, STEP_UP_SESSION_HEADER);
    if session.is_empty()
        || !state
            .stepup
            .verify_session(&principal.id, session, Instant::now())
    {
        state.audit.record(&Event {
            action: Action::Reveal,
            principal: &principal.id,
            path: Some(&path),
            key: Some(&body.key),
            version: body.version,
            outcome: "step_up_required",
            detail: None,
        });
        return error(StatusCode::FORBIDDEN, "step_up_required");
    }
    if !valid_key(&body.key) {
        audit_outcome(
            &state,
            &principal,
            Action::Reveal,
            &path,
            Some(&body.key),
            body.version,
            "invalid_key",
        );
        return error(StatusCode::BAD_REQUEST, "invalid_key");
    }
    let Some(version) = body.version else {
        audit_outcome(
            &state,
            &principal,
            Action::Reveal,
            &path,
            Some(&body.key),
            None,
            "version_required",
        );
        return error(StatusCode::BAD_REQUEST, "version_required");
    };
    let meta = match state.openbao.metadata(&path).await {
        Ok(meta) => meta,
        Err(failure) => {
            audit_outcome(
                &state,
                &principal,
                Action::Reveal,
                &path,
                Some(&body.key),
                Some(version),
                "error",
            );
            return openbao_error(failure);
        }
    };
    let Some(info) = meta.versions.iter().find(|item| item.version == version) else {
        audit_outcome(
            &state,
            &principal,
            Action::Reveal,
            &path,
            Some(&body.key),
            Some(version),
            "version_not_found",
        );
        return error(StatusCode::NOT_FOUND, "version_not_found");
    };
    if info.destroyed || !info.deletion_time.is_empty() {
        audit_outcome(
            &state,
            &principal,
            Action::Reveal,
            &path,
            Some(&body.key),
            Some(version),
            "version_unavailable",
        );
        return error(StatusCode::CONFLICT, "version_unavailable");
    }
    let secret = match state.openbao.read(&path, Some(version)).await {
        Ok(secret) => secret,
        Err(failure) => {
            audit_outcome(
                &state,
                &principal,
                Action::Reveal,
                &path,
                Some(&body.key),
                Some(version),
                "error",
            );
            return openbao_error(failure);
        }
    };
    let Some(value) = secret.data.get(&body.key) else {
        audit_outcome(
            &state,
            &principal,
            Action::Reveal,
            &path,
            Some(&body.key),
            Some(version),
            "key_not_found",
        );
        return error(StatusCode::NOT_FOUND, "key_not_found");
    };
    state.audit.record(&Event {
        action: Action::Reveal,
        principal: &principal.id,
        path: Some(&path),
        key: Some(&body.key),
        version: Some(version),
        outcome: "ok",
        detail: None,
    });
    let mut response = Json(json!({
        "path": path,
        "version": version,
        "key": body.key,
        "value": value,
    }))
    .into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn update(
    State(state): State<GatewayState>,
    Path(raw): Path<String>,
    headers: HeaderMap,
    Json(body): Json<UpdateBody>,
) -> Response {
    let principal = match authorized(&state, &headers).await {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let path = match resolve(&raw) {
        Ok(path) => path,
        Err(response) => return response,
    };
    if let Err(response) = require_mutation(
        &state,
        &principal,
        &headers,
        Action::Update,
        &path,
        Some(&body.key),
        None,
    ) {
        return response;
    }
    if !valid_key(&body.key) {
        audit_outcome(
            &state,
            &principal,
            Action::Update,
            &path,
            Some(&body.key),
            Some(body.cas),
            "invalid_key",
        );
        return error(StatusCode::BAD_REQUEST, "invalid_key");
    }
    if body.value.len() > MAX_VALUE_BYTES {
        audit_outcome(
            &state,
            &principal,
            Action::Update,
            &path,
            Some(&body.key),
            Some(body.cas),
            "value_too_large",
        );
        return error(StatusCode::BAD_REQUEST, "value_too_large");
    }
    let meta = match state.openbao.metadata(&path).await {
        Ok(meta) => meta,
        Err(failure) => {
            audit_outcome(
                &state,
                &principal,
                Action::Update,
                &path,
                Some(&body.key),
                Some(body.cas),
                "error",
            );
            return openbao_error(failure);
        }
    };
    if meta.current_version != body.cas {
        audit_conflict(
            &state,
            &principal,
            Action::Update,
            &path,
            &body.key,
            meta.current_version,
        );
        return conflict(meta.current_version);
    }
    let mut data = match state.openbao.read(&path, Some(body.cas)).await {
        Ok(secret) => secret.data,
        Err(openbao::Error::NotFound) => BTreeMap::new(),
        Err(failure) => {
            audit_outcome(
                &state,
                &principal,
                Action::Update,
                &path,
                Some(&body.key),
                Some(body.cas),
                "error",
            );
            return openbao_error(failure);
        }
    };
    data.insert(body.key.clone(), body.value);
    let version = match state.openbao.write(&path, &data, body.cas).await {
        Ok(version) => version,
        Err(openbao::Error::Conflict) => {
            audit_conflict(
                &state,
                &principal,
                Action::Update,
                &path,
                &body.key,
                meta.current_version,
            );
            return conflict(meta.current_version);
        }
        Err(failure) => {
            audit_outcome(
                &state,
                &principal,
                Action::Update,
                &path,
                Some(&body.key),
                Some(body.cas),
                "error",
            );
            return openbao_error(failure);
        }
    };
    state.audit.record(&Event {
        action: Action::Update,
        principal: &principal.id,
        path: Some(&path),
        key: Some(&body.key),
        version: Some(version),
        outcome: "ok",
        detail: None,
    });
    Json(json!({"path": path, "version": version, "key": body.key})).into_response()
}

async fn rollback(
    State(state): State<GatewayState>,
    Path(raw): Path<String>,
    headers: HeaderMap,
    Json(body): Json<RollbackBody>,
) -> Response {
    let principal = match authorized(&state, &headers).await {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let path = match resolve(&raw) {
        Ok(path) => path,
        Err(response) => return response,
    };
    if let Err(response) = require_mutation(
        &state,
        &principal,
        &headers,
        Action::Rollback,
        &path,
        None,
        Some(body.version),
    ) {
        return response;
    }
    let meta = match state.openbao.metadata(&path).await {
        Ok(meta) => meta,
        Err(failure) => {
            audit_outcome(
                &state,
                &principal,
                Action::Rollback,
                &path,
                None,
                Some(body.version),
                "error",
            );
            return openbao_error(failure);
        }
    };
    let Some(source) = meta
        .versions
        .iter()
        .find(|item| item.version == body.version)
    else {
        audit_outcome(
            &state,
            &principal,
            Action::Rollback,
            &path,
            None,
            Some(body.version),
            "version_not_found",
        );
        return error(StatusCode::NOT_FOUND, "version_not_found");
    };
    if source.destroyed || !source.deletion_time.is_empty() {
        audit_outcome(
            &state,
            &principal,
            Action::Rollback,
            &path,
            None,
            Some(body.version),
            "version_unavailable",
        );
        return error(StatusCode::CONFLICT, "version_unavailable");
    }
    if meta.current_version != body.cas {
        audit_outcome(
            &state,
            &principal,
            Action::Rollback,
            &path,
            None,
            Some(body.version),
            "conflict",
        );
        return conflict(meta.current_version);
    }
    let secret = match state.openbao.read(&path, Some(body.version)).await {
        Ok(secret) => secret,
        Err(failure) => {
            audit_outcome(
                &state,
                &principal,
                Action::Rollback,
                &path,
                None,
                Some(body.version),
                "error",
            );
            return openbao_error(failure);
        }
    };
    let version = match state.openbao.write(&path, &secret.data, body.cas).await {
        Ok(version) => version,
        Err(openbao::Error::Conflict) => {
            audit_outcome(
                &state,
                &principal,
                Action::Rollback,
                &path,
                None,
                Some(body.version),
                "conflict",
            );
            return conflict(meta.current_version);
        }
        Err(failure) => {
            audit_outcome(
                &state,
                &principal,
                Action::Rollback,
                &path,
                None,
                Some(body.version),
                "error",
            );
            return openbao_error(failure);
        }
    };
    state.audit.record(&Event {
        action: Action::Rollback,
        principal: &principal.id,
        path: Some(&path),
        key: None,
        version: Some(version),
        outcome: "ok",
        detail: Some("rollback"),
    });
    Json(json!({
        "path": path,
        "fromVersion": body.version,
        "version": version,
    }))
    .into_response()
}

async fn delete_latest(
    State(state): State<GatewayState>,
    Path(raw): Path<String>,
    headers: HeaderMap,
) -> Response {
    let principal = match authorized(&state, &headers).await {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let path = match resolve(&raw) {
        Ok(path) => path,
        Err(response) => return response,
    };
    if let Err(response) = require_mutation(
        &state,
        &principal,
        &headers,
        Action::Delete,
        &path,
        None,
        None,
    ) {
        return response;
    }
    match state.openbao.delete_latest(&path).await {
        Ok(()) => {
            state.audit.record(&Event {
                action: Action::Delete,
                principal: &principal.id,
                path: Some(&path),
                key: None,
                version: None,
                outcome: "ok",
                detail: None,
            });
            Json(json!({"path": path, "deleted": true})).into_response()
        }
        Err(failure) => {
            audit_outcome(
                &state,
                &principal,
                Action::Delete,
                &path,
                None,
                None,
                "error",
            );
            openbao_error(failure)
        }
    }
}

async fn step_up(
    State(state): State<GatewayState>,
    headers: HeaderMap,
    Json(body): Json<StepUpBody>,
) -> Response {
    let principal = match authorized(&state, &headers).await {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    match state
        .stepup
        .start_session(&principal.id, &body.totp, totp::now_unix(), Instant::now())
    {
        Ok((session, expires_in)) => {
            state.audit.record(&Event {
                action: Action::StepUp,
                principal: &principal.id,
                path: None,
                key: None,
                version: None,
                outcome: "ok",
                detail: None,
            });
            Json(json!({"session": session, "expiresIn": expires_in})).into_response()
        }
        Err(Failure::RateLimited { retry_after }) => {
            state.audit.record(&Event {
                action: Action::StepUp,
                principal: &principal.id,
                path: None,
                key: None,
                version: None,
                outcome: "rate_limited",
                detail: None,
            });
            rate_limited(retry_after)
        }
        Err(_) => {
            state.audit.record(&Event {
                action: Action::StepUp,
                principal: &principal.id,
                path: None,
                key: None,
                version: None,
                outcome: "step_up_invalid",
                detail: None,
            });
            error(StatusCode::FORBIDDEN, "step_up_invalid")
        }
    }
}

#[allow(clippy::result_large_err)]
fn require_mutation(
    state: &GatewayState,
    principal: &Principal,
    headers: &HeaderMap,
    action: Action,
    path: &str,
    key: Option<&str>,
    version: Option<u64>,
) -> Result<(), Response> {
    let code = header_value(headers, STEP_UP_TOTP_HEADER);
    if code.is_empty() {
        state.audit.record(&Event {
            action,
            principal: &principal.id,
            path: Some(path),
            key,
            version,
            outcome: "step_up_required",
            detail: None,
        });
        return Err(error(StatusCode::FORBIDDEN, "step_up_required"));
    }
    match state
        .stepup
        .verify_mutation(&principal.id, code, totp::now_unix(), Instant::now())
    {
        Ok(_) => Ok(()),
        Err(Failure::RateLimited { retry_after }) => {
            state.audit.record(&Event {
                action,
                principal: &principal.id,
                path: Some(path),
                key,
                version,
                outcome: "rate_limited",
                detail: None,
            });
            Err(rate_limited(retry_after))
        }
        Err(_) => {
            state.audit.record(&Event {
                action,
                principal: &principal.id,
                path: Some(path),
                key,
                version,
                outcome: "step_up_invalid",
                detail: None,
            });
            Err(error(StatusCode::FORBIDDEN, "step_up_invalid"))
        }
    }
}

fn audit_conflict(
    state: &GatewayState,
    principal: &Principal,
    action: Action,
    path: &str,
    key: &str,
    version: u64,
) {
    audit_outcome(
        state,
        principal,
        action,
        path,
        Some(key),
        Some(version),
        "conflict",
    );
}

fn audit_outcome(
    state: &GatewayState,
    principal: &Principal,
    action: Action,
    path: &str,
    key: Option<&str>,
    version: Option<u64>,
    outcome: &'static str,
) {
    state.audit.record(&Event {
        action,
        principal: &principal.id,
        path: Some(path),
        key,
        version,
        outcome,
        detail: None,
    });
}

#[allow(clippy::result_large_err)]
fn resolve(raw: &str) -> Result<String, Response> {
    let path = path::normalize(raw).map_err(|_| error(StatusCode::BAD_REQUEST, "invalid_path"))?;
    if deny::is_denied(&path) {
        return Err(error(StatusCode::FORBIDDEN, "path_forbidden"));
    }
    Ok(path)
}

fn header_value<'a>(headers: &'a HeaderMap, name: &str) -> &'a str {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
}

fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key.chars().count() <= MAX_KEY_CHARS
        && key.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | '+' | '@')
        })
}

fn forbidden_response(forbidden: Forbidden) -> Response {
    let mut response = (StatusCode::FORBIDDEN, forbidden.body()).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response
}

fn error(status: StatusCode, code: &'static str) -> Response {
    (status, Json(json!({"error": code}))).into_response()
}

fn conflict(current_version: u64) -> Response {
    (
        StatusCode::CONFLICT,
        Json(json!({"error": "conflict", "currentVersion": current_version})),
    )
        .into_response()
}

fn rate_limited(retry_after: u64) -> Response {
    let mut response = (
        StatusCode::TOO_MANY_REQUESTS,
        Json(json!({"error": "rate_limited", "retryAfter": retry_after})),
    )
        .into_response();
    let value = HeaderValue::from_str(&retry_after.to_string())
        .unwrap_or_else(|_| HeaderValue::from_static("60"));
    response.headers_mut().insert(header::RETRY_AFTER, value);
    response
}

fn openbao_error(failure: openbao::Error) -> Response {
    match failure {
        openbao::Error::NotFound => error(StatusCode::NOT_FOUND, "not_found"),
        openbao::Error::Conflict => error(StatusCode::CONFLICT, "conflict"),
        openbao::Error::Forbidden => error(StatusCode::BAD_GATEWAY, "openbao_forbidden"),
        openbao::Error::Invalid => error(StatusCode::BAD_REQUEST, "openbao_rejected"),
        openbao::Error::Unavailable => error(StatusCode::BAD_GATEWAY, "openbao_unavailable"),
        openbao::Error::Parse => error(StatusCode::BAD_GATEWAY, "openbao_invalid_response"),
    }
}

#[must_use]
pub fn route_summary() -> Value {
    json!({
        "list": "GET /v1/secrets",
        "metadata": "GET /v1/secrets/meta/{path}",
        "reveal": "POST /v1/secrets/reveal/{path}",
        "update": "POST /v1/secrets/update/{path}",
        "rollback": "POST /v1/secrets/rollback/{path}",
        "delete": "DELETE /v1/secrets/{path}",
        "stepUp": "POST /v1/step-up",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_bounded_and_ascii_safe() {
        assert!(valid_key("service_token"));
        assert!(valid_key("vpn.json"));
        assert!(!valid_key(""));
        assert!(!valid_key("with space"));
        assert!(!valid_key("with/slash"));
        assert!(!valid_key(&"a".repeat(MAX_KEY_CHARS + 1)));
    }

    #[test]
    fn route_summary_lists_no_destroy_endpoint() {
        let summary = route_summary().to_string();
        assert!(summary.contains("reveal"));
        assert!(!summary.contains("destroy"));
    }
}
