//! Distroless `ha-bridge -healthcheck` probe.
//!
//! The bridge has no listener, so readiness means "the authenticated scheduler
//! API answers and reports a healthy tick loop". The token is read from its
//! mounted file by the normal config loader and never appears in process
//! arguments or probe events.

use std::process::ExitCode;
use std::time::Duration;

use home_proto::{SchedulerClient, SchedulerError};

use crate::config::Config;

/// A single probe stays below the Kubernetes timeout.
const TIMEOUT: Duration = Duration::from_secs(5);

/// Build the scheduler client, honouring `SCHEDULER_CA_FILE`.
pub fn scheduler_client(
    config: &Config,
    timeout: Duration,
) -> Result<SchedulerClient, SchedulerError> {
    config.scheduler_ca_file.as_ref().map_or_else(
        || {
            SchedulerClient::new(
                &config.scheduler_url,
                config.scheduler_token.clone(),
                timeout,
            )
        },
        |path| {
            SchedulerClient::new_with_ca_file(
                &config.scheduler_url,
                config.scheduler_token.clone(),
                timeout,
                path,
            )
        },
    )
}

/// `ha-bridge -healthcheck`: authenticated `GetStatus` must report ready.
pub async fn run_healthcheck() -> ExitCode {
    let config = match Config::load() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("healthcheck: configuration: {error}");
            return ExitCode::FAILURE;
        }
    };
    let client = match scheduler_client(&config, TIMEOUT) {
        Ok(client) => client,
        Err(error) => {
            eprintln!("healthcheck: scheduler client: {error}");
            return ExitCode::FAILURE;
        }
    };
    match client.get_status().await {
        Ok(status) if status.ready => ExitCode::SUCCESS,
        Ok(_) => {
            eprintln!("healthcheck: scheduler is not ready");
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("healthcheck: scheduler GetStatus: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use axum::{
        Router,
        http::{StatusCode, header},
        routing::post,
    };

    use home_proto::SchedulerClient;
    use home_secret::Token;

    async fn serve(body: &'static str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("addr");
        let app = Router::new().route(
            "/ricardo.v1.SchedulerService/GetStatus",
            post(move || async move {
                (
                    StatusCode::OK,
                    [(header::CONTENT_TYPE, "application/json")],
                    body,
                )
            }),
        );
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        format!("http://{address}")
    }

    #[tokio::test]
    async fn readiness_requires_ready_true() {
        let url = serve(r#"{"ready":true}"#).await;
        let client =
            SchedulerClient::new(&url, Token::new("t"), Duration::from_secs(1)).expect("client");
        assert!(client.get_status().await.expect("status").ready);

        let url = serve(r#"{"ready":false}"#).await;
        let client =
            SchedulerClient::new(&url, Token::new("t"), Duration::from_secs(1)).expect("client");
        assert!(!client.get_status().await.expect("status").ready);
    }
}
