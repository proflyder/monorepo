//! The compatibility error envelope used by every API failure.

use std::collections::BTreeMap;

use axum::Json;
use axum::response::{IntoResponse, Response};
use http::StatusCode;
use serde::Serialize;

use super::ApiState;

/// Go's `errorResponse` field order and optional fields.
#[derive(Debug, Serialize)]
pub struct ErrorEnvelope {
    pub timestamp: String,
    pub status: u16,
    pub error: &'static str,
    pub message: String,
    pub path: String,
    #[serde(rename = "errorCode", skip_serializing_if = "Option::is_none")]
    pub error_code: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<BTreeMap<String, String>>,
}

/// Build and return an error response for `path`.
#[allow(clippy::too_many_arguments)]
pub fn write_error(
    state: &ApiState,
    path: &str,
    status: StatusCode,
    error: &'static str,
    message: impl Into<String>,
    error_code: &'static str,
    service: &'static str,
    details: Option<BTreeMap<String, String>>,
) -> Response {
    let envelope = ErrorEnvelope {
        timestamp: crate::domain::time::rfc3339_nano(state.clock.now()),
        status: status.as_u16(),
        error,
        message: message.into(),
        path: path.to_owned(),
        error_code: Some(error_code),
        service: Some(service),
        details,
    };
    (status, Json(envelope)).into_response()
}

/// 401 for a missing or invalid bearer token.
pub fn unauthorized(state: &ApiState, path: &str) -> Response {
    write_error(
        state,
        path,
        StatusCode::UNAUTHORIZED,
        "Unauthorized",
        "Authentication required: Invalid or missing API key",
        "UNAUTHORIZED",
        "auth",
        None,
    )
}
