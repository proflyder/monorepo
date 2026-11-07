//! Distroless probe: `mcp-gateway -healthcheck`.
//!
//! The image has no shell, curl or wget, so the binary is its own probe. It
//! loads the same configuration as the server (a broken Secret mount fails the
//! probe) and asks the loopback `/healthz`.

use std::process::ExitCode;
use std::time::Duration;

use crate::config::Config;

const TIMEOUT: Duration = Duration::from_secs(4);

/// `mcp-gateway -healthcheck`: a `200 /healthz` is success.
pub fn run_healthcheck() -> ExitCode {
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
    if let Err(error) = check(&base_url) {
        eprintln!("healthcheck: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn check(base_url: &str) -> Result<(), String> {
    let _already_installed = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .build()
        .map_err(|error| format!("http client: {error}"))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("runtime: {error}"))?;
    runtime.block_on(async move {
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
    })
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::routing::get;

    use super::check;

    #[tokio::test]
    async fn healthz_is_checked() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("addr");
        let app = Router::new().route("/healthz", get(|| async { "{\"status\":\"ok\"}" }));
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });

        // `check` builds its own runtime, so run it on a blocking thread.
        let url = format!("http://{address}");
        tokio::task::spawn_blocking(move || check(&url))
            .await
            .expect("join")
            .expect("healthy");
    }
}
