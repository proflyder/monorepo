use std::collections::BTreeMap;
use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use home_secret::Token;
use jiff::Timestamp;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::devices::Registry;
use crate::fcm::{self, Message, Sender};
use crate::history::{self, HistoryClient, HistoryEntry};

pub const MAX_REASON_CHARS: usize = 200;
pub const MAX_TITLE_CHARS: usize = 120;
pub const MAX_BODY_CHARS: usize = 500;
pub const MAX_DEEP_LINK_CHARS: usize = 512;
pub const MAX_DATA_ENTRIES: usize = 8;
pub const MAX_DATA_KEY_CHARS: usize = 32;
pub const MAX_DATA_VALUE_CHARS: usize = 256;
const RESERVED_DATA_KEYS: [&str; 5] = ["type", "title", "body", "notificationId", "deepLink"];

#[derive(Clone)]
pub struct PushState {
    pub sender: Arc<Sender>,
    pub registry: Arc<Registry>,
    pub history: Arc<HistoryClient>,
    pub token: Token,
}

impl std::fmt::Debug for PushState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PushState")
            .field("sender", &self.sender)
            .field("registry", &self.registry)
            .field("history", &self.history)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindBody {
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub device_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyBody {
    #[serde(default)]
    pub module: Option<String>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub deep_link: Option<String>,
    #[serde(default)]
    pub data: BTreeMap<String, String>,
    #[serde(default)]
    pub device_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncBody {
    pub entity: String,
    pub op: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub origin_device_id: Option<String>,
    #[serde(default)]
    pub target_device_id: Option<String>,
}

pub const SYNC_ENTITIES: [&str; 6] = [
    "alarms",
    "reminders",
    "timers",
    "scenarios",
    "notifications",
    "access",
];
pub const SYNC_OPS: [&str; 2] = ["upsert", "delete"];
pub const MAX_SYNC_ID_CHARS: usize = 128;
pub const MAX_DEVICE_ID_CHARS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Audience<'a> {
    Everyone,
    Only(&'a str),
    AllBut(&'a str),
    Nobody,
}

impl<'a> Audience<'a> {
    #[must_use]
    pub fn only(device_id: Option<&'a str>) -> Self {
        device_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map_or(Self::Everyone, Self::Only)
    }

    #[must_use]
    pub fn all_but(device_id: Option<&'a str>) -> Self {
        device_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map_or(Self::Everyone, Self::AllBut)
    }

    #[must_use]
    pub fn of_sync(body: &'a SyncBody) -> Self {
        let target = body
            .target_device_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let origin = body
            .origin_device_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        match target {
            Some(target) if Some(target) == origin => Self::Nobody,
            Some(target) => Self::Only(target),
            None => Self::all_but(origin),
        }
    }
}

#[derive(Debug, serde::Serialize, PartialEq, Eq)]
pub struct Delivery {
    pub sent: usize,
    pub failed: usize,
    pub devices: usize,
}

#[derive(Debug, serde::Serialize)]
struct ErrorBody {
    error: &'static str,
}

pub fn router(state: PushState) -> Router {
    Router::new()
        .route("/v1/push/find", axum::routing::post(find))
        .route("/v1/push/notify", axum::routing::post(notify))
        .route("/v1/push/sync", axum::routing::post(sync))
        .route("/v1/push/devices", axum::routing::get(devices))
        .with_state(state)
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceView {
    pub id: String,
    pub name: String,
    pub platform: String,
}

#[derive(Debug, serde::Serialize)]
struct DevicesBody {
    devices: Vec<DeviceView>,
}

pub async fn deliver(
    message: &Message,
    sender: &Sender,
    registry: &Registry,
    history: &HistoryClient,
    now: i64,
    audience: Audience<'_>,
    module: Option<&str>,
) -> Delivery {
    let devices = match registry.list_for(module).await {
        Ok(devices) => devices,
        Err(error) => {
            tracing::warn!(error = %error, "push registry unavailable");
            return Delivery {
                sent: 0,
                failed: 0,
                devices: 0,
            };
        }
    };
    let devices = select_devices(devices, audience);
    let total = devices.len();
    let mut sent = 0;
    let mut failed = 0;
    let row = history_row(message);
    let mut records = Vec::new();
    for device in devices {
        let record_id = uuid::Uuid::new_v4().to_string();
        match sender.send(&device.token, message, &record_id, now).await {
            Ok(()) => {
                sent += 1;
                if row.is_some() {
                    records.push((device.device_id, record_id));
                }
            }
            Err(fcm::Error::Unregistered) => {
                if let Err(error) = registry.forget(&device.token).await {
                    tracing::warn!(error = %error, "could not forget an unregistered token");
                }
            }
            Err(error) => {
                failed += 1;
                tracing::warn!(error = %error, "push delivery failed");
            }
        }
    }
    if let Some(row) = row {
        remember_later(history, records, &row);
    }
    tracing::info!(
        event = "push.deliver",
        kind = message.kind(),
        devices = total,
        sent,
        failed,
        "push fan-out finished"
    );
    Delivery {
        sent,
        failed,
        devices: total,
    }
}

async fn find(
    State(state): State<PushState>,
    headers: HeaderMap,
    Json(body): Json<FindBody>,
) -> Response {
    if !authorized(&state.token, &headers) {
        return error(StatusCode::UNAUTHORIZED, "unauthenticated");
    }
    let reason = body.reason.trim();
    if reason.chars().count() > MAX_REASON_CHARS {
        return error(StatusCode::BAD_REQUEST, "reason is too long");
    }
    let message = Message::Find {
        reason: reason.to_owned(),
    };
    respond(
        deliver(
            &message,
            &state.sender,
            &state.registry,
            &state.history,
            now(),
            Audience::only(body.device_id.as_deref()),
            None,
        )
        .await,
    )
}

async fn notify(
    State(state): State<PushState>,
    headers: HeaderMap,
    Json(body): Json<NotifyBody>,
) -> Response {
    if !authorized(&state.token, &headers) {
        return error(StatusCode::UNAUTHORIZED, "unauthenticated");
    }
    let title = body.title.trim();
    let text = body.body.trim();
    if title.is_empty() || text.is_empty() {
        return error(StatusCode::BAD_REQUEST, "title and body are required");
    }
    if title.chars().count() > MAX_TITLE_CHARS || text.chars().count() > MAX_BODY_CHARS {
        return error(StatusCode::BAD_REQUEST, "title or body is too long");
    }
    let deep_link = body
        .deep_link
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if deep_link.is_some_and(|value| value.chars().count() > MAX_DEEP_LINK_CHARS) {
        return error(StatusCode::BAD_REQUEST, "deep link is too long");
    }
    let Some(data) = notification_data(&body.data) else {
        return error(StatusCode::BAD_REQUEST, "invalid data");
    };
    let message = Message::Notification {
        title: title.to_owned(),
        body: text.to_owned(),
        deep_link: deep_link.map(str::to_owned),
        data,
    };
    respond(
        deliver(
            &message,
            &state.sender,
            &state.registry,
            &state.history,
            now(),
            Audience::only(body.device_id.as_deref()),
            body.module.as_deref(),
        )
        .await,
    )
}

async fn sync(
    State(state): State<PushState>,
    headers: HeaderMap,
    Json(body): Json<SyncBody>,
) -> Response {
    if !authorized(&state.token, &headers) {
        return error(StatusCode::UNAUTHORIZED, "unauthenticated");
    }
    let Some(message) = sync_message(&body) else {
        return error(StatusCode::BAD_REQUEST, "unknown entity, op or id");
    };
    respond(
        deliver(
            &message,
            &state.sender,
            &state.registry,
            &state.history,
            now(),
            Audience::of_sync(&body),
            None,
        )
        .await,
    )
}

fn notification_data(values: &BTreeMap<String, String>) -> Option<BTreeMap<String, String>> {
    if values.len() > MAX_DATA_ENTRIES {
        return None;
    }
    let mut data = BTreeMap::new();
    for (key, value) in values {
        if key.is_empty()
            || key.chars().count() > MAX_DATA_KEY_CHARS
            || !key.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
            })
            || RESERVED_DATA_KEYS.contains(&key.as_str())
            || value.is_empty()
            || value.chars().count() > MAX_DATA_VALUE_CHARS
            || value.chars().any(char::is_control)
        {
            return None;
        }
        data.insert(key.clone(), value.clone());
    }
    Some(data)
}

struct HistoryRow {
    kind: &'static str,
    title: String,
    body: String,
    deep_link: Option<String>,
}

fn history_row(message: &Message) -> Option<HistoryRow> {
    match message {
        Message::Find { reason } => Some(HistoryRow {
            kind: history::FIND_KIND,
            title: String::new(),
            body: reason.clone(),
            deep_link: None,
        }),
        Message::Notification {
            title,
            body,
            deep_link,
            ..
        } => Some(HistoryRow {
            kind: history::NOTIFY_KIND,
            title: title.clone(),
            body: body.clone(),
            deep_link: deep_link.clone(),
        }),
        Message::Sync { .. } => None,
    }
}

fn remember_later(history: &HistoryClient, records: Vec<(String, String)>, row: &HistoryRow) {
    for (device_id, id) in records {
        let history = history.clone();
        let entry = HistoryEntry {
            device_id,
            id,
            kind: row.kind.to_owned(),
            title: row.title.clone(),
            body: row.body.clone(),
            deep_link: row.deep_link.clone(),
        };
        tokio::spawn(async move {
            if let Err(error) = history.record(&entry).await {
                tracing::warn!(%error, "notification history write failed");
            }
        });
    }
}

fn valid_target(raw: Option<&str>) -> bool {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none_or(|value| {
            value.chars().count() <= MAX_DEVICE_ID_CHARS
                && value.chars().all(|character| {
                    character.is_ascii_alphanumeric() || "-_.:".contains(character)
                })
        })
}

fn sync_message(body: &SyncBody) -> Option<Message> {
    let entity = body.entity.trim();
    let op = body.op.trim();
    let id = body.id.trim();
    let valid = SYNC_ENTITIES.contains(&entity)
        && SYNC_OPS.contains(&op)
        && id.chars().count() <= MAX_SYNC_ID_CHARS
        && valid_target(body.target_device_id.as_deref());
    valid.then(|| Message::Sync {
        entity: entity.to_owned(),
        op: op.to_owned(),
        id: id.to_owned(),
    })
}

async fn devices(State(state): State<PushState>, headers: HeaderMap) -> Response {
    if !authorized(&state.token, &headers) {
        return error(StatusCode::UNAUTHORIZED, "unauthenticated");
    }
    match state.registry.list().await {
        Ok(list) => Json(DevicesBody {
            devices: list
                .into_iter()
                .map(|device| DeviceView {
                    id: device.device_id,
                    name: device.device_name,
                    platform: device.platform,
                })
                .collect(),
        })
        .into_response(),
        Err(failure) => {
            tracing::warn!(error = %failure, "push registry unavailable");
            error(StatusCode::BAD_GATEWAY, "registry unavailable")
        }
    }
}

fn select_devices(
    devices: Vec<crate::devices::Device>,
    audience: Audience<'_>,
) -> Vec<crate::devices::Device> {
    devices
        .into_iter()
        .filter(|device| match audience {
            Audience::Everyone => true,
            Audience::Only(target) => device.device_id == target,
            Audience::AllBut(origin) => device.device_id != origin,
            Audience::Nobody => false,
        })
        .collect()
}

fn authorized(token: &Token, headers: &HeaderMap) -> bool {
    let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let value = value.trim();
    let Some(candidate) = value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
    else {
        return false;
    };
    token.ct_eq(candidate.trim())
}

fn now() -> i64 {
    Timestamp::now().as_second()
}

fn respond(delivery: Delivery) -> Response {
    Json(delivery).into_response()
}

fn error(status: StatusCode, message: &'static str) -> Response {
    (status, Json(ErrorBody { error: message })).into_response()
}

#[must_use]
pub fn summary(detail: &Delivery) -> Value {
    json!({"sent": detail.sent, "failed": detail.failed, "devices": detail.devices})
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    use super::*;

    #[tokio::test]
    async fn history_writes_do_not_block_the_caller() {
        let recorded = Arc::new(AtomicUsize::new(0));
        let app = Router::new()
            .route(history::HISTORY_PATH, axum::routing::post(slow_record))
            .with_state(Arc::clone(&recorded));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        let client = HistoryClient::new(
            &format!("http://{address}"),
            Token::new("scheduler-token"),
            Duration::from_secs(5),
        )
        .expect("client");
        let row = HistoryRow {
            kind: history::NOTIFY_KIND,
            title: "Дом".to_owned(),
            body: "Готово".to_owned(),
            deep_link: None,
        };

        let started = Instant::now();
        remember_later(
            &client,
            vec![
                ("device-a".to_owned(), "n-1".to_owned()),
                ("device-b".to_owned(), "n-2".to_owned()),
            ],
            &row,
        );
        assert!(started.elapsed() < Duration::from_millis(100));

        tokio::time::sleep(Duration::from_millis(1_200)).await;
        assert_eq!(recorded.load(Ordering::SeqCst), 2);
    }

    async fn slow_record(State(recorded): State<Arc<AtomicUsize>>) -> Json<Value> {
        tokio::time::sleep(Duration::from_millis(500)).await;
        recorded.fetch_add(1, Ordering::SeqCst);
        Json(serde_json::json!({"entry": {"id": "n-1"}}))
    }

    #[test]
    fn a_bearer_is_required_and_must_match() {
        let token = Token::new("push-token");
        let mut headers = HeaderMap::new();
        assert!(!authorized(&token, &headers));
        headers.insert(header::AUTHORIZATION, "Bearer nope".parse().expect("value"));
        assert!(!authorized(&token, &headers));
        headers.insert(
            header::AUTHORIZATION,
            "Bearer push-token".parse().expect("value"),
        );
        assert!(authorized(&token, &headers));
    }

    fn device(id: &str) -> crate::devices::Device {
        crate::devices::Device {
            device_id: id.to_owned(),
            token: format!("token-{id}-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            platform: "android".to_owned(),
            device_name: id.to_owned(),
        }
    }

    #[test]
    fn an_explicit_device_id_selects_exactly_one_phone() {
        let devices = vec![device("one"), device("two")];
        let selected = select_devices(devices, Audience::only(Some("two")));
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].device_id, "two");
        assert!(select_devices(vec![device("one")], Audience::only(Some("missing"))).is_empty());
    }

    #[test]
    fn a_missing_or_blank_device_id_fans_out_to_every_phone() {
        assert_eq!(
            select_devices(vec![device("one"), device("two")], Audience::only(None)).len(),
            2
        );
        assert_eq!(
            select_devices(
                vec![device("one"), device("two")],
                Audience::only(Some("  "))
            )
            .len(),
            2
        );
    }

    #[test]
    fn a_targeted_sync_goes_only_to_the_owner() {
        let body = SyncBody {
            entity: "notifications".to_owned(),
            op: "upsert".to_owned(),
            id: "n-1".to_owned(),
            origin_device_id: None,
            target_device_id: Some("two".to_owned()),
        };
        let devices = vec![device("one"), device("two")];
        let selected = select_devices(devices, Audience::of_sync(&body));
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].device_id, "two");
    }

    #[test]
    fn a_targeted_sync_is_not_sent_back_to_the_initiator() {
        let body = SyncBody {
            entity: "notifications".to_owned(),
            op: "upsert".to_owned(),
            id: "n-1".to_owned(),
            origin_device_id: Some("two".to_owned()),
            target_device_id: Some("two".to_owned()),
        };
        let devices = vec![device("one"), device("two")];
        assert!(select_devices(devices, Audience::of_sync(&body)).is_empty());
    }

    #[test]
    fn an_invalid_target_device_is_rejected() {
        let mut body = sync_body("notifications", "upsert", "n-1");
        body.target_device_id = Some("bad device".to_owned());
        assert!(sync_message(&body).is_none());
        body.target_device_id = Some("x".repeat(MAX_DEVICE_ID_CHARS + 1));
        assert!(sync_message(&body).is_none());
        body.target_device_id = Some("device-1".to_owned());
        assert!(sync_message(&body).is_some());
    }

    #[test]
    fn a_sync_skips_the_device_that_made_the_change() {
        let devices = vec![device("one"), device("two")];
        let selected = select_devices(devices, Audience::all_but(Some("one")));
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].device_id, "two");
        assert_eq!(
            select_devices(vec![device("one"), device("two")], Audience::all_but(None)).len(),
            2
        );
    }

    fn sync_body(entity: &str, op: &str, id: &str) -> SyncBody {
        SyncBody {
            entity: entity.to_owned(),
            op: op.to_owned(),
            id: id.to_owned(),
            origin_device_id: None,
            target_device_id: None,
        }
    }

    #[test]
    fn a_sync_names_a_known_entity_and_operation() {
        assert_eq!(
            sync_message(&sync_body("timers", "delete", "t-1")),
            Some(Message::Sync {
                entity: "timers".to_owned(),
                op: "delete".to_owned(),
                id: "t-1".to_owned(),
            })
        );
        assert!(sync_message(&sync_body("secrets", "upsert", "x")).is_none());
        assert!(sync_message(&sync_body("alarms", "wipe", "x")).is_none());
        assert!(sync_message(&sync_body("alarms", "upsert", &"x".repeat(200))).is_none());
    }

    #[test]
    fn notification_data_is_bounded_and_cannot_override_reserved_keys() {
        let data = BTreeMap::from([
            ("entity".to_owned(), "secrets".to_owned()),
            ("path".to_owned(), "k8s/test".to_owned()),
        ]);
        assert_eq!(notification_data(&data), Some(data.clone()));
        for key in RESERVED_DATA_KEYS {
            let reserved = BTreeMap::from([(key.to_owned(), "value".to_owned())]);
            assert!(notification_data(&reserved).is_none(), "{key}");
        }
        assert!(
            notification_data(&BTreeMap::from([(
                "k".repeat(MAX_DATA_KEY_CHARS + 1),
                "v".to_owned(),
            )]))
            .is_none()
        );
        assert!(
            notification_data(&BTreeMap::from([(
                "key".to_owned(),
                "v".repeat(MAX_DATA_VALUE_CHARS + 1),
            )]))
            .is_none()
        );
        assert!(notification_data(&BTreeMap::from([("key".to_owned(), String::new())])).is_none());
        assert!(
            notification_data(&BTreeMap::from([("key".to_owned(), "a\nb".to_owned())])).is_none()
        );
        let too_many = (0..=MAX_DATA_ENTRIES)
            .map(|index| (format!("k{index}"), "v".to_owned()))
            .collect();
        assert!(notification_data(&too_many).is_none());
    }

    #[test]
    fn the_summary_has_counts_only() {
        let summary = summary(&Delivery {
            sent: 1,
            failed: 0,
            devices: 1,
        });
        assert_eq!(summary["sent"], 1);
        assert!(summary.get("token").is_none());
    }
}
