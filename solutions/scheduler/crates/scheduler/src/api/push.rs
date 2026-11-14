//! `POST /v1/push/devices` — the phone's push registration surface.
//!
//! It is deliberately *not* a Connect RPC: device registration is device
//! plumbing, not part of the durable schedule contract, and keeping it a plain
//! JSON route means the phone reuses the scheduler base URL and bearer without
//! a new Remote Config key or a second endpoint. The scheduler stores tokens
//! only; delivery happens in `solutions/phone-push`.
//!
//! Authentication is the same bearer token as every Connect RPC, compared in
//! constant time ([`super::auth::BearerAuth`]).

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use crate::store::{PushDevice, PushDeviceInput, Store, StoreError};

use super::auth::BearerAuth;

/// Shared state of the push routes.
#[derive(Clone, Debug)]
pub struct PushState {
    pub store: Arc<Store>,
    pub auth: Arc<BearerAuth>,
}

/// Registration body: the FCM token plus optional owner-visible metadata.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterBody {
    /// Stable device identity chosen by the app (never the FCM token).
    #[serde(default)]
    pub device_id: String,
    /// FCM registration token.
    #[serde(default)]
    pub token: String,
    /// Client platform, `android` today.
    #[serde(default)]
    pub platform: String,
    /// App version string, for operator diagnostics.
    #[serde(default)]
    pub app_version: String,
    /// Owner-visible device name.
    #[serde(default)]
    pub device_name: String,
}

/// Removal body.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveBody {
    /// Token to forget.
    #[serde(default)]
    pub token: String,
}

/// Rename body: the name is server-side metadata, no token re-registration.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameBody {
    /// Stable device identity.
    #[serde(default)]
    pub device_id: String,
    /// New owner-visible name.
    #[serde(default)]
    pub device_name: String,
}

/// One device as the wire contract exposes it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceView {
    pub device_id: String,
    pub token: String,
    pub platform: String,
    pub app_version: String,
    pub device_name: String,
    /// Decimal string: protobuf JSON carries int64 this way, and the phone
    /// keeps a single JSON convention.
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

/// `POST /v1/push/devices`.
pub async fn register(
    State(state): State<PushState>,
    headers: HeaderMap,
    Json(body): Json<RegisterBody>,
) -> Response {
    if !state.auth.authorized(headers.get(header::AUTHORIZATION)) {
        return error(StatusCode::UNAUTHORIZED, "unauthenticated");
    }
    let input = PushDeviceInput {
        device_id: body.device_id,
        token: body.token,
        platform: body.platform,
        app_version: body.app_version,
        device_name: body.device_name,
    };
    let now = now_unix();
    let result = blocking(Arc::clone(&state.store), move |store| {
        store.register_push_device(&input, now)
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

/// `POST /v1/push/devices/rename` — changes the owner-visible name only.
pub async fn rename(
    State(state): State<PushState>,
    headers: HeaderMap,
    Json(body): Json<RenameBody>,
) -> Response {
    if !state.auth.authorized(headers.get(header::AUTHORIZATION)) {
        return error(StatusCode::UNAUTHORIZED, "unauthenticated");
    }
    let now = now_unix();
    let result = blocking(Arc::clone(&state.store), move |store| {
        store.rename_push_device(&body.device_id, &body.device_name, now)
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

/// `GET /v1/push/devices` — the sender reads the registry from here.
pub async fn list(State(state): State<PushState>, headers: HeaderMap) -> Response {
    if !state.auth.authorized(headers.get(header::AUTHORIZATION)) {
        return error(StatusCode::UNAUTHORIZED, "unauthenticated");
    }
    match blocking(Arc::clone(&state.store), Store::list_push_devices).await {
        Ok(devices) => Json(DevicesResponse {
            devices: devices.iter().map(DeviceView::from).collect(),
        })
        .into_response(),
        Err(response) => *response,
    }
}

/// `POST /v1/push/devices/remove`.
pub async fn remove(
    State(state): State<PushState>,
    headers: HeaderMap,
    Json(body): Json<RemoveBody>,
) -> Response {
    if !state.auth.authorized(headers.get(header::AUTHORIZATION)) {
        return error(StatusCode::UNAUTHORIZED, "unauthenticated");
    }
    let token = body.token;
    match blocking(Arc::clone(&state.store), move |store| {
        store.remove_push_device(&token)
    })
    .await
    {
        Ok(removed) => Json(RemovedResponse { removed }).into_response(),
        Err(response) => *response,
    }
}

/// Runs one store operation on the blocking pool: the store owns a single
/// SQLite writer thread and must never block the reactor.
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

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
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
            auth: Arc::new(BearerAuth::new(TOKEN)),
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

    #[tokio::test]
    async fn register_then_list_round_trip() {
        let (router, _store) = app();
        let body = r#"{"token":"token-cccccccccccccccccccccccccccccccccccc",
            "platform":"android","appVersion":"0.1.0","deviceName":"Pixel"}"#;
        let (status, response) = call(router.clone(), register_request(body)).await;
        assert_eq!(status, StatusCode::OK, "{response}");
        assert!(response.contains("\"deviceName\":\"Pixel\""), "{response}");

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
        assert_eq!(devices[0].device_name, "Pixel");
    }

    #[tokio::test]
    async fn renaming_a_device_keeps_its_token() {
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
        assert!(response.contains("\"deviceName\":\"Стас\""), "{response}");
        let devices = store.list_push_devices().expect("list");
        assert_eq!(devices[0].device_name, "Стас");
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
