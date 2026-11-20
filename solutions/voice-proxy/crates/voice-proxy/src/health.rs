use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    #[error("healthcheck: http client: {0}")]
    Client(#[source] reqwest::Error),
    #[error("healthcheck: upstream is unreachable: {0}")]
    Request(#[source] reqwest::Error),
    #[error("healthcheck: upstream returned HTTP {0}")]
    Status(u16),
}

pub async fn probe(upstream: &str) -> Result<(), ProbeError> {
    let _already_installed = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(4))
        .build()
        .map_err(ProbeError::Client)?;
    let response = client
        .get(format!("{}/v1/models", upstream.trim_end_matches('/')))
        .send()
        .await
        .map_err(ProbeError::Request)?;
    let status = response.status();
    if status.as_u16() >= 500 {
        return Err(ProbeError::Status(status.as_u16()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::http::StatusCode;
    use axum::routing::get;

    use super::{ProbeError, probe};

    async fn serve(status: StatusCode) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("address");
        let app = Router::new().route(
            "/v1/models",
            get(move || async move { (status, "{\"data\":[]}") }),
        );
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        format!("http://{address}")
    }

    #[tokio::test]
    async fn probe_accepts_a_healthy_and_an_unauthorized_upstream() {
        probe(&serve(StatusCode::OK).await).await.expect("healthy");
        probe(&serve(StatusCode::UNAUTHORIZED).await)
            .await
            .expect("401 is up");
    }

    #[tokio::test]
    async fn probe_rejects_a_server_error() {
        let error = probe(&serve(StatusCode::SERVICE_UNAVAILABLE).await)
            .await
            .expect_err("unhealthy");
        assert!(matches!(error, ProbeError::Status(503)));
    }

    #[tokio::test]
    async fn probe_rejects_an_unreachable_upstream() {
        let error = probe("http://127.0.0.1:1").await.expect_err("unreachable");
        assert!(matches!(error, ProbeError::Request(_)));
    }
}
