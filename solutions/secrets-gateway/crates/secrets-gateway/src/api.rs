use std::collections::BTreeMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
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
use crate::deny::Deny;
use crate::device::{self, DeviceStore};
use crate::openbao::{self, OpenBao};
use crate::path;
use crate::stepup::{Failure, Operation, Purpose, StepUp, now_unix};

pub const STEP_UP_SESSION_HEADER: &str = "x-step-up-session";
pub const STEP_UP_CHALLENGE_HEADER: &str = "x-step-up-challenge";
pub const STEP_UP_SIGNATURE_HEADER: &str = "x-step-up-signature";
pub const MAX_KEY_CHARS: usize = 128;
pub const MAX_VALUE_BYTES: usize = 32 * 1024;

#[derive(Clone)]
pub struct GatewayState {
    pub access: Arc<AccessClient>,
    pub openbao: Arc<OpenBao>,
    pub catalog: Arc<Catalog>,
    pub deny: Arc<Deny>,
    pub device: Arc<DeviceStore>,
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
    challenge: String,
    signature: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ChallengeBody {
    action: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    version: Option<u64>,
    #[serde(default)]
    cas: Option<u64>,
    #[serde(default)]
    value_hash: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EnrollBody {
    code: String,
    public_key: String,
    attestation_challenge: String,
    #[serde(default)]
    attestation_chain: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeleteQuery {
    cas: u64,
    #[serde(default)]
    version: Option<u64>,
}

pub fn router(state: GatewayState) -> Router {
    Router::new()
        .route("/v1/secrets", get(list))
        .route("/v1/secrets/meta/{*path}", get(metadata))
        .route("/v1/secrets/reveal/{*path}", post(reveal))
        .route("/v1/secrets/update/{*path}", post(update))
        .route("/v1/secrets/rollback/{*path}", post(rollback))
        .route("/v1/secrets/{*path}", axum::routing::delete(delete_latest))
        .route("/v1/step-up/challenge", post(step_up_challenge))
        .route("/v1/step-up", post(step_up))
        .route("/v1/device", get(device_status))
        .route("/v1/device/challenge", post(device_challenge))
        .route("/v1/device/enroll", post(device_enroll))
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
        Ok(()) => {
            if let Ok(Some(device_id)) = state.device.poll_revocation(Instant::now()).await {
                state.audit.record(&Event {
                    action: Action::DeviceRevoke,
                    principal: &principal.id,
                    path: None,
                    key: None,
                    version: None,
                    outcome: "ok",
                    detail: Some(&device_id),
                });
            }
            Ok(principal)
        }
        Err(forbidden) => Err(forbidden_response(forbidden)),
    }
}

async fn list(State(state): State<GatewayState>, headers: HeaderMap) -> Response {
    let principal = match authorized(&state, &headers).await {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let listing = match state
        .openbao
        .list_all(&state.deny, state.max_list_entries)
        .await
    {
        Ok(listing) => listing,
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
        if state.deny.is_denied(path) || path.as_str() == state.device.path() {
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
    let openbao::ListResult { paths, truncated } = listing;
    for path in paths {
        if state.deny.is_denied(&path) || path.as_str() == state.device.path() {
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
    Json(json!({
        "entries": entries.into_values().collect::<Vec<_>>(),
        "truncated": truncated,
    }))
    .into_response()
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
    let path = match resolve(&state, &principal, Action::Metadata, &raw, None, None) {
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
    let path = match resolve(
        &state,
        &principal,
        Action::Reveal,
        &raw,
        Some(audit_key(&body.key)),
        body.version,
    ) {
        Ok(path) => path,
        Err(response) => return response,
    };
    let session = header_value(&headers, STEP_UP_SESSION_HEADER);
    if session.is_empty()
        || !state
            .stepup
            .verify_session(&principal.id, session, Instant::now())
            .await
    {
        state.audit.record(&Event {
            action: Action::Reveal,
            principal: &principal.id,
            path: Some(&path),
            key: Some(audit_key(&body.key)),
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
            Some(audit_key(&body.key)),
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
            Some(audit_key(&body.key)),
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
                Some(audit_key(&body.key)),
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
            Some(audit_key(&body.key)),
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
            Some(audit_key(&body.key)),
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
                Some(audit_key(&body.key)),
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
            Some(audit_key(&body.key)),
            Some(version),
            "key_not_found",
        );
        return error(StatusCode::NOT_FOUND, "key_not_found");
    };
    state.audit.record(&Event {
        action: Action::Reveal,
        principal: &principal.id,
        path: Some(&path),
        key: Some(audit_key(&body.key)),
        version: Some(version),
        outcome: "ok",
        detail: None,
    });
    no_store(
        Json(json!({
            "path": path,
            "version": version,
            "key": body.key,
            "value": value,
        }))
        .into_response(),
    )
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
    let path = match resolve(
        &state,
        &principal,
        Action::Update,
        &raw,
        Some(audit_key(&body.key)),
        Some(body.cas),
    ) {
        Ok(path) => path,
        Err(response) => return response,
    };
    if let Err(response) = require_mutation(
        &state,
        &principal,
        &headers,
        Action::Update,
        Operation {
            purpose: Purpose::Update,
            path: Some(path.clone()),
            key: Some(body.key.clone()),
            version: None,
            cas: Some(body.cas),
            value_hash: Some(sha256_hex(&body.value)),
        },
        Some(audit_key(&body.key)),
        None,
    )
    .await
    {
        return response;
    }
    if !valid_key(&body.key) {
        audit_outcome(
            &state,
            &principal,
            Action::Update,
            &path,
            Some(audit_key(&body.key)),
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
            Some(audit_key(&body.key)),
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
                Some(audit_key(&body.key)),
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
            audit_key(&body.key),
            meta.current_version,
        );
        return conflict(meta.current_version);
    }
    let current = meta
        .versions
        .iter()
        .find(|item| item.version == meta.current_version);
    if current.is_none_or(|item| item.destroyed || !item.deletion_time.is_empty()) {
        audit_outcome(
            &state,
            &principal,
            Action::Update,
            &path,
            Some(audit_key(&body.key)),
            Some(body.cas),
            "version_unavailable",
        );
        return error(StatusCode::CONFLICT, "version_unavailable");
    }
    let mut data = match state.openbao.read(&path, Some(body.cas)).await {
        Ok(secret) => secret.data,
        Err(openbao::Error::NotFound) => {
            audit_outcome(
                &state,
                &principal,
                Action::Update,
                &path,
                Some(audit_key(&body.key)),
                Some(body.cas),
                "version_unavailable",
            );
            return error(StatusCode::CONFLICT, "version_unavailable");
        }
        Err(failure) => {
            audit_outcome(
                &state,
                &principal,
                Action::Update,
                &path,
                Some(audit_key(&body.key)),
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
                audit_key(&body.key),
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
                Some(audit_key(&body.key)),
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
        key: Some(audit_key(&body.key)),
        version: Some(version),
        outcome: "ok",
        detail: None,
    });
    no_store(Json(json!({"path": path, "version": version, "key": body.key})).into_response())
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
    let path = match resolve(
        &state,
        &principal,
        Action::Rollback,
        &raw,
        None,
        Some(body.version),
    ) {
        Ok(path) => path,
        Err(response) => return response,
    };
    if let Err(response) = require_mutation(
        &state,
        &principal,
        &headers,
        Action::Rollback,
        Operation {
            purpose: Purpose::Rollback,
            path: Some(path.clone()),
            key: None,
            version: Some(body.version),
            cas: Some(body.cas),
            value_hash: None,
        },
        None,
        Some(body.version),
    )
    .await
    {
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
    no_store(
        Json(json!({
            "path": path,
            "fromVersion": body.version,
            "version": version,
        }))
        .into_response(),
    )
}

async fn delete_latest(
    State(state): State<GatewayState>,
    Path(raw): Path<String>,
    Query(query): Query<DeleteQuery>,
    headers: HeaderMap,
) -> Response {
    let principal = match authorized(&state, &headers).await {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let path = match resolve(&state, &principal, Action::Delete, &raw, None, None) {
        Ok(path) => path,
        Err(response) => return response,
    };
    if let Err(response) = require_mutation(
        &state,
        &principal,
        &headers,
        Action::Delete,
        Operation {
            purpose: Purpose::Delete,
            path: Some(path.clone()),
            key: None,
            version: query.version,
            cas: Some(query.cas),
            value_hash: None,
        },
        None,
        Some(query.cas),
    )
    .await
    {
        return response;
    }
    let meta = match state.openbao.metadata(&path).await {
        Ok(meta) => meta,
        Err(failure) => {
            audit_outcome(
                &state,
                &principal,
                Action::Delete,
                &path,
                None,
                Some(query.cas),
                "error",
            );
            return openbao_error(failure);
        }
    };
    if meta.current_version != query.cas {
        audit_outcome(
            &state,
            &principal,
            Action::Delete,
            &path,
            None,
            Some(meta.current_version),
            "conflict",
        );
        return conflict(meta.current_version);
    }
    if query
        .version
        .is_some_and(|version| version != meta.current_version)
    {
        audit_outcome(
            &state,
            &principal,
            Action::Delete,
            &path,
            None,
            query.version,
            "version_not_current",
        );
        return error(StatusCode::CONFLICT, "version_not_current");
    }
    let fresh = match state.openbao.metadata(&path).await {
        Ok(meta) => meta,
        Err(failure) => {
            audit_outcome(
                &state,
                &principal,
                Action::Delete,
                &path,
                None,
                Some(query.cas),
                "error",
            );
            return openbao_error(failure);
        }
    };
    if fresh.current_version != meta.current_version {
        audit_outcome(
            &state,
            &principal,
            Action::Delete,
            &path,
            None,
            Some(fresh.current_version),
            "conflict",
        );
        return conflict(fresh.current_version);
    }
    match state.openbao.delete_latest(&path).await {
        Ok(()) => {
            state.audit.record(&Event {
                action: Action::Delete,
                principal: &principal.id,
                path: Some(&path),
                key: None,
                version: Some(meta.current_version),
                outcome: "ok",
                detail: None,
            });
            no_store(
                Json(json!({
                    "path": path,
                    "deleted": true,
                    "version": meta.current_version,
                }))
                .into_response(),
            )
        }
        Err(failure) => {
            audit_outcome(
                &state,
                &principal,
                Action::Delete,
                &path,
                None,
                Some(meta.current_version),
                "error",
            );
            openbao_error(failure)
        }
    }
}

async fn step_up_challenge(
    State(state): State<GatewayState>,
    headers: HeaderMap,
    Json(body): Json<ChallengeBody>,
) -> Response {
    let principal = match authorized(&state, &headers).await {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let Some(purpose) = Purpose::parse(&body.action) else {
        return error(StatusCode::BAD_REQUEST, "invalid_action");
    };
    let operation = match operation_from(&state, &principal, purpose, &body) {
        Ok(operation) => operation,
        Err(response) => return response,
    };
    match state
        .stepup
        .issue(&principal.id, operation, now_unix(), Instant::now())
        .await
    {
        Ok(view) => no_store(
            Json(json!({
                "challenge": view.challenge,
                "payload": view.payload,
                "expiresIn": view.expires_in,
            }))
            .into_response(),
        ),
        Err(failure) => {
            let (outcome, response) = failure_response(failure);
            audit_event(
                &state,
                &principal,
                Action::StepUp,
                None,
                None,
                None,
                outcome,
            );
            response
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
        .open_session(
            &principal.id,
            &body.challenge,
            &body.signature,
            now_unix(),
            Instant::now(),
        )
        .await
    {
        Ok((session, expires_in)) => {
            audit_event(&state, &principal, Action::StepUp, None, None, None, "ok");
            no_store(Json(json!({"session": session, "expiresIn": expires_in})).into_response())
        }
        Err(failure) => {
            let (outcome, response) = failure_response(failure);
            audit_event(
                &state,
                &principal,
                Action::StepUp,
                None,
                None,
                None,
                outcome,
            );
            response
        }
    }
}

async fn device_status(State(state): State<GatewayState>, headers: HeaderMap) -> Response {
    if let Err(response) = authorized(&state, &headers).await {
        return response;
    }
    match state.device.status().await {
        Ok(Some(device)) => no_store(
            Json(json!({
                "enrolled": true,
                "deviceId": device.device_id,
                "enrolledAt": device.enrolled_at,
            }))
            .into_response(),
        ),
        Ok(None) => no_store(Json(json!({"enrolled": false})).into_response()),
        Err(_) => error(StatusCode::SERVICE_UNAVAILABLE, "device_unavailable"),
    }
}

async fn device_challenge(State(state): State<GatewayState>, headers: HeaderMap) -> Response {
    let principal = match authorized(&state, &headers).await {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    match state
        .device
        .attestation_challenge(&principal.id, now_unix())
    {
        Ok((challenge, expires_in)) => {
            no_store(Json(json!({"challenge": challenge, "expiresIn": expires_in})).into_response())
        }
        Err(_) => error(StatusCode::SERVICE_UNAVAILABLE, "device_unavailable"),
    }
}

async fn device_enroll(
    State(state): State<GatewayState>,
    headers: HeaderMap,
    Json(body): Json<EnrollBody>,
) -> Response {
    let principal = match authorized(&state, &headers).await {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let enrollment = device::Enrollment {
        code: &body.code,
        public_key: &body.public_key,
        challenge: &body.attestation_challenge,
        attestation_chain: &body.attestation_chain,
    };
    match state
        .device
        .enroll(&principal.id, enrollment, now_unix(), Instant::now())
        .await
    {
        Ok(device) => {
            audit_event(&state, &principal, Action::Enroll, None, None, None, "ok");
            no_store(
                Json(json!({
                    "enrolled": true,
                    "deviceId": device.device_id,
                    "enrolledAt": device.enrolled_at,
                }))
                .into_response(),
            )
        }
        Err(failure) => {
            let (outcome, detail, response) = device_failure(failure);
            audit_event_with_detail(
                &state,
                &principal,
                Action::Enroll,
                None,
                None,
                None,
                outcome,
                detail,
            );
            response
        }
    }
}

#[allow(clippy::result_large_err)]
fn operation_from(
    state: &GatewayState,
    principal: &Principal,
    purpose: Purpose,
    body: &ChallengeBody,
) -> Result<Operation, Response> {
    match purpose {
        Purpose::Session => {
            if body.path.is_some()
                || body.key.is_some()
                || body.version.is_some()
                || body.cas.is_some()
            {
                return Err(error(StatusCode::BAD_REQUEST, "invalid_challenge"));
            }
            Ok(Operation::session())
        }
        Purpose::Update => {
            let Some(raw) = body.path.as_deref() else {
                return Err(error(StatusCode::BAD_REQUEST, "invalid_challenge"));
            };
            let Some(key) = body.key.as_deref() else {
                return Err(error(StatusCode::BAD_REQUEST, "invalid_challenge"));
            };
            let Some(cas) = body.cas else {
                return Err(error(StatusCode::BAD_REQUEST, "invalid_challenge"));
            };
            if body.version.is_some() {
                return Err(error(StatusCode::BAD_REQUEST, "invalid_challenge"));
            }
            let Some(value_hash) = body.value_hash.as_deref().filter(|hash| valid_hash(hash))
            else {
                return Err(error(StatusCode::BAD_REQUEST, "invalid_challenge"));
            };
            let path = resolve(
                state,
                principal,
                Action::Update,
                raw,
                Some(audit_key(key)),
                None,
            )?;
            Ok(Operation {
                purpose,
                path: Some(path),
                key: Some(key.to_owned()),
                version: None,
                cas: Some(cas),
                value_hash: Some(value_hash.to_ascii_lowercase()),
            })
        }
        Purpose::Rollback => {
            let Some(raw) = body.path.as_deref() else {
                return Err(error(StatusCode::BAD_REQUEST, "invalid_challenge"));
            };
            let Some(version) = body.version else {
                return Err(error(StatusCode::BAD_REQUEST, "invalid_challenge"));
            };
            let Some(cas) = body.cas else {
                return Err(error(StatusCode::BAD_REQUEST, "invalid_challenge"));
            };
            if body.key.is_some() {
                return Err(error(StatusCode::BAD_REQUEST, "invalid_challenge"));
            }
            let path = resolve(state, principal, Action::Rollback, raw, None, Some(version))?;
            Ok(Operation {
                purpose,
                path: Some(path),
                key: None,
                version: Some(version),
                cas: Some(cas),
                value_hash: None,
            })
        }
        Purpose::Delete => {
            let Some(raw) = body.path.as_deref() else {
                return Err(error(StatusCode::BAD_REQUEST, "invalid_challenge"));
            };
            let Some(cas) = body.cas else {
                return Err(error(StatusCode::BAD_REQUEST, "invalid_challenge"));
            };
            if body.key.is_some() || body.value_hash.is_some() {
                return Err(error(StatusCode::BAD_REQUEST, "invalid_challenge"));
            }
            let path = resolve(state, principal, Action::Delete, raw, None, body.version)?;
            Ok(Operation {
                purpose,
                path: Some(path),
                key: None,
                version: body.version,
                cas: Some(cas),
                value_hash: None,
            })
        }
    }
}

#[allow(clippy::result_large_err)]
async fn require_mutation(
    state: &GatewayState,
    principal: &Principal,
    headers: &HeaderMap,
    action: Action,
    operation: Operation,
    key: Option<&str>,
    version: Option<u64>,
) -> Result<(), Response> {
    let path = operation.path.as_deref();
    let challenge = header_value(headers, STEP_UP_CHALLENGE_HEADER);
    let signature = header_value(headers, STEP_UP_SIGNATURE_HEADER);
    if challenge.is_empty() || signature.is_empty() {
        audit_event(
            state,
            principal,
            action,
            path,
            key,
            version,
            "step_up_required",
        );
        return Err(error(StatusCode::FORBIDDEN, "step_up_required"));
    }
    match state
        .stepup
        .authorize_mutation(
            &principal.id,
            &operation,
            challenge,
            signature,
            now_unix(),
            Instant::now(),
        )
        .await
    {
        Ok(()) => Ok(()),
        Err(failure) => {
            let (outcome, response) = failure_response(failure);
            audit_event(state, principal, action, path, key, version, outcome);
            Err(response)
        }
    }
}

fn failure_response(failure: Failure) -> (&'static str, Response) {
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

fn device_failure(failure: device::Error) -> (&'static str, Option<&'static str>, Response) {
    match failure {
        device::Error::EnrollmentClosed => (
            "enrollment_invalid",
            Some("enrollment_closed"),
            error(StatusCode::FORBIDDEN, "enrollment_invalid"),
        ),
        device::Error::CodeExpired => (
            "enrollment_invalid",
            Some("enrollment_expired"),
            error(StatusCode::FORBIDDEN, "enrollment_invalid"),
        ),
        device::Error::CodeInvalid => (
            "enrollment_invalid",
            Some("enrollment_invalid"),
            error(StatusCode::FORBIDDEN, "enrollment_invalid"),
        ),
        device::Error::ChallengeInvalid => (
            "enrollment_invalid",
            Some("attestation_challenge_invalid"),
            error(StatusCode::FORBIDDEN, "enrollment_invalid"),
        ),
        device::Error::KeyInvalid => (
            "enrollment_invalid",
            Some("public_key_invalid"),
            error(StatusCode::FORBIDDEN, "enrollment_invalid"),
        ),
        device::Error::AttestationInvalid => (
            "enrollment_invalid",
            Some("attestation_invalid"),
            error(StatusCode::FORBIDDEN, "enrollment_invalid"),
        ),
        device::Error::Conflict => (
            "enrollment_conflict",
            None,
            error(StatusCode::CONFLICT, "enrollment_conflict"),
        ),
        device::Error::RateLimited { retry_after } => {
            ("rate_limited", None, rate_limited(retry_after))
        }
        device::Error::NotEnrolled => (
            "device_not_enrolled",
            None,
            error(StatusCode::FORBIDDEN, "device_not_enrolled"),
        ),
        device::Error::Unavailable => (
            "device_unavailable",
            None,
            error(StatusCode::SERVICE_UNAVAILABLE, "device_unavailable"),
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn audit_event_with_detail(
    state: &GatewayState,
    principal: &Principal,
    action: Action,
    path: Option<&str>,
    key: Option<&str>,
    version: Option<u64>,
    outcome: &'static str,
    detail: Option<&'static str>,
) {
    state.audit.record(&Event {
        action,
        principal: &principal.id,
        path,
        key,
        version,
        outcome,
        detail,
    });
}

fn audit_event(
    state: &GatewayState,
    principal: &Principal,
    action: Action,
    path: Option<&str>,
    key: Option<&str>,
    version: Option<u64>,
    outcome: &'static str,
) {
    state.audit.record(&Event {
        action,
        principal: &principal.id,
        path,
        key,
        version,
        outcome,
        detail: None,
    });
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
fn resolve(
    state: &GatewayState,
    principal: &Principal,
    action: Action,
    raw: &str,
    key: Option<&str>,
    version: Option<u64>,
) -> Result<String, Response> {
    let Ok(path) = path::normalize(raw) else {
        audit_outcome(
            state,
            principal,
            action,
            "<invalid>",
            key,
            version,
            "invalid_path",
        );
        return Err(error(StatusCode::BAD_REQUEST, "invalid_path"));
    };
    if state.deny.is_denied(&path) || path.as_str() == state.device.path() {
        audit_outcome(
            state,
            principal,
            action,
            &path,
            key,
            version,
            "path_forbidden",
        );
        return Err(error(StatusCode::FORBIDDEN, "path_forbidden"));
    }
    Ok(path)
}

fn audit_key(key: &str) -> &str {
    if valid_key(key) { key } else { "<invalid>" }
}

fn header_value<'a>(headers: &'a HeaderMap, name: &str) -> &'a str {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn sha256_hex(value: &str) -> String {
    use sha2::Digest as _;
    hex::encode(sha2::Sha256::digest(value.as_bytes()))
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

fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
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
        "device": "GET /v1/device",
        "deviceChallenge": "POST /v1/device/challenge",
        "deviceEnroll": "POST /v1/device/enroll",
        "stepUpChallenge": "POST /v1/step-up/challenge",
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
