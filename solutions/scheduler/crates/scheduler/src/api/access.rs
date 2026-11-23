use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use home_app_auth::{AccessCache, Forbidden, MODULES, Module, Modules};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::auth::{AccessAuth, error_response};
use crate::domain::{SyncChange, SyncEntity};
use crate::store::{AccessToken, Store, StoreError};
use crate::sync::{Announcement, SyncSink};

#[derive(Clone)]
pub struct AccessState {
    pub store: Arc<Store>,
    pub auth: Arc<AccessAuth>,
    pub cache: AccessCache,
    pub sync: Arc<dyn SyncSink>,
}

impl std::fmt::Debug for AccessState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AccessState")
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateBody {
    pub name: String,
    #[serde(default)]
    pub modules: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateBody {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub modules: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TokenView {
    id: String,
    name: String,
    modules: Vec<&'static str>,
    owner: bool,
    created_unix: String,
    updated_unix: String,
}

impl From<AccessToken> for TokenView {
    fn from(token: AccessToken) -> Self {
        Self {
            id: token.id,
            name: token.name,
            modules: token.modules.ids(),
            owner: token.owner,
            created_unix: token.created_unix.to_string(),
            updated_unix: token.updated_unix.to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MeView {
    name: String,
    modules: Vec<&'static str>,
    owner: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModuleView {
    id: &'static str,
    title: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreatedView {
    token: String,
    access: TokenView,
}

fn forbidden(forbidden: Forbidden) -> Response {
    let (status, headers, body) = error_response(forbidden);
    (status, headers, body).into_response()
}

fn store_error(error: StoreError) -> Response {
    match error {
        StoreError::NotFound => {
            (StatusCode::NOT_FOUND, Json(json!({"error": "not_found"}))).into_response()
        }
        StoreError::Domain(domain) if domain.kind() == crate::domain::ErrorKind::Precondition => (
            StatusCode::CONFLICT,
            Json(json!({"error": "conflict", "detail": domain.to_string()})),
        )
            .into_response(),
        StoreError::Domain(domain) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid", "detail": domain.to_string()})),
        )
            .into_response(),
        other => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "internal", "detail": other.to_string()})),
        )
            .into_response(),
    }
}

fn parse_modules(values: &[String]) -> Result<Modules, Box<Response>> {
    Modules::parse(values).map_err(|error| {
        Box::new(
            (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "invalid", "detail": error.to_string()})),
            )
                .into_response(),
        )
    })
}

fn announce(state: &AccessState, id: &str, deleted: bool) {
    let change = if deleted {
        SyncChange::delete(SyncEntity::Access, id)
    } else {
        SyncChange::upsert(SyncEntity::Access, id)
    };
    state.sync.publish(Announcement {
        change,
        origin: None,
        target: None,
    });
}

fn reload(state: &AccessState) -> Result<(), Box<Response>> {
    let snapshot = state
        .store
        .access_snapshot()
        .map_err(|error| Box::new(store_error(error)))?;
    state.cache.replace(snapshot);
    Ok(())
}

pub async fn me(State(state): State<AccessState>, headers: HeaderMap) -> Response {
    let principal = match state.auth.any_app(headers.get(header::AUTHORIZATION)) {
        Ok(principal) => principal,
        Err(error) => return forbidden(error),
    };
    Json(MeView {
        name: principal.name.clone(),
        modules: principal.module_ids(),
        owner: principal.owner,
    })
    .into_response()
}

pub async fn modules(State(state): State<AccessState>, headers: HeaderMap) -> Response {
    if let Err(error) = state
        .auth
        .app(headers.get(header::AUTHORIZATION), Module::Admin)
    {
        return forbidden(error);
    }
    let modules: Vec<ModuleView> = MODULES
        .iter()
        .map(|module| ModuleView {
            id: module.id(),
            title: module.title(),
        })
        .collect();
    Json(json!({"modules": modules})).into_response()
}

pub async fn list(State(state): State<AccessState>, headers: HeaderMap) -> Response {
    if let Err(error) = state
        .auth
        .app(headers.get(header::AUTHORIZATION), Module::Admin)
    {
        return forbidden(error);
    }
    match state.store.access_list() {
        Ok(tokens) => {
            let tokens: Vec<TokenView> = tokens.into_iter().map(TokenView::from).collect();
            Json(json!({"tokens": tokens})).into_response()
        }
        Err(error) => store_error(error),
    }
}

pub async fn create(
    State(state): State<AccessState>,
    headers: HeaderMap,
    Json(body): Json<CreateBody>,
) -> Response {
    if let Err(error) = state
        .auth
        .app(headers.get(header::AUTHORIZATION), Module::Admin)
    {
        return forbidden(error);
    }
    let modules = match parse_modules(&body.modules) {
        Ok(modules) => modules,
        Err(response) => return *response,
    };
    let token = match home_secret::hex(32) {
        Ok(secret) => format!("home_{}", secret.expose()),
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "token_generation"})),
            )
                .into_response();
        }
    };
    let id = uuid::Uuid::new_v4().simple().to_string();
    let digest = home_app_auth::sha256_hex(&token);
    let now = super::now_unix();
    let created = match state
        .store
        .access_create(&id, &body.name, &digest, &modules, now)
    {
        Ok(token) => token,
        Err(error) => return store_error(error),
    };
    if let Err(response) = reload(&state) {
        return *response;
    }
    announce(&state, &created.id, false);
    tracing::info!(
        token = created.name.as_str(),
        modules = ?created.modules.ids(),
        "application token created"
    );
    (
        StatusCode::CREATED,
        Json(CreatedView {
            token,
            access: TokenView::from(created),
        }),
    )
        .into_response()
}

pub async fn update(
    State(state): State<AccessState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<UpdateBody>,
) -> Response {
    if let Err(error) = state
        .auth
        .app(headers.get(header::AUTHORIZATION), Module::Admin)
    {
        return forbidden(error);
    }
    if let Some(name) = body.name.as_deref()
        && let Err(error) = state.store.access_rename(&id, name, super::now_unix())
    {
        return store_error(error);
    }
    if let Some(values) = body.modules {
        let modules = match parse_modules(&values) {
            Ok(modules) => modules,
            Err(response) => return *response,
        };
        if let Err(error) = state
            .store
            .access_set_modules(&id, &modules, super::now_unix())
        {
            return store_error(error);
        }
    }
    let updated = match state.store.access_token(&id) {
        Ok(token) => token,
        Err(error) => return store_error(error),
    };
    if let Err(response) = reload(&state) {
        return *response;
    }
    announce(&state, &updated.id, false);
    tracing::info!(
        token = updated.name.as_str(),
        modules = ?updated.modules.ids(),
        "application token updated"
    );
    Json(json!({"access": TokenView::from(updated)})).into_response()
}

pub async fn revoke(
    State(state): State<AccessState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    if let Err(error) = state
        .auth
        .app(headers.get(header::AUTHORIZATION), Module::Admin)
    {
        return forbidden(error);
    }
    let token = match state.store.access_token(&id) {
        Ok(token) => token,
        Err(error) => return store_error(error),
    };
    if let Err(error) = state.store.access_revoke(&id, super::now_unix()) {
        return store_error(error);
    }
    if let Err(response) = reload(&state) {
        return *response;
    }
    announce(&state, &token.id, true);
    tracing::info!(token = token.name.as_str(), "application token revoked");
    Json(json!({"revoked": true})).into_response()
}

pub async fn snapshot(State(state): State<AccessState>, headers: HeaderMap) -> Response {
    if !state
        .auth
        .service_authorized(headers.get(header::AUTHORIZATION))
    {
        return forbidden(Forbidden::ServiceOnly);
    }
    match state.store.access_snapshot() {
        Ok(snapshot) => Json(snapshot).into_response(),
        Err(error) => store_error(error),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::http::HeaderValue;
    use home_app_auth::{AccessCache, AccessSnapshot, Modules, TokenRecord, sha256_hex};

    use super::*;
    use crate::sync::NoSync;

    fn state() -> AccessState {
        let snapshot = AccessSnapshot {
            revision: 1,
            tokens: vec![
                TokenRecord {
                    id: "owner".to_owned(),
                    name: "owner".to_owned(),
                    sha256: sha256_hex("owner-token"),
                    owner: true,
                    modules: Modules::owner()
                        .ids()
                        .into_iter()
                        .map(str::to_owned)
                        .collect(),
                },
                TokenRecord {
                    id: "mama".to_owned(),
                    name: "Мама".to_owned(),
                    sha256: sha256_hex("mama-token"),
                    owner: false,
                    modules: Modules::member()
                        .ids()
                        .into_iter()
                        .map(str::to_owned)
                        .collect(),
                },
            ],
        };
        let store =
            Arc::new(Store::open_in_memory(crate::store::Config::default()).expect("store"));
        store.access_import_seed(&snapshot, 10).expect("seed");
        let snapshot = store.access_snapshot().expect("snapshot");
        let cache = AccessCache::new(snapshot);
        let auth = Arc::new(AccessAuth::new(
            cache.clone(),
            home_secret::Token::new("service-token"),
        ));
        AccessState {
            store,
            auth,
            cache,
            sync: Arc::new(NoSync),
        }
    }

    fn headers(token: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}")).expect("header"),
        );
        headers
    }

    async fn read_body(response: Response) -> (StatusCode, serde_json::Value) {
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("body");
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
        )
    }

    #[tokio::test]
    async fn me_returns_the_token_name_and_effective_modules() {
        let (status, body) = read_body(me(State(state()), headers("mama-token")).await).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["name"], "Мама");
        assert_eq!(body["owner"], false);
        assert!(
            body["modules"]
                .as_array()
                .expect("modules")
                .iter()
                .any(|module| module == "base")
        );
        assert!(
            !body["modules"]
                .as_array()
                .expect("modules")
                .iter()
                .any(|module| module == "chat")
        );
    }

    #[tokio::test]
    async fn the_admin_api_is_owner_only_and_creates_a_token_once() {
        let state = state();
        let (status, body) =
            read_body(modules(State(state.clone()), headers("mama-token")).await).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["error"], "module_forbidden");
        assert_eq!(body["module"], "admin");

        let (status, _) =
            read_body(modules(State(state.clone()), headers("owner-token")).await).await;
        assert_eq!(status, StatusCode::OK);

        let (status, created) = read_body(
            create(
                State(state.clone()),
                headers("owner-token"),
                Json(CreateBody {
                    name: "Гость".to_owned(),
                    modules: vec!["home".to_owned(), "notifications".to_owned()],
                }),
            )
            .await,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        let token = created["token"].as_str().expect("token").to_owned();
        assert!(token.starts_with("home_"));
        let snapshot = state.store.access_snapshot().expect("snapshot");
        assert!(snapshot.authenticate_token(&token).is_some());
        assert!(
            !snapshot
                .authenticate_token(&token)
                .expect("token")
                .allows(Module::Chat)
        );
    }

    #[tokio::test]
    async fn the_internal_snapshot_requires_the_service_token() {
        let state = state();
        let (status, _) =
            read_body(snapshot(State(state.clone()), headers("owner-token")).await).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, body) =
            read_body(snapshot(State(state), headers("service-token")).await).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["revision"], 1);
    }

    #[test]
    fn module_errors_use_the_documented_body() {
        assert_eq!(
            Forbidden::Module(Module::Chat).body(),
            r#"{"error":"module_forbidden","module":"chat"}"#
        );
    }
}
