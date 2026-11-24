use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use home_app_auth::{AccessClient, snapshot_url};
use home_runtime::{Supervisor, shutdown_signal};
use home_telemetry::TelemetryBuilder;
use tokio::net::TcpListener;

use crate::api::{self, GatewayState};
use crate::audit::{Auditor, PushClient};
use crate::catalog::Catalog;
use crate::config::Config;
use crate::deny::Deny;
use crate::device::DeviceStore;
use crate::openbao::OpenBao;
use crate::stepup::StepUp;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const SHUTDOWN_GRACE: u64 = 5;
const BODY_LIMIT: usize = 128 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

pub async fn run() -> anyhow::Result<()> {
    let config = Config::load().context("configuration")?;
    TelemetryBuilder::new()
        .with_json()
        .init()
        .context("telemetry")?;

    let catalog =
        Arc::new(Catalog::load(Path::new(&config.catalog_file)).map_err(anyhow::Error::msg)?);
    let deny = Arc::new(Deny::load(Path::new(&config.deny_file)).map_err(anyhow::Error::msg)?);
    let access = AccessClient::new(
        snapshot_url(&config.scheduler_url),
        config.service_token.expose(),
    )
    .map_err(|error| anyhow::anyhow!("access client: {error}"))?;
    let openbao = Arc::new(
        OpenBao::new(
            &config.openbao_url,
            &config.openbao_role,
            &config.sa_token_file,
            config.timeout,
        )
        .map_err(anyhow::Error::msg)?,
    );
    let device = Arc::new(DeviceStore::new(Arc::clone(&openbao), &config.device_path));
    let stepup = Arc::new(
        StepUp::with_state(Arc::clone(&device), Path::new(&config.state_file))
            .map_err(|error| anyhow::anyhow!("step-up: {error}"))?,
    );
    let push = PushClient::new(
        &config.phone_push_url,
        config.phone_push_token.clone(),
        config.timeout,
    )
    .map_err(anyhow::Error::msg)?;
    let audit = Arc::new(Auditor::new(push));

    let state = GatewayState {
        access: Arc::clone(&access),
        openbao,
        catalog,
        deny,
        device,
        stepup,
        audit,
        max_list_entries: config.max_list_entries,
    };
    let router = api::router(state.clone()).merge(crate::health::router(state));
    let router = home_http::with_timeout(
        home_http::with_body_limit(router, BODY_LIMIT),
        REQUEST_TIMEOUT,
    );

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
        .shutdown(Duration::from_secs(SHUTDOWN_GRACE))
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
