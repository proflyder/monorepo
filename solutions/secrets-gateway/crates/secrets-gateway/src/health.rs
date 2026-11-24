use std::process::ExitCode;
use std::time::Duration;

use axum::Json;
use axum::Router;
use axum::routing::get;
use serde_json::{Value, json};

use crate::config::Config;

const TIMEOUT: Duration = Duration::from_secs(4);

pub fn router() -> Router {
    Router::new()
        .route("/healthz", get(health))
        .route("/readyz", get(health))
}

#[allow(clippy::unused_async)]
async fn health() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": "secrets-gateway",
        "version": crate::VERSION
    }))
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
    if response.status() != reqwest::StatusCode::OK {
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
            let _ = axum::serve(listener, router()).await;
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
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let body: Value =
            serde_json::from_str(&response.text().await.expect("body")).expect("json");
        assert_eq!(body["service"], "secrets-gateway");
    }
}
