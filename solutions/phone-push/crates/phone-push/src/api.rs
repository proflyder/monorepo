//! HTTP surface: `POST /v1/push/find`, `POST /v1/push/notify` and
//! `GET /v1/push/devices`.
//!
//! Every route requires `Authorization: Bearer <PUSH_TOKEN_FILE>`, compared in
//! constant time. A send fans out to every registered device unless the caller
//! names one by its stable `deviceId`, and reports counts only. The device
//! listing exposes identities and owner-visible names for name matching in the
//! MCP gateway, but never a token: the model cannot enumerate FCM credentials.

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

/// Longest accepted reason; it is spoken by the phone, not stored.
pub const MAX_REASON_CHARS: usize = 200;
/// Longest notification title.
pub const MAX_TITLE_CHARS: usize = 120;
/// Longest notification body.
pub const MAX_BODY_CHARS: usize = 500;
/// Longest deep link; the app validates it again.
pub const MAX_DEEP_LINK_CHARS: usize = 512;

/// Everything a route needs.
#[derive(Clone)]
pub struct PushState {
    pub sender: Arc<Sender>,
    pub registry: Arc<Registry>,
    pub token: Token,
}

impl std::fmt::Debug for PushState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PushState")
            .field("sender", &self.sender)
            .field("registry", &self.registry)
            .finish_non_exhaustive()
    }
}

/// `POST /v1/push/find` body.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindBody {
    /// Optional short reason, e.g. the phrase that triggered it.
    #[serde(default)]
    pub reason: String,
    /// Optional stable device identity; absent means every device.
    #[serde(default)]
    pub device_id: Option<String>,
}

/// `POST /v1/push/notify` body.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyBody {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: String,
    /// Optional in-app destination, e.g. `home://alarms`.
    #[serde(default)]
    pub deep_link: Option<String>,
    /// Optional stable device identity; absent means every device.
    #[serde(default)]
    pub device_id: Option<String>,
}

/// Delivery summary. Counts only — no tokens.
#[derive(Debug, serde::Serialize, PartialEq, Eq)]
pub struct Delivery {
    /// Devices the message reached.
    pub sent: usize,
    /// Devices that failed for a transient reason.
    pub failed: usize,
    /// Devices that were registered for this send.
    pub devices: usize,
}

#[derive(Debug, serde::Serialize)]
struct ErrorBody {
    error: &'static str,
}

/// Builds the push router.
pub fn router(state: PushState) -> Router {
    Router::new()
        .route("/v1/push/find", axum::routing::post(find))
        .route("/v1/push/notify", axum::routing::post(notify))
        .route("/v1/push/devices", axum::routing::get(devices))
        .with_state(state)
}

/// A device as the gateway sees it: identity, owner-visible name, platform.
/// The FCM token stays inside this process; the gateway only picks a target.
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

/// Invokes one message on every registered device.
pub async fn deliver(
    message: &Message,
    sender: &Sender,
    registry: &Registry,
    now: i64,
    device_id: Option<&str>,
) -> Delivery {
    let devices = match registry.list().await {
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
    let devices = select_devices(devices, device_id);
    let total = devices.len();
    let mut sent = 0;
    let mut failed = 0;
    for device in devices {
        match sender.send(&device.token, message, now).await {
            Ok(()) => sent += 1,
            Err(fcm::Error::Unregistered) => {
                // Dead token: forget it so the next send is not wasted.
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
            now(),
            body.device_id.as_deref(),
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
    let message = Message::Notification {
        title: title.to_owned(),
        body: text.to_owned(),
        deep_link: deep_link.map(str::to_owned),
    };
    respond(
        deliver(
            &message,
            &state.sender,
            &state.registry,
            now(),
            body.device_id.as_deref(),
        )
        .await,
    )
}

/// `GET /v1/push/devices` — names for deterministic matching in the gateway.
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

/// Applies the gateway's explicit choice. An unknown id selects nothing,
/// which the caller reports as "the phone did not ring".
fn select_devices(
    devices: Vec<crate::devices::Device>,
    device_id: Option<&str>,
) -> Vec<crate::devices::Device> {
    let Some(target) = device_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return devices;
    };
    devices
        .into_iter()
        .filter(|device| device.device_id == target)
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

/// A small JSON summary for the MCP tool speech: never includes a token.
#[must_use]
pub fn summary(detail: &Delivery) -> Value {
    json!({"sent": detail.sent, "failed": detail.failed, "devices": detail.devices})
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let selected = select_devices(devices, Some("two"));
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].device_id, "two");
        assert!(select_devices(vec![device("one")], Some("missing")).is_empty());
    }

    #[test]
    fn a_missing_or_blank_device_id_fans_out_to_every_phone() {
        assert_eq!(
            select_devices(vec![device("one"), device("two")], None).len(),
            2
        );
        assert_eq!(
            select_devices(vec![device("one"), device("two")], Some("  ")).len(),
            2
        );
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
