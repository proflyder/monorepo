use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use serde_json::json;
use tokio::time::Instant;

use super::audit_scope::AuditScope;
use super::guard::{authorized, require_mutation, resolve};
use super::reply::{Rejection, Reply, conflict, error, no_store};
use super::request::{DeleteQuery, RevealBody, RollbackBody, UpdateBody};
use super::validate::{audit_key, header_value, sha256_hex, valid_key};
use super::{GatewayState, MAX_VALUE_BYTES, STEP_UP_SESSION_HEADER};
use crate::audit::Action;
use crate::openbao::{self, Metadata};
use crate::stepup::{Operation, Purpose};

pub(super) async fn reveal(
    State(state): State<GatewayState>,
    Path(raw): Path<String>,
    headers: HeaderMap,
    Json(body): Json<RevealBody>,
) -> Reply {
    let principal = authorized(&state, &headers).await?;
    let base =
        AuditScope::new(&state, &principal, Action::Reveal).with_key(Some(audit_key(&body.key)));
    let path = resolve(&state, base, &raw, body.version)?;
    let scope = base.at(&path);
    let session = header_value(&headers, STEP_UP_SESSION_HEADER);
    if session.is_empty()
        || !state
            .stepup
            .verify_session(&principal.id, session, Instant::now())
            .await
    {
        return Err(scope.reject(
            body.version,
            "step_up_required",
            error(StatusCode::FORBIDDEN, "step_up_required"),
        ));
    }
    if !valid_key(&body.key) {
        return Err(scope.reject(body.version, "invalid_key", invalid_key()));
    }
    let Some(version) = body.version else {
        return Err(scope.reject(
            None,
            "version_required",
            error(StatusCode::BAD_REQUEST, "version_required"),
        ));
    };
    let at = Some(version);
    let meta = state
        .openbao
        .metadata(&path)
        .await
        .map_err(|failure| scope.openbao(at, failure))?;
    let Some(info) = meta.version(version) else {
        return Err(scope.reject(
            at,
            "version_not_found",
            error(StatusCode::NOT_FOUND, "version_not_found"),
        ));
    };
    if !info.is_available() {
        return Err(scope.reject(at, "version_unavailable", version_unavailable()));
    }
    let secret = state
        .openbao
        .read(&path, at)
        .await
        .map_err(|failure| scope.openbao(at, failure))?;
    let Some(value) = secret.data.get(&body.key) else {
        return Err(scope.reject(
            at,
            "key_not_found",
            error(StatusCode::NOT_FOUND, "key_not_found"),
        ));
    };
    scope.record(at, "ok");
    Ok(no_store(json!({
        "path": path,
        "version": version,
        "key": body.key,
        "value": value,
    })))
}

pub(super) async fn update(
    State(state): State<GatewayState>,
    Path(raw): Path<String>,
    headers: HeaderMap,
    Json(body): Json<UpdateBody>,
) -> Reply {
    let principal = authorized(&state, &headers).await?;
    let base =
        AuditScope::new(&state, &principal, Action::Update).with_key(Some(audit_key(&body.key)));
    let cas = Some(body.cas);
    let path = resolve(&state, base, &raw, cas)?;
    let scope = base.at(&path);
    let operation = Operation {
        purpose: Purpose::Update,
        path: Some(path.clone()),
        key: Some(body.key.clone()),
        version: None,
        cas,
        value_hash: Some(sha256_hex(&body.value)),
    };
    require_mutation(&state, scope, &headers, &operation, None).await?;
    if !valid_key(&body.key) {
        return Err(scope.reject(cas, "invalid_key", invalid_key()));
    }
    if body.value.len() > MAX_VALUE_BYTES {
        return Err(scope.reject(
            cas,
            "value_too_large",
            error(StatusCode::BAD_REQUEST, "value_too_large"),
        ));
    }
    let meta = state
        .openbao
        .metadata(&path)
        .await
        .map_err(|failure| scope.openbao(cas, failure))?;
    let current = meta.current_version;
    if current != body.cas {
        return Err(scope.reject(Some(current), "conflict", conflict(current)));
    }
    if !meta.is_available(current) {
        return Err(scope.reject(cas, "version_unavailable", version_unavailable()));
    }
    let mut data = match state.openbao.read(&path, cas).await {
        Ok(secret) => secret.data,
        Err(openbao::Error::NotFound) => {
            return Err(scope.reject(cas, "version_unavailable", version_unavailable()));
        }
        Err(failure) => return Err(scope.openbao(cas, failure)),
    };
    data.insert(body.key.clone(), body.value);
    let version = match state.openbao.write(&path, &data, body.cas).await {
        Ok(version) => version,
        Err(openbao::Error::Conflict) => {
            return Err(scope.reject(Some(current), "conflict", conflict(current)));
        }
        Err(failure) => return Err(scope.openbao(cas, failure)),
    };
    scope.record(Some(version), "ok");
    Ok(no_store(
        json!({"path": path, "version": version, "key": body.key}),
    ))
}

pub(super) async fn rollback(
    State(state): State<GatewayState>,
    Path(raw): Path<String>,
    headers: HeaderMap,
    Json(body): Json<RollbackBody>,
) -> Reply {
    let principal = authorized(&state, &headers).await?;
    let base = AuditScope::new(&state, &principal, Action::Rollback);
    let source_version = Some(body.version);
    let path = resolve(&state, base, &raw, source_version)?;
    let scope = base.at(&path);
    let operation = Operation {
        purpose: Purpose::Rollback,
        path: Some(path.clone()),
        key: None,
        version: source_version,
        cas: Some(body.cas),
        value_hash: None,
    };
    require_mutation(&state, scope, &headers, &operation, source_version).await?;
    let fail = |failure| scope.openbao(source_version, failure);
    let meta = state.openbao.metadata(&path).await.map_err(fail)?;
    let Some(source) = meta.version(body.version) else {
        return Err(scope.reject(
            source_version,
            "version_not_found",
            error(StatusCode::NOT_FOUND, "version_not_found"),
        ));
    };
    if !source.is_available() {
        return Err(scope.reject(source_version, "version_unavailable", version_unavailable()));
    }
    let stale = || scope.reject(source_version, "conflict", conflict(meta.current_version));
    if meta.current_version != body.cas {
        return Err(stale());
    }
    let secret = state
        .openbao
        .read(&path, source_version)
        .await
        .map_err(fail)?;
    let version = match state.openbao.write(&path, &secret.data, body.cas).await {
        Ok(version) => version,
        Err(openbao::Error::Conflict) => return Err(stale()),
        Err(failure) => return Err(fail(failure)),
    };
    scope.record_detail(Some(version), "ok", Some("rollback"));
    Ok(no_store(json!({
        "path": path,
        "fromVersion": body.version,
        "version": version,
    })))
}

pub(super) async fn delete_latest(
    State(state): State<GatewayState>,
    Path(raw): Path<String>,
    Query(query): Query<DeleteQuery>,
    headers: HeaderMap,
) -> Reply {
    let principal = authorized(&state, &headers).await?;
    let base = AuditScope::new(&state, &principal, Action::Delete);
    let path = resolve(&state, base, &raw, None)?;
    let scope = base.at(&path);
    let cas = Some(query.cas);
    let operation = Operation {
        purpose: Purpose::Delete,
        path: Some(path.clone()),
        key: None,
        version: query.version,
        cas,
        value_hash: None,
    };
    require_mutation(&state, scope, &headers, &operation, cas).await?;
    let meta = current_metadata(&state, scope, &path, cas).await?;
    let current = meta.current_version;
    if current != query.cas {
        return Err(scope.reject(Some(current), "conflict", conflict(current)));
    }
    if query.version.is_some_and(|version| version != current) {
        return Err(scope.reject(
            query.version,
            "version_not_current",
            error(StatusCode::CONFLICT, "version_not_current"),
        ));
    }
    let fresh = current_metadata(&state, scope, &path, cas)
        .await?
        .current_version;
    if fresh != current {
        return Err(scope.reject(Some(fresh), "conflict", conflict(fresh)));
    }
    state
        .openbao
        .delete_latest(&path)
        .await
        .map_err(|failure| scope.openbao(Some(current), failure))?;
    scope.record(Some(current), "ok");
    Ok(no_store(json!({
        "path": path,
        "deleted": true,
        "version": current,
    })))
}

async fn current_metadata(
    state: &GatewayState,
    scope: AuditScope<'_>,
    path: &str,
    version: Option<u64>,
) -> Result<Metadata, Rejection> {
    state
        .openbao
        .metadata(path)
        .await
        .map_err(|failure| scope.openbao(version, failure))
}

fn invalid_key() -> Response {
    error(StatusCode::BAD_REQUEST, "invalid_key")
}

fn version_unavailable() -> Response {
    error(StatusCode::CONFLICT, "version_unavailable")
}
