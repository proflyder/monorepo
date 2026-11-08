//! Distroless probes: `-healthcheck` and `-status`.
//!
//! The image has no shell or curl, so the binary probes its own loopback
//! `/healthz` and `/metrics`. No token is needed on these routes and no secret
//! enters a command line or an error message.

use std::process::ExitCode;
use std::time::Duration;

use crate::config::Config;

const TIMEOUT: Duration = Duration::from_secs(4);

/// `intercom -healthcheck`: loopback `/healthz` must answer 200.
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
    if let Err(error) = check(&format!("http://{address}/healthz")).await {
        eprintln!("healthcheck: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// `intercom -status`: prints `GetIntercomStatus` JSON using the phone bearer.
pub async fn run_status() -> ExitCode {
    let config = match Config::load() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("status: configuration: {error}");
            return ExitCode::FAILURE;
        }
    };
    let address = match config.healthcheck_address() {
        Ok(address) => address,
        Err(error) => {
            eprintln!("status: {error}");
            return ExitCode::FAILURE;
        }
    };
    match status_body(&format!("http://{address}"), config.phone_token.expose()).await {
        Ok(body) => {
            print!("{body}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("status: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn check(url: &str) -> Result<(), String> {
    let client = client()?;
    let response = client
        .get(url)
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

async fn status_body(base_url: &str, token: &str) -> Result<String, String> {
    let client = client()?;
    let response = client
        .post(format!(
            "{}/home.v1.IntercomService/GetStatus",
            base_url.trim_end_matches('/')
        ))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
        .body("{}")
        .send()
        .await
        .map_err(|error| format!("GetStatus request failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("GetStatus response: {error}"))?;
    if status != reqwest::StatusCode::OK {
        return Err(format!("GetStatus returned HTTP {}", status.as_u16()));
    }
    Ok(body)
}

fn client() -> Result<reqwest::Client, String> {
    let _already_installed = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .timeout(TIMEOUT)
        .build()
        .map_err(|error| format!("http client: {error}"))
}
