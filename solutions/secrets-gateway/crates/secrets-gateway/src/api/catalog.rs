use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use serde_json::json;

use super::GatewayState;
use super::audit_scope::AuditScope;
use super::guard::{authorized, resolve};
use super::reply::{Rejection, Reply, openbao_error};
use super::request::EntryView;
use crate::audit::Action;
use crate::openbao::{self, Metadata};
use crate::path;

pub(super) async fn list(State(state): State<GatewayState>, headers: HeaderMap) -> Reply {
    let principal = authorized(&state, &headers).await?;
    let scope = AuditScope::new(&state, &principal, Action::List);
    let openbao::ListResult { paths, truncated } = state
        .openbao
        .list_all(&state.deny, state.max_list_entries)
        .await
        .map_err(|failure| {
            scope.record_detail(None, "error", Some("openbao"));
            Rejection::from(openbao_error(failure))
        })?;
    let mut entries = catalog_entries(&state);
    for path in paths.into_iter().filter(|path| is_listable(&state, path)) {
        entries
            .entry(path.clone())
            .or_insert_with(|| EntryView {
                path,
                description: String::new(),
                consumer: None,
                live: false,
                in_catalog: false,
            })
            .live = true;
    }
    scope.record(None, "ok");
    Ok(Json(json!({
        "entries": entries.into_values().collect::<Vec<_>>(),
        "truncated": truncated,
    }))
    .into_response())
}

fn catalog_entries(state: &GatewayState) -> BTreeMap<String, EntryView> {
    state
        .catalog
        .iter()
        .filter(|(path, _)| is_listable(state, path))
        .map(|(path, entry)| {
            (
                path.to_owned(),
                EntryView {
                    path: path.to_owned(),
                    description: entry.purpose.clone(),
                    consumer: entry.consumer.clone(),
                    live: false,
                    in_catalog: true,
                },
            )
        })
        .collect()
}

fn is_listable(state: &GatewayState, path: &str) -> bool {
    !state.is_hidden(path) && path::normalize(path).is_ok()
}

pub(super) async fn metadata(
    State(state): State<GatewayState>,
    Path(raw): Path<String>,
    headers: HeaderMap,
) -> Reply {
    let principal = authorized(&state, &headers).await?;
    let base = AuditScope::new(&state, &principal, Action::Metadata);
    let path = resolve(&state, base, &raw, None)?;
    let scope = base.at(&path);
    let meta = state
        .openbao
        .metadata(&path)
        .await
        .map_err(|failure| scope.openbao(None, failure))?;
    let keys = current_keys(&state, scope, &path, &meta).await?;
    scope.record(Some(meta.current_version), "ok");
    Ok(Json(json!({
        "path": path,
        "currentVersion": meta.current_version,
        "oldestVersion": meta.oldest_version,
        "versions": meta.versions,
        "keys": keys,
    }))
    .into_response())
}

async fn current_keys(
    state: &GatewayState,
    scope: AuditScope<'_>,
    path: &str,
    meta: &Metadata,
) -> Result<Vec<String>, Rejection> {
    if !meta.is_available(meta.current_version) {
        return Ok(Vec::new());
    }
    match state.openbao.read(path, Some(meta.current_version)).await {
        Ok(secret) => Ok(secret.data.into_keys().collect()),
        Err(openbao::Error::NotFound) => Ok(Vec::new()),
        Err(failure) => Err(scope.openbao(Some(meta.current_version), failure)),
    }
}
