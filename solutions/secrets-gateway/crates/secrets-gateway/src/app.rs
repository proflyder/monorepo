use std::process::ExitCode;
use std::sync::Arc;

use anyhow::Context as _;
use home_app_auth::{AccessClient, snapshot_url};
use home_runtime::{Supervisor, shutdown_signal};
use home_telemetry::TelemetryBuilder;
use tokio::net::TcpListener;

use crate::api::{self, GatewayState};
use crate::audit::{Auditor, PushClient};
use crate::catalog::Catalog;
use crate::config::Config;
use crate::openbao::OpenBao;
use crate::stepup::StepUp;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const SHUTDOWN_GRACE: u64 = 5;
const BODY_LIMIT: usize = 128 * 1024;

pub async fn run() -> anyhow::Result<()> {
    let config = Config::load().context("configuration")?;
    TelemetryBuilder::new()
        .with_json()
        .init()
        .context("telemetry")?;

    let catalog = Arc::new(Catalog::embedded().map_err(anyhow::Error::msg)?);
    let access = AccessClient::new(
        snapshot_url(&config.scheduler_url),
        config.service_token.expose(),
    )
    .map_err(|error| anyhow::anyhow!("access client: {error}"))?;
    let stepup = Arc::new(
        StepUp::new(config.totp_secret.expose())
            .map_err(|error| anyhow::anyhow!("step-up: {error}"))?,
    );
    let openbao = Arc::new(
        OpenBao::new(
            &config.openbao_url,
            &config.openbao_role,
            config.sa_token.clone(),
            config.timeout,
        )
        .map_err(anyhow::Error::msg)?,
    );
    let push = PushClient::new(
        &config.phone_push_url,
        config.phone_push_token.clone(),
        config.timeout,
    )
    .map_err(anyhow::Error::msg)?;
    let audit = Arc::new(Auditor::new(push));

    let router = api::router(GatewayState {
        access: Arc::clone(&access),
        openbao,
        catalog,
        stepup,
        audit,
        max_list_entries: config.max_list_entries,
    })
    .merge(crate::health::router());
    let router = home_http::with_body_limit(router, BODY_LIMIT);

    let listener = TcpListener::bind(&config.listen)
        .await
        .with_context(|| format!("bind {}", config.listen))?;
    tracing::info!(
        service = "secrets-gateway",
        version = VERSION,
        addr = %config.listen,
        "secrets-gateway listening"
    );

    let supervisor = Supervisor::new();
    let cancel = supervisor.token();
    supervisor.spawn("access", Arc::clone(&access).run(cancel.clone()));
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
    tracing::info!(event = "stopped", "secrets-gateway stopped");
    Ok(())
}

pub async fn healthcheck_exit() -> ExitCode {
    crate::health::run_healthcheck().await
}
