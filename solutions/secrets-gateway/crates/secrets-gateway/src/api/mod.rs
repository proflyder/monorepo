mod audit_scope;
mod catalog;
mod device;
mod guard;
mod reply;
mod request;
mod secret;
mod stepup;
mod validate;

use std::sync::Arc;

use axum::Router;
use axum::routing::{delete, get, post};
use home_app_auth::AccessClient;
use serde_json::{Value, json};

use crate::audit::Auditor;
use crate::catalog::Catalog;
use crate::deny::Deny;
use crate::device::DeviceStore;
use crate::openbao::OpenBao;
use crate::stepup::StepUp;

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

impl GatewayState {
    fn is_hidden(&self, path: &str) -> bool {
        self.deny.is_denied(path) || path == self.device.path()
    }
}

impl std::fmt::Debug for GatewayState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GatewayState")
            .field("max_list_entries", &self.max_list_entries)
            .finish_non_exhaustive()
    }
}

pub fn router(state: GatewayState) -> Router {
    Router::new()
        .route("/v1/secrets", get(catalog::list))
        .route("/v1/secrets/meta/{*path}", get(catalog::metadata))
        .route("/v1/secrets/reveal/{*path}", post(secret::reveal))
        .route("/v1/secrets/update/{*path}", post(secret::update))
        .route("/v1/secrets/rollback/{*path}", post(secret::rollback))
        .route("/v1/secrets/{*path}", delete(secret::delete_latest))
        .route("/v1/step-up/challenge", post(stepup::challenge))
        .route("/v1/step-up", post(stepup::open_session))
        .route("/v1/device", get(device::status))
        .route("/v1/device/challenge", post(device::challenge))
        .route("/v1/device/enroll", post(device::enroll))
        .with_state(state)
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
    fn route_summary_lists_no_destroy_endpoint() {
        let summary = route_summary().to_string();
        assert!(summary.contains("reveal"));
        assert!(!summary.contains("destroy"));
    }
}
