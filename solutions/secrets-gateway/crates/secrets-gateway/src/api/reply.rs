use axum::Json;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use home_app_auth::Forbidden;
use serde_json::{Value, json};

use crate::openbao;

#[derive(Debug)]
pub(super) struct Rejection(Box<Response>);

impl From<Response> for Rejection {
    fn from(response: Response) -> Self {
        Self(Box::new(response))
    }
}

impl IntoResponse for Rejection {
    fn into_response(self) -> Response {
        *self.0
    }
}

pub(super) type Reply = Result<Response, Rejection>;

pub(super) fn error(status: StatusCode, code: &'static str) -> Response {
    (status, Json(json!({"error": code}))).into_response()
}

pub(super) fn no_store(body: Value) -> Response {
    let mut response = Json(body).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

pub(super) fn conflict(current_version: u64) -> Response {
    (
        StatusCode::CONFLICT,
        Json(json!({"error": "conflict", "currentVersion": current_version})),
    )
        .into_response()
}

pub(super) fn rate_limited(retry_after: u64) -> Response {
    let mut response = (
        StatusCode::TOO_MANY_REQUESTS,
        Json(json!({"error": "rate_limited", "retryAfter": retry_after})),
    )
        .into_response();
    let value = HeaderValue::from_str(&retry_after.to_string())
        .unwrap_or_else(|_| HeaderValue::from_static("60"));
    response.headers_mut().insert(header::RETRY_AFTER, value);
    response
}

pub(super) fn forbidden(forbidden: &Forbidden) -> Response {
    let mut response = (StatusCode::FORBIDDEN, forbidden.body()).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response
}

pub(super) fn openbao_error(failure: openbao::Error) -> Response {
    match failure {
        openbao::Error::NotFound => error(StatusCode::NOT_FOUND, "not_found"),
        openbao::Error::Conflict => error(StatusCode::CONFLICT, "conflict"),
        openbao::Error::Forbidden => error(StatusCode::BAD_GATEWAY, "openbao_forbidden"),
        openbao::Error::Invalid => error(StatusCode::BAD_REQUEST, "openbao_rejected"),
        openbao::Error::Unavailable => error(StatusCode::BAD_GATEWAY, "openbao_unavailable"),
        openbao::Error::Parse => error(StatusCode::BAD_GATEWAY, "openbao_invalid_response"),
    }
}
