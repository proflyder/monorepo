//! Distroless probes: `-healthcheck` and `-status`.
//!
//! The image has no shell, curl or wget, so the binary is its own probe. Both
//! commands load the same configuration as the server (a broken Secret mount
//! fails the probe), read the bearer from `SCHEDULER_TOKEN_FILE` and never put
//! the token on a command line or in an error message.

use std::process::ExitCode;
use std::time::Duration;

use crate::config::Config;

/// Bounds a single probe. Kubernetes gives the probe a slightly larger
/// timeout; keeping the client lower avoids overlapping calls.
const TIMEOUT: Duration = Duration::from_secs(4);

const GET_STATUS_PATH: &str = "/home.v1.SchedulerService/GetStatus";

/// `scheduler -healthcheck`: authenticated `GetStatus` must report ready.
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
    if let Err(error) = check_ready(&format!("http://{address}"), config.token.expose()).await {
        eprintln!("healthcheck: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// `scheduler -status`: prints the raw authenticated `GetStatus` JSON body.
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
    match get_status_body(&format!("http://{address}"), config.token.expose()).await {
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

/// Performs the authenticated `GetStatus` call and returns the body.
pub async fn get_status_body(base_url: &str, token: &str) -> Result<String, String> {
    let client = client()?;
    let response = client
        .post(format!(
            "{}{GET_STATUS_PATH}",
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

/// Evaluates readiness from an authenticated `GetStatus` response.
pub async fn check_ready(base_url: &str, token: &str) -> Result<(), String> {
    let body = get_status_body(base_url, token).await?;
    let status: serde_json::Value = serde_json::from_str(&body)
        .map_err(|error| format!("GetStatus response is not JSON: {error}"))?;
    if status.get("ready").and_then(serde_json::Value::as_bool) != Some(true) {
        let reason = status
            .get("lastTickError")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
            .unwrap_or("tick not confirmed");
        return Err(format!("scheduler is not ready: {reason}"));
    }
    Ok(())
}

fn client() -> Result<reqwest::Client, String> {
    // The workspace builds reqwest with rustls but without a default crypto
    // provider; building a client without one panics. Install the process-wide
    // ring provider first, keeping one installed earlier.
    let _already_installed = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .timeout(TIMEOUT)
        .build()
        .map_err(|error| format!("http client: {error}"))
}

#[cfg(test)]
mod tests {
    use axum::{
        Router,
        http::StatusCode,
        routing::{any, post},
    };

    use super::{check_ready, get_status_body};

    async fn serve(status: StatusCode, body: &'static str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("addr");
        let app = Router::new().route(
            "/home.v1.SchedulerService/GetStatus",
            post(move || async move { (status, body) }),
        );
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        format!("http://{address}")
    }

    #[tokio::test]
    async fn status_returns_the_raw_body() {
        let url = serve(StatusCode::OK, r#"{"ready":true,"queues":[]}"#).await;
        let body = get_status_body(&url, "probe-token").await.expect("status");
        assert_eq!(body, r#"{"ready":true,"queues":[]}"#);
    }

    #[tokio::test]
    async fn readiness_requires_ready_true() {
        let healthy = serve(StatusCode::OK, r#"{"ready":true}"#).await;
        check_ready(&healthy, "t").await.expect("ready");

        let unhealthy = serve(
            StatusCode::OK,
            r#"{"ready":false,"lastTickError":"tick failed"}"#,
        )
        .await;
        let error = check_ready(&unhealthy, "t").await.expect_err("not ready");
        assert!(error.contains("tick failed"));
    }

    #[tokio::test]
    async fn non_200_is_an_error_without_the_token() {
        let url = serve(StatusCode::UNAUTHORIZED, "").await;
        let error = get_status_body(&url, "secret-token")
            .await
            .expect_err("unauthorized");
        assert!(error.contains("HTTP 401"));
        assert!(!error.contains("secret-token"));
    }

    #[tokio::test]
    async fn unknown_path_returns_404() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("addr");
        let app = Router::new().route("/other", any(|| async { "ok" }));
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        let error = get_status_body(&format!("http://{address}"), "t")
            .await
            .expect_err("404");
        assert!(error.contains("HTTP 404"));
    }
}
