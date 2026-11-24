use std::process::ExitCode;
use std::time::Duration;

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde_json::{Value, json};

use crate::api::GatewayState;
use crate::config::Config;

const TIMEOUT: Duration = Duration::from_secs(4);

pub fn liveness_router() -> Router {
    Router::new().route("/healthz", get(health))
}

pub fn router(state: GatewayState) -> Router {
    liveness_router().merge(Router::new().route("/readyz", get(ready)).with_state(state))
}

#[allow(clippy::unused_async)]
async fn health() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": "secrets-gateway",
        "version": crate::VERSION
    }))
}

async fn ready(State(state): State<GatewayState>) -> Response {
    if !state.access.cache().is_fresh() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"status": "not_ready", "reason": "access_snapshot"})),
        )
            .into_response();
    }
    match state.openbao.health().await {
        Ok(()) => (
            StatusCode::OK,
            Json(json!({
                "status": "ready",
                "service": "secrets-gateway",
                "version": crate::VERSION
            })),
        )
            .into_response(),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"status": "not_ready", "reason": "openbao"})),
        )
            .into_response(),
    }
}

pub async fn run_healthcheck() -> ExitCode {
    let config = match Config::load() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("healthcheck: configuration: {error}");
            return ExitCode::FAILURE;
        }
    };
    let base_url = match config.healthcheck_url() {
        Ok(url) => url,
        Err(error) => {
            eprintln!("healthcheck: {error}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(error) = check(&base_url).await {
        eprintln!("healthcheck: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

async fn check(base_url: &str) -> Result<(), String> {
    home_tls::install_crypto_provider();
    let client = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .build()
        .map_err(|error| format!("http client: {error}"))?;
    let response = client
        .get(format!("{}/healthz", base_url.trim_end_matches('/')))
        .send()
        .await
        .map_err(|error| format!("/healthz request failed: {error}"))?;
    if response.status() != StatusCode::OK {
        return Err(format!(
            "/healthz returned HTTP {}",
            response.status().as_u16()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn healthz_answers_without_a_token() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            let _ = axum::serve(listener, liveness_router()).await;
        });
        let client = home_tls::public_https_client()
            .expect("client builder")
            .build()
            .expect("client");
        let response = client
            .get(format!("http://{address}/healthz"))
            .send()
            .await
            .expect("healthz");
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value =
            serde_json::from_str(&response.text().await.expect("body")).expect("json");
        assert_eq!(body["service"], "secrets-gateway");
    }
}
