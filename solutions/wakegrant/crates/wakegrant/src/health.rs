//! The `-healthcheck` probe of the distroless image: `GET /healthz` on loopback.

use std::time::Duration;

/// Errors of one probe. No secret or response body is included.
#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    #[error("healthcheck: http client: {0}")]
    Client(#[source] reqwest::Error),
    #[error("healthcheck: request failed: {0}")]
    Request(#[source] reqwest::Error),
    #[error("healthcheck: health returned HTTP {0}")]
    Status(reqwest::StatusCode),
}

/// Calls `<base_url>/healthz` and succeeds only on HTTP 200.
///
/// The workspace builds reqwest with rustls but without a default crypto
/// provider, and reqwest panics ("No provider set") when it builds a client
/// without one, even for plain HTTP. The process-wide ring provider is
/// installed first; a provider installed earlier is kept.
pub async fn probe(base_url: &str) -> Result<(), ProbeError> {
    let _already_installed = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(4))
        .build()
        .map_err(ProbeError::Client)?;
    let response = client
        .get(format!("{base_url}/healthz"))
        .send()
        .await
        .map_err(ProbeError::Request)?;
    if response.status() != reqwest::StatusCode::OK {
        return Err(ProbeError::Status(response.status()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use axum::{Router, http::StatusCode, routing::get};

    use super::{ProbeError, probe};

    async fn serve(status: StatusCode) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("addr");
        let app = Router::new().route("/healthz", get(move || async move { (status, "ok\n") }));
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        format!("http://{address}")
    }

    // Regression: the first Rust deploy panicked here with "No provider set".
    #[tokio::test]
    async fn probe_accepts_a_healthy_server() {
        let url = serve(StatusCode::OK).await;
        probe(&url).await.expect("healthy");
    }

    #[tokio::test]
    async fn probe_rejects_a_non_200_answer() {
        let url = serve(StatusCode::SERVICE_UNAVAILABLE).await;
        let error = probe(&url).await.expect_err("unhealthy");
        assert!(matches!(error, ProbeError::Status(status) if status == 503));
    }
}
