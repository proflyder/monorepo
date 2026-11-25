use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use home_app_auth::AccessCache;
use serde::{Deserialize, Serialize};

use crate::store::{DeviceScope, PushDevice, PushDeviceInput, Store, StoreError};

use super::auth::{AccessAuth, Caller, error_response};

#[derive(Clone, Debug)]
pub struct PushState {
    pub store: Arc<Store>,
    pub auth: Arc<AccessAuth>,
    pub cache: AccessCache,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub module: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterBody {
    #[serde(default)]
    pub device_id: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub app_version: String,
    #[serde(default)]
    pub device_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveBody {
    #[serde(default)]
    pub token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameBody {
    #[serde(default)]
    pub device_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceView {
    pub device_id: String,
    pub token: String,
    pub platform: String,
    pub app_version: String,
    pub device_name: String,
    pub created_unix: String,
    pub updated_unix: String,
}

impl From<&PushDevice> for DeviceView {
    fn from(device: &PushDevice) -> Self {
        Self {
            device_id: device.device_id.clone(),
            token: device.token.clone(),
            platform: device.platform.clone(),
            app_version: device.app_version.clone(),
            device_name: device.device_name.clone(),
            created_unix: device.created_unix.to_string(),
            updated_unix: device.updated_unix.to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
struct DeviceResponse {
    device: DeviceView,
}

#[derive(Debug, Serialize)]
struct DevicesResponse {
    devices: Vec<DeviceView>,
}

#[derive(Debug, Serialize)]
struct RemovedResponse {
    removed: bool,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: &'static str,
}

fn forbidden_response(forbidden: home_app_auth::Forbidden) -> Response {
    let (status, headers, body) = error_response(forbidden);
    (status, headers, body).into_response()
}

fn app_principal(
    auth: &AccessAuth,
    headers: &HeaderMap,
) -> Result<home_app_auth::Principal, Box<Response>> {
    auth.any_app(headers.get(header::AUTHORIZATION))
        .map_err(|forbidden| Box::new(forbidden_response(forbidden)))
}

fn caller(
    auth: &AccessAuth,
    headers: &HeaderMap,
    module: home_app_auth::Module,
) -> Result<Caller, Box<Response>> {
    auth.require(headers.get(header::AUTHORIZATION), module)
        .map_err(|forbidden| Box::new(forbidden_response(forbidden)))
}

fn scope(caller: &Caller) -> DeviceScope {
    match caller {
        Caller::Service => DeviceScope::All,
        Caller::App(principal) if principal.owner => DeviceScope::All,
        Caller::App(principal) => DeviceScope::Token(principal.id.clone()),
    }
}

pub async fn register(
    State(state): State<PushState>,
    headers: HeaderMap,
    Json(body): Json<RegisterBody>,
) -> Response {
    let principal = match app_principal(&state.auth, &headers) {
        Ok(principal) => principal,
        Err(response) => return *response,
    };
    let input = PushDeviceInput {
        device_id: body.device_id,
        token: body.token,
        platform: body.platform,
        app_version: body.app_version,
        device_name: principal.name.clone(),
    };
    let now = super::now_unix();
    let result = blocking(Arc::clone(&state.store), move |store| {
        store.register_push_device_for(&input, &principal.id, principal.owner, now)
    })
    .await;
    match result {
        Ok(device) => Json(DeviceResponse {
            device: DeviceView::from(&device),
        })
        .into_response(),
        Err(response) => *response,
    }
}

pub async fn rename(
    State(state): State<PushState>,
    headers: HeaderMap,
    Json(body): Json<RenameBody>,
) -> Response {
    let caller = match caller(&state.auth, &headers, home_app_auth::Module::Notifications) {
        Ok(caller) => caller,
        Err(response) => return *response,
    };
    let scope = scope(&caller);
    let result = blocking(Arc::clone(&state.store), move |store| {
        store.scoped_push_device(&body.device_id, &scope)
    })
    .await;
    match result {
        Ok(device) => Json(DeviceResponse {
            device: DeviceView::from(&device),
        })
        .into_response(),
        Err(response) => *response,
    }
}

pub async fn list(
    State(state): State<PushState>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Response {
    let caller = match caller(&state.auth, &headers, home_app_auth::Module::Notifications) {
        Ok(caller) => caller,
        Err(response) => return *response,
    };
    let scope = scope(&caller);
    let module = match query.module.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(id) => match home_app_auth::Module::from_id(id) {
            Some(module) => Some(module),
            None => return error(StatusCode::BAD_REQUEST, "unknown module"),
        },
    };
    let allowed = module.map(|module| state.cache.token_ids_with(module));
    match blocking(Arc::clone(&state.store), move |store| {
        store.list_push_devices_for(scope)
    })
    .await
    {
        Ok(devices) => Json(DevicesResponse {
            devices: devices
                .iter()
                .filter(|device| {
                    allowed
                        .as_ref()
                        .is_none_or(|ids| ids.iter().any(|id| id == &device.token_id))
                })
                .map(DeviceView::from)
                .collect(),
        })
        .into_response(),
        Err(response) => *response,
    }
}

pub async fn remove(
    State(state): State<PushState>,
    headers: HeaderMap,
    Json(body): Json<RemoveBody>,
) -> Response {
    let caller = match caller(&state.auth, &headers, home_app_auth::Module::Notifications) {
        Ok(caller) => caller,
        Err(response) => return *response,
    };
    let token = body.token;
    let scope = scope(&caller);
    match blocking(Arc::clone(&state.store), move |store| {
        store.remove_push_device_for(&token, scope)
    })
    .await
    {
        Ok(removed) => Json(RemovedResponse { removed }).into_response(),
        Err(response) => *response,
    }
}

async fn blocking<T, F>(store: Arc<Store>, call: F) -> Result<T, Box<Response>>
where
    T: Send + 'static,
    F: FnOnce(&Store) -> Result<T, StoreError> + Send + 'static,
{
    match tokio::task::spawn_blocking(move || call(&store)).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(Box::new(store_error(&error))),
        Err(_) => Err(Box::new(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal error",
        ))),
    }
}

fn store_error(failure: &StoreError) -> Response {
    match failure.kind() {
        Some(crate::domain::ErrorKind::InvalidArgument) => {
            error(StatusCode::BAD_REQUEST, "invalid argument")
        }
        Some(crate::domain::ErrorKind::LimitExceeded) => {
            error(StatusCode::TOO_MANY_REQUESTS, "limit exceeded")
        }
        Some(crate::domain::ErrorKind::NotFound) => error(StatusCode::NOT_FOUND, "not found"),
        Some(crate::domain::ErrorKind::Precondition) => error(StatusCode::CONFLICT, "conflict"),
        _ => error(StatusCode::INTERNAL_SERVER_ERROR, "internal error"),
    }
}

fn error(status: StatusCode, message: &'static str) -> Response {
    (status, Json(ErrorResponse { error: message })).into_response()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::{get, post};
    use tower::ServiceExt;

    use super::*;
    use crate::store::{Config, Store};

    const TOKEN: &str = "push-test-token";

    fn app() -> (Router, Arc<Store>) {
        let store = Arc::new(Store::open_in_memory(Config::default()).expect("store"));
        let state = PushState {
            store: Arc::clone(&store),
            auth: Arc::new(AccessAuth::for_tests(TOKEN)),
            cache: AccessCache::default(),
        };
        let router = Router::new()
            .route("/v1/push/devices", get(list).post(register))
            .route("/v1/push/devices/remove", post(remove))
            .route("/v1/push/devices/rename", post(rename))
            .with_state(state);
        (router, store)
    }

    async fn call(router: Router, request: Request<Body>) -> (StatusCode, String) {
        let response = router.oneshot(request).await.expect("response");
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("body");
        (status, String::from_utf8(body.to_vec()).expect("utf8"))
    }

    fn register_request(body: &str) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri("/v1/push/devices")
            .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_owned()))
            .expect("request")
    }

    fn scoped_app() -> (Router, Arc<Store>) {
        let record = |id: &str, owner: bool, modules: &[&str]| home_app_auth::TokenRecord {
            id: id.to_owned(),
            name: id.to_owned(),
            sha256: home_app_auth::sha256_hex(&format!("{id}-token")),
            owner,
            modules: modules.iter().map(|module| (*module).to_owned()).collect(),
        };
        let cache = AccessCache::new(home_app_auth::AccessSnapshot {
            revision: 1,
            tokens: vec![
                record("owner", true, &["admin"]),
                record("member", false, &["notifications"]),
            ],
        });
        let store = Arc::new(Store::open_in_memory(Config::default()).expect("store"));
        let state = PushState {
            store: Arc::clone(&store),
            auth: Arc::new(AccessAuth::new(
                cache.clone(),
                home_secret::Token::new("service-token"),
            )),
            cache,
        };
        let router = Router::new()
            .route("/v1/push/devices", get(list).post(register))
            .route("/v1/push/devices/remove", post(remove))
            .route("/v1/push/devices/rename", post(rename))
            .with_state(state);
        (router, store)
    }

    fn as_token(method: &str, uri: &str, token: &str, body: &str) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_owned()))
            .expect("request")
    }

    #[tokio::test]
    async fn a_member_token_only_sees_and_changes_its_own_devices() {
        let (router, store) = scoped_app();
        let owner_device = r#"{"deviceId":"device-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "token":"token-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","platform":"android"}"#;
        let member_device = r#"{"deviceId":"device-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "token":"token-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","platform":"android"}"#;
        let (status, _) = call(
            router.clone(),
            as_token("POST", "/v1/push/devices", "owner-token", owner_device),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (status, _) = call(
            router.clone(),
            as_token("POST", "/v1/push/devices", "member-token", member_device),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, listed) = call(
            router.clone(),
            as_token("GET", "/v1/push/devices", "member-token", ""),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(listed.contains("device-bbbb"), "{listed}");
        assert!(!listed.contains("device-aaaa"), "{listed}");

        let rename = r#"{"deviceId":"device-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","deviceName":"x"}"#;
        let (status, _) = call(
            router.clone(),
            as_token("POST", "/v1/push/devices/rename", "member-token", rename),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let own = r#"{"deviceId":"device-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","deviceName":"x"}"#;
        let (status, body) = call(
            router.clone(),
            as_token("POST", "/v1/push/devices/rename", "member-token", own),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(body.contains("\"deviceName\":\"member\""), "{body}");
        let remove = r#"{"token":"token-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#;
        let (status, body) = call(
            router.clone(),
            as_token("POST", "/v1/push/devices/remove", "member-token", remove),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("false"), "{body}");
        assert_eq!(store.list_push_devices().expect("list").len(), 2);

        let (status, alerts) = call(
            router.clone(),
            as_token("GET", "/v1/push/devices?module=alerts", "service-token", ""),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            alerts.contains("device-aaaa") && !alerts.contains("device-bbbb"),
            "{alerts}"
        );
        let (status, _) = call(
            router,
            as_token("GET", "/v1/push/devices?module=alarms", "service-token", ""),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn register_then_list_round_trip() {
        let (router, _store) = app();
        let body = r#"{"token":"token-cccccccccccccccccccccccccccccccccccc",
            "platform":"android","appVersion":"0.1.0","deviceName":"Pixel"}"#;
        let (status, response) = call(router.clone(), register_request(body)).await;
        assert_eq!(status, StatusCode::OK, "{response}");
        assert!(response.contains("\"deviceName\":\"test\""), "{response}");
        assert!(!response.contains("Pixel"), "{response}");

        let request = Request::builder()
            .uri("/v1/push/devices")
            .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
            .body(Body::empty())
            .expect("request");
        let (status, response) = call(router, request).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(response.matches("\"token\"").count(), 1, "{response}");
    }

    #[tokio::test]
    async fn a_missing_bearer_is_rejected_before_touching_the_store() {
        let (router, store) = app();
        let request = Request::builder()
            .method("POST")
            .uri("/v1/push/devices")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from("{}"))
            .expect("request");
        let (status, _) = call(router, request).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(store.list_push_devices().expect("list").is_empty());
    }

    #[tokio::test]
    async fn a_short_token_is_a_bad_request() {
        let (router, _store) = app();
        let (status, _) = call(router, register_request(r#"{"token":"short"}"#)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn a_rotated_token_updates_the_same_device() {
        let (router, store) = app();
        let first = r#"{"deviceId":"device-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "token":"token-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "platform":"android","deviceName":"Pixel"}"#;
        let (status, _) = call(router.clone(), register_request(first)).await;
        assert_eq!(status, StatusCode::OK);
        let rotated = r#"{"deviceId":"device-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "token":"token-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "platform":"android","deviceName":""}"#;
        let (status, response) = call(router, register_request(rotated)).await;
        assert_eq!(status, StatusCode::OK, "{response}");
        assert!(
            response.contains("token-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            "{response}"
        );
        let devices = store.list_push_devices().expect("list");
        assert_eq!(devices.len(), 1);
        assert_eq!(
            devices[0].device_id,
            "device-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        assert_eq!(devices[0].device_name, "test");
    }

    #[tokio::test]
    async fn a_legacy_rename_keeps_the_token_name() {
        let (router, store) = app();
        let body = r#"{"deviceId":"device-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "token":"token-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "platform":"android","deviceName":"Pixel"}"#;
        let (status, _) = call(router.clone(), register_request(body)).await;
        assert_eq!(status, StatusCode::OK);
        let request = Request::builder()
            .method("POST")
            .uri("/v1/push/devices/rename")
            .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                r#"{"deviceId":"device-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","deviceName":"Стас"}"#,
            ))
            .expect("request");
        let (status, response) = call(router, request).await;
        assert_eq!(status, StatusCode::OK, "{response}");
        assert!(response.contains("\"deviceName\":\"test\""), "{response}");
        let devices = store.list_push_devices().expect("list");
        assert_eq!(devices[0].device_name, "test");
        assert_eq!(
            devices[0].token,
            "token-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
    }

    #[tokio::test]
    async fn renaming_an_unknown_device_is_not_found() {
        let (router, _store) = app();
        let request = Request::builder()
            .method("POST")
            .uri("/v1/push/devices/rename")
            .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                r#"{"deviceId":"device-missing","deviceName":"Стас"}"#,
            ))
            .expect("request");
        let (status, _) = call(router, request).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn removing_an_unknown_token_reports_false() {
        let (router, _store) = app();
        let request = Request::builder()
            .method("POST")
            .uri("/v1/push/devices/remove")
            .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                r#"{"token":"token-dddddddddddddddddddddddddddddddddddd"}"#,
            ))
            .expect("request");
        let (status, body) = call(router, request).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, r#"{"removed":false}"#);
    }
}
