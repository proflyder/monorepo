use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use crate::domain::{SyncChange, SyncOp};
use crate::store::{
    DEFAULT_HISTORY_LIMIT, Notification, NotificationInput, NotificationPage, Store, StoreError,
};
use crate::sync::{Announcement, SyncSink};

use super::auth::{AccessAuth, Caller, error_response};

pub const DEVICE_HEADER: &str = "home-device-id";
const MAX_DEVICE_ID_CHARS: usize = 128;

#[derive(Clone, Debug)]
pub struct NotificationState {
    pub store: Arc<Store>,
    pub auth: Arc<AccessAuth>,
    pub sync: Arc<dyn SyncSink>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordBody {
    #[serde(default)]
    pub device_id: String,
    #[serde(default)]
    pub id: String,
    #[serde(default, rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub deep_link: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadBody {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub read: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveBody {
    #[serde(default)]
    pub id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryQuery {
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryView {
    pub id: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub title: String,
    pub body: String,
    pub deep_link: Option<String>,
    pub created_unix: String,
    pub read_unix: String,
}

impl From<&Notification> for EntryView {
    fn from(entry: &Notification) -> Self {
        Self {
            id: entry.id.clone(),
            type_name: entry.kind.wire().to_owned(),
            title: entry.title.clone(),
            body: entry.body.clone(),
            deep_link: entry.deep_link.clone(),
            created_unix: entry.created_unix.to_string(),
            read_unix: entry.read_unix.to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EntryResponse {
    entry: EntryView,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HistoryResponse {
    entries: Vec<EntryView>,
    next_cursor: Option<String>,
}

#[derive(Debug, Serialize)]
struct UnreadResponse {
    unread: i64,
}

#[derive(Debug, Serialize)]
struct UpdatedResponse {
    updated: usize,
}

#[derive(Debug, Serialize)]
struct RemovedResponse {
    removed: bool,
}

#[derive(Debug, Serialize)]
struct RemovedCountResponse {
    removed: usize,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: &'static str,
}

fn caller(auth: &AccessAuth, headers: &HeaderMap) -> Result<Caller, Box<Response>> {
    auth.require(
        headers.get(header::AUTHORIZATION),
        home_app_auth::Module::Notifications,
    )
    .map_err(|forbidden| {
        let (status, headers, body) = error_response(forbidden);
        Box::new((status, headers, body).into_response())
    })
}

async fn ensure_device(
    state: &NotificationState,
    caller: &Caller,
    device_id: &str,
) -> Result<(), Box<Response>> {
    let principal = match caller {
        Caller::Service => return Ok(()),
        Caller::App(principal) if principal.owner => return Ok(()),
        Caller::App(principal) => principal,
    };
    let id = device_id.to_owned();
    let device = blocking(Arc::clone(&state.store), move |store| {
        store.push_device(&id)
    })
    .await
    .map_err(|response| *response)?;
    match device {
        Some(device) if device.token_id == principal.id => Ok(()),
        _ => Err(Box::new(error(StatusCode::NOT_FOUND, "not_found"))),
    }
}

pub async fn record(
    State(state): State<NotificationState>,
    headers: HeaderMap,
    Json(body): Json<RecordBody>,
) -> Response {
    let caller = match caller(&state.auth, &headers) {
        Ok(caller) => caller,
        Err(response) => return *response,
    };
    if body.device_id.trim().is_empty() {
        return error(StatusCode::BAD_REQUEST, "deviceId is required");
    }
    if let Err(response) = ensure_device(&state, &caller, &body.device_id).await {
        return *response;
    }
    let device_id = body.device_id;
    let input = NotificationInput {
        id: body.id,
        kind: body.kind,
        title: body.title,
        body: body.body,
        deep_link: body.deep_link,
    };
    let now = now_unix();
    let result = blocking(Arc::clone(&state.store), move |store| {
        store.record_notification(&device_id, &input, now)
    })
    .await;
    match result {
        Ok(entry) => {
            announce(
                &state,
                SyncOp::Upsert,
                entry.id.clone(),
                Some(entry.device_id.clone()),
                None,
            );
            Json(EntryResponse {
                entry: EntryView::from(&entry),
            })
            .into_response()
        }
        Err(response) => *response,
    }
}

pub async fn history(
    State(state): State<NotificationState>,
    headers: HeaderMap,
    Query(query): Query<HistoryQuery>,
) -> Response {
    let caller = match caller(&state.auth, &headers) {
        Ok(caller) => caller,
        Err(response) => return *response,
    };
    let Some(device_id) = header_device(&headers) else {
        return error(StatusCode::BAD_REQUEST, "device_required");
    };
    if let Err(response) = ensure_device(&state, &caller, &device_id).await {
        return *response;
    }
    let cursor = query
        .cursor
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let limit = query.limit.unwrap_or(DEFAULT_HISTORY_LIMIT);
    let result = blocking(Arc::clone(&state.store), move |store| {
        store.list_notifications(&device_id, cursor.as_deref(), limit)
    })
    .await;
    match result {
        Ok(page) => Json(history_view(&page)).into_response(),
        Err(response) => *response,
    }
}

pub async fn unread(State(state): State<NotificationState>, headers: HeaderMap) -> Response {
    let caller = match caller(&state.auth, &headers) {
        Ok(caller) => caller,
        Err(response) => return *response,
    };
    let Some(device_id) = header_device(&headers) else {
        return error(StatusCode::BAD_REQUEST, "device_required");
    };
    if let Err(response) = ensure_device(&state, &caller, &device_id).await {
        return *response;
    }
    match blocking(Arc::clone(&state.store), move |store| {
        store.unread_notifications(&device_id)
    })
    .await
    {
        Ok(unread) => Json(UnreadResponse { unread }).into_response(),
        Err(response) => *response,
    }
}

pub async fn set_read(
    State(state): State<NotificationState>,
    headers: HeaderMap,
    Json(body): Json<ReadBody>,
) -> Response {
    let caller = match caller(&state.auth, &headers) {
        Ok(caller) => caller,
        Err(response) => return *response,
    };
    let Some(device_id) = header_device(&headers) else {
        return error(StatusCode::BAD_REQUEST, "device_required");
    };
    if let Err(response) = ensure_device(&state, &caller, &device_id).await {
        return *response;
    }
    let now = now_unix();
    let id = body.id;
    let read = body.read;
    let result = blocking(Arc::clone(&state.store), move |store| {
        store.set_notification_read(&device_id, &id, read, now)
    })
    .await;
    match result {
        Ok(entry) => {
            announce(
                &state,
                SyncOp::Upsert,
                entry.id.clone(),
                Some(entry.device_id.clone()),
                Some(entry.device_id.clone()),
            );
            Json(EntryResponse {
                entry: EntryView::from(&entry),
            })
            .into_response()
        }
        Err(response) => *response,
    }
}

pub async fn read_all(State(state): State<NotificationState>, headers: HeaderMap) -> Response {
    let caller = match caller(&state.auth, &headers) {
        Ok(caller) => caller,
        Err(response) => return *response,
    };
    let Some(device_id) = header_device(&headers) else {
        return error(StatusCode::BAD_REQUEST, "device_required");
    };
    if let Err(response) = ensure_device(&state, &caller, &device_id).await {
        return *response;
    }
    let now = now_unix();
    let target = device_id.clone();
    match blocking(Arc::clone(&state.store), move |store| {
        store.mark_notifications_read(&device_id, now)
    })
    .await
    {
        Ok(updated) => {
            if updated > 0 {
                announce(
                    &state,
                    SyncOp::Upsert,
                    String::new(),
                    Some(target.clone()),
                    Some(target),
                );
            }
            Json(UpdatedResponse { updated }).into_response()
        }
        Err(response) => *response,
    }
}

pub async fn remove(
    State(state): State<NotificationState>,
    headers: HeaderMap,
    Json(body): Json<RemoveBody>,
) -> Response {
    let caller = match caller(&state.auth, &headers) {
        Ok(caller) => caller,
        Err(response) => return *response,
    };
    let Some(device_id) = header_device(&headers) else {
        return error(StatusCode::BAD_REQUEST, "device_required");
    };
    if let Err(response) = ensure_device(&state, &caller, &device_id).await {
        return *response;
    }
    let id = body.id;
    let key = id.clone();
    let target = device_id.clone();
    match blocking(Arc::clone(&state.store), move |store| {
        store.remove_notification(&device_id, &key)
    })
    .await
    {
        Ok(removed) => {
            if removed {
                announce(
                    &state,
                    SyncOp::Delete,
                    id,
                    Some(target.clone()),
                    Some(target),
                );
            }
            Json(RemovedResponse { removed }).into_response()
        }
        Err(response) => *response,
    }
}

pub async fn clear(State(state): State<NotificationState>, headers: HeaderMap) -> Response {
    let caller = match caller(&state.auth, &headers) {
        Ok(caller) => caller,
        Err(response) => return *response,
    };
    let Some(device_id) = header_device(&headers) else {
        return error(StatusCode::BAD_REQUEST, "device_required");
    };
    if let Err(response) = ensure_device(&state, &caller, &device_id).await {
        return *response;
    }
    let target = device_id.clone();
    match blocking(Arc::clone(&state.store), move |store| {
        store.clear_notifications(&device_id)
    })
    .await
    {
        Ok(removed) => {
            if removed > 0 {
                announce(
                    &state,
                    SyncOp::Delete,
                    String::new(),
                    Some(target.clone()),
                    Some(target),
                );
            }
            Json(RemovedCountResponse { removed }).into_response()
        }
        Err(response) => *response,
    }
}

fn history_view(page: &NotificationPage) -> HistoryResponse {
    HistoryResponse {
        entries: page.entries.iter().map(EntryView::from).collect(),
        next_cursor: page.next_cursor.clone(),
    }
}

fn header_device(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(DEVICE_HEADER)?.to_str().ok()?.trim();
    let valid = !value.is_empty()
        && value.chars().count() <= MAX_DEVICE_ID_CHARS
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.:".contains(character));
    valid.then(|| value.to_owned())
}

fn announce(
    state: &NotificationState,
    op: SyncOp,
    id: String,
    target: Option<String>,
    origin: Option<String>,
) {
    state.sync.publish(Announcement {
        change: SyncChange::notification(op, id),
        origin,
        target,
    });
}

async fn blocking<T, F>(store: Arc<Store>, call: F) -> Result<T, Box<Response>>
where
    T: Send + 'static,
    F: FnOnce(&Store) -> Result<T, StoreError> + Send + 'static,
{
    match tokio::task::spawn_blocking(move || call(&store)).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(failure)) => Err(Box::new(store_error(&failure))),
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
    use crate::store::{Config, PushDeviceInput, Store};
    use crate::sync::MemorySync;

    const TOKEN: &str = "push-test-token";
    const DEVICE: &str = "device-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn app() -> (Router, Arc<Store>, Arc<MemorySync>) {
        let store = Arc::new(Store::open_in_memory(Config::default()).expect("store"));
        store
            .register_push_device(
                &PushDeviceInput {
                    device_id: DEVICE.to_owned(),
                    token: "token-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
                    platform: "android".to_owned(),
                    app_version: "0.1.0".to_owned(),
                    device_name: "Стас".to_owned(),
                },
                1,
            )
            .expect("device");
        let sync = Arc::new(MemorySync::default());
        let state = NotificationState {
            store: Arc::clone(&store),
            auth: Arc::new(AccessAuth::for_tests(TOKEN)),
            sync: sync.clone(),
        };
        let router = Router::new()
            .route("/v1/push/history", get(history).post(record))
            .route("/v1/push/history/unread-count", get(unread))
            .route("/v1/push/history/read", post(set_read))
            .route("/v1/push/history/read-all", post(read_all))
            .route("/v1/push/history/remove", post(remove))
            .route("/v1/push/history/clear", post(clear))
            .with_state(state);
        (router, store, sync)
    }

    async fn call(router: Router, request: Request<Body>) -> (StatusCode, String) {
        let response = router.oneshot(request).await.expect("response");
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("body");
        (status, String::from_utf8(body.to_vec()).expect("utf8"))
    }

    fn request(method: &str, uri: &str, body: &str) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
            .header(DEVICE_HEADER, DEVICE)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_owned()))
            .expect("request")
    }

    fn record_body(id: &str) -> String {
        format!(
            r#"{{"deviceId":"{DEVICE}","id":"{id}","type":"notify",
                "title":"Title","body":"Body","deepLink":"home://alarms"}}"#
        )
    }

    #[tokio::test]
    async fn a_recorded_entry_is_listed_and_counted() {
        let (router, _store, _sync) = app();
        let (status, response) = call(
            router.clone(),
            request("POST", "/v1/push/history", &record_body("n-1")),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{response}");
        assert!(response.contains("\"type\":\"notify\""), "{response}");

        let (status, response) = call(
            router.clone(),
            request("GET", "/v1/push/history?limit=20", ""),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{response}");
        assert!(response.contains("\"id\":\"n-1\""), "{response}");
        assert!(response.contains("\"nextCursor\":null"), "{response}");

        let (status, response) =
            call(router, request("GET", "/v1/push/history/unread-count", "")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(response, r#"{"unread":1}"#);
    }

    #[tokio::test]
    async fn read_unread_remove_and_clear_change_the_count() {
        let (router, _store, _sync) = app();
        for id in ["n-1", "n-2"] {
            let _ = call(
                router.clone(),
                request("POST", "/v1/push/history", &record_body(id)),
            )
            .await;
        }
        let (status, _) = call(
            router.clone(),
            request(
                "POST",
                "/v1/push/history/read",
                r#"{"id":"n-1","read":true}"#,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (_, response) = call(
            router.clone(),
            request("GET", "/v1/push/history/unread-count", ""),
        )
        .await;
        assert_eq!(response, r#"{"unread":1}"#);

        let (status, _) = call(
            router.clone(),
            request("POST", "/v1/push/history/read-all", ""),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (_, response) = call(
            router.clone(),
            request("GET", "/v1/push/history/unread-count", ""),
        )
        .await;
        assert_eq!(response, r#"{"unread":0}"#);

        let (status, response) = call(
            router.clone(),
            request("POST", "/v1/push/history/remove", r#"{"id":"n-1"}"#),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(response, r#"{"removed":true}"#);

        let (status, response) = call(router, request("POST", "/v1/push/history/clear", "")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(response, r#"{"removed":1}"#);
    }

    #[tokio::test]
    async fn the_history_requires_a_device_header() {
        let (router, _store, _sync) = app();
        let request = Request::builder()
            .uri("/v1/push/history")
            .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
            .body(Body::empty())
            .expect("request");
        let (status, response) = call(router, request).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(response, r#"{"error":"device_required"}"#);
    }

    #[tokio::test]
    async fn a_missing_bearer_is_rejected_before_the_store() {
        let (router, store, _sync) = app();
        let request = Request::builder()
            .uri("/v1/push/history")
            .header(DEVICE_HEADER, DEVICE)
            .body(Body::empty())
            .expect("request");
        let (status, _) = call(router, request).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(
            store
                .list_notifications(DEVICE, None, 20)
                .expect("list")
                .entries
                .is_empty()
        );
    }

    #[tokio::test]
    async fn the_cursor_pages_through_older_entries() {
        let (router, _store, _sync) = app();
        for index in 0..3 {
            let _ = call(
                router.clone(),
                request(
                    "POST",
                    "/v1/push/history",
                    &record_body(&format!("n-{index}")),
                ),
            )
            .await;
        }
        let (_, first) = call(
            router.clone(),
            request("GET", "/v1/push/history?limit=2", ""),
        )
        .await;
        let page: serde_json::Value = serde_json::from_str(&first).expect("json");
        assert_eq!(page["entries"].as_array().expect("entries").len(), 2);
        let cursor = page["nextCursor"].as_str().expect("cursor").to_owned();
        let (_, second) = call(
            router,
            request(
                "GET",
                &format!("/v1/push/history?limit=2&cursor={cursor}"),
                "",
            ),
        )
        .await;
        let page: serde_json::Value = serde_json::from_str(&second).expect("json");
        assert_eq!(page["entries"].as_array().expect("entries").len(), 1);
        assert!(page["nextCursor"].is_null());
    }

    #[tokio::test]
    async fn every_change_announces_a_notification_sync() {
        let (router, _store, sync) = app();
        let _ = call(
            router.clone(),
            request("POST", "/v1/push/history", &record_body("n-1")),
        )
        .await;
        let _ = call(
            router.clone(),
            request(
                "POST",
                "/v1/push/history/read",
                r#"{"id":"n-1","read":true}"#,
            ),
        )
        .await;
        let _ = call(
            router.clone(),
            request("POST", "/v1/push/history/remove", r#"{"id":"n-1"}"#),
        )
        .await;
        let announcements = sync.taken();
        assert_eq!(announcements.len(), 3);
        assert!(
            announcements
                .iter()
                .all(|it| it.change.entity == crate::domain::SyncEntity::Notifications)
        );
        assert_eq!(announcements[0].change.op, SyncOp::Upsert);
        assert_eq!(announcements[0].target.as_deref(), Some(DEVICE));
        assert_eq!(announcements[0].origin, None);
        assert_eq!(announcements[1].target.as_deref(), Some(DEVICE));
        assert_eq!(announcements[1].origin.as_deref(), Some(DEVICE));
        assert_eq!(announcements[2].change.op, SyncOp::Delete);
    }
    #[tokio::test]
    async fn a_foreign_device_id_is_not_found() {
        let store = Arc::new(Store::open_in_memory(Config::default()).expect("store"));
        store
            .register_push_device_for(
                &PushDeviceInput {
                    device_id: DEVICE.to_owned(),
                    token: "token-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
                    platform: "android".to_owned(),
                    app_version: "0.1.0".to_owned(),
                    device_name: "Стас".to_owned(),
                },
                "owner",
                1,
            )
            .expect("device");
        let snapshot = home_app_auth::AccessSnapshot {
            revision: 1,
            tokens: vec![home_app_auth::TokenRecord {
                id: "member".to_owned(),
                name: "Мама".to_owned(),
                sha256: home_app_auth::sha256_hex("member-token"),
                owner: false,
                modules: home_app_auth::Modules::member()
                    .ids()
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            }],
        };
        let cache = home_app_auth::AccessCache::new(snapshot);
        let auth = Arc::new(AccessAuth::new(
            cache,
            home_secret::Token::new("service-token"),
        ));
        let state = NotificationState {
            store,
            auth,
            sync: Arc::new(MemorySync::default()),
        };
        let router = Router::new()
            .route("/v1/push/history", get(history))
            .with_state(state);
        let request = Request::builder()
            .uri("/v1/push/history")
            .header(header::AUTHORIZATION, "Bearer member-token")
            .header(DEVICE_HEADER, DEVICE)
            .body(Body::empty())
            .expect("request");
        let (status, _) = call(router, request).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}
