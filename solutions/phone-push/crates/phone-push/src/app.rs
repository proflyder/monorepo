//! Composition root: configuration, telemetry, FCM sender, registry, HTTP.

use std::process::ExitCode;
use std::sync::Arc;

use anyhow::Context as _;
use home_runtime::{Supervisor, shutdown_signal};
use home_telemetry::TelemetryBuilder;
use tokio::net::TcpListener;

use crate::api::{self, PushState};
use crate::config::Config;
use crate::devices::Registry;
use crate::fcm::Sender;

/// Reported implementation version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Seconds a terminated request gets before the process exits.
const SHUTDOWN_GRACE: u64 = 5;

/// Starts the service and blocks until SIGTERM/SIGINT or a listener failure.
pub async fn run() -> anyhow::Result<()> {
    let config = Config::load().context("configuration")?;
    TelemetryBuilder::new()
        .with_json()
        .init()
        .context("telemetry")?;

    let sender = Arc::new(
        Sender::new(
            &config.service_account,
            &config.token_url,
            &config.fcm_url,
            config.timeout,
        )
        .map_err(|error| anyhow::anyhow!("FCM sender: {error}"))?,
    );
    let registry = Arc::new(
        Registry::new(
            &config.scheduler_url,
            config.scheduler_token.clone(),
            config.timeout,
        )
        .map_err(anyhow::Error::msg)?,
    );

    let router = api::router(PushState {
        sender,
        registry,
        token: config.token.clone(),
    })
    .merge(crate::health::router());

    let listener = TcpListener::bind(&config.listen)
        .await
        .with_context(|| format!("bind {}", config.listen))?;
    tracing::info!(
        service = "phone-push",
        version = VERSION,
        addr = %config.listen,
        "phone-push listening"
    );

    let supervisor = Supervisor::new();
    let cancel = supervisor.token();
    let server = home_http::serve(listener, router, cancel.clone());
    tokio::pin!(server);
    let serve_result = tokio::select! {
        result = &mut server => result,
        () = shutdown_signal() => {
            cancel.cancel();
            server.await
        }
    };
    cancel.cancel();
    let unfinished = supervisor
        .shutdown(std::time::Duration::from_secs(SHUTDOWN_GRACE))
        .await;
    if !unfinished.is_empty() {
        tracing::warn!(?unfinished, "tasks did not stop before the shutdown grace");
    }
    serve_result.context("serve")?;
    tracing::info!(event = "stopped", "phone-push stopped");
    Ok(())
}

/// `phone-push -healthcheck` exit code.
pub fn healthcheck_exit() -> ExitCode {
    crate::health::run_healthcheck()
}
