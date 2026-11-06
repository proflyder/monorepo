//! Distroless `-healthcheck`: GET the public `/readyz` probe.
//!
//! The image has no shell, curl or wget, so the binary is its own probe. It
//! loads the same configuration as the server (a broken Secret mount fails the
//! probe) and asks its loopback address.

use std::process::ExitCode;
use std::time::Duration;

use crate::config::Config;

const TIMEOUT: Duration = Duration::from_secs(4);

/// `currency-bot -healthcheck`: `/readyz` must return HTTP 200.
pub async fn run_healthcheck() -> ExitCode {
    let config = match Config::load() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("healthcheck: configuration: {error}");
            return ExitCode::FAILURE;
        }
    };
    let address = match config.healthcheck_address() {
        Ok(address) => address,
        Err(error) => {
            eprintln!("healthcheck: {error}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(error) = check_ready(&format!("http://{address}/readyz")).await {
        eprintln!("healthcheck: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// GET `/readyz` and require 200.
pub async fn check_ready(url: &str) -> Result<(), String> {
    let _already_installed = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .build()
        .map_err(|error| format!("http client: {error}"))?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("readyz request failed: {error}"))?;
    if response.status() != reqwest::StatusCode::OK {
        return Err(format!(
            "readyz returned HTTP {}",
            response.status().as_u16()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use http::StatusCode;

    use super::check_ready;

    async fn serve(status: StatusCode) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("addr");
        let app = axum::Router::new().route(
            "/readyz",
            axum::routing::get(move || async move { (status, "{}") }),
        );
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        format!("http://{address}/readyz")
    }

    #[tokio::test]
    async fn readiness_requires_http_200() {
        let ready = serve(StatusCode::OK).await;
        check_ready(&ready).await.expect("ready");
        let unavailable = serve(StatusCode::SERVICE_UNAVAILABLE).await;
        let error = check_ready(&unavailable).await.expect_err("unavailable");
        assert!(error.contains("HTTP 503"));
    }
}
