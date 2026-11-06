//! Composition root of `ha-control`: catalog server and control-queue worker.
//!
//! `CONTROL_EXECUTE=false` (the default, and the manifest value) serves the
//! catalog only; nothing is leased or acknowledged. `CONTROL_EXECUTE=true`
//! starts the executor on the dedicated `control` queue. Both modes share one
//! discovery provider, so the catalog the phone reads is the catalog the
//! executor validates against.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use home_runtime::{Supervisor, shutdown_signal};
use home_telemetry::TelemetryBuilder;
use home_worker::{LeaseRequest, SchedulerQueue, Worker, WorkerConfig};
use tokio::net::TcpListener;
use tracing::{info, warn};

use crate::catalog_api;
use crate::control::Control;
use crate::control_config::ControlConfig;
use crate::discovery::{CatalogSource, Provider};

/// Reported implementation version, matching the crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);
/// Bound for one catalog request. The Go server also bounded header read (5 s),
/// body read (10 s), write (30 s) and idle keep-alive (120 s); axum applies one
/// request deadline here, which covers the handler (discovery + write).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Start `ha-control` and block until SIGTERM/SIGINT.
pub async fn run() -> anyhow::Result<()> {
    let offset = home_telemetry::local_offset();
    let config = ControlConfig::load().context("configuration")?;
    TelemetryBuilder::new()
        .with_local_offset(offset)
        .with_json()
        .init()
        .context("telemetry")?;

    let config = Arc::new(config);
    log_configuration(&config);

    let ha = build_ha(&config)?;
    let provider = Arc::new(Provider::new(
        Arc::clone(&ha),
        config.rules(),
        config.cache_ttl,
    ));
    let source: Arc<dyn CatalogSource> = provider;
    let router = home_http::with_timeout(
        catalog_api::router(Arc::clone(&source), config.control_token.clone()),
        REQUEST_TIMEOUT,
    );

    let listener = TcpListener::bind(&config.listen)
        .await
        .with_context(|| format!("bind {}", config.listen))?;
    let address = listener.local_addr().context("listen address")?;
    info!(
        addr = %address,
        scenario_label = %config.scenario_label,
        profile_label = %config.profile_label,
        exclude = ?config.exclude,
        execute = config.execute,
        "catalog listening"
    );

    let supervisor = Supervisor::new();
    let cancel = supervisor.token();

    if config.execute {
        let scheduler = scheduler_client(&config).context("scheduler client")?;
        let control = Control::new(Arc::clone(&config), Arc::clone(&source), Arc::clone(&ha));
        let worker = Worker::new(
            SchedulerQueue::new(scheduler),
            control,
            WorkerConfig {
                lease: LeaseRequest {
                    consumer_id: config.consumer_id.clone(),
                    queue: config.queue.clone(),
                    lease_seconds: config.lease_seconds,
                    limit: config.batch_size,
                },
                poll_interval: config.poll_interval,
                retry_min: config.poll_interval,
                retry_max: config.max_backoff,
            },
        );
        let worker_cancel = cancel.clone();
        supervisor.spawn("control-executor", async move {
            worker.run(worker_cancel).await;
        });
        info!(queue = %config.queue, "control executor starting");
    } else {
        warn!("CONTROL_EXECUTE is not true: serving the catalog only; commands are not executed");
    }

    let server = home_http::serve(listener, router, cancel.clone());
    tokio::pin!(server);
    let serve_result = tokio::select! {
        result = &mut server => result,
        () = shutdown_signal() => {
            info!("shutdown requested");
            cancel.cancel();
            server.await
        }
    };

    cancel.cancel();
    let unfinished = supervisor.shutdown(SHUTDOWN_TIMEOUT).await;
    if !unfinished.is_empty() {
        warn!(
            ?unfinished,
            "tasks did not stop before the shutdown timeout"
        );
    }
    serve_result.context("serve catalog")?;
    info!("ha-control stopped");
    Ok(())
}

fn build_ha(config: &ControlConfig) -> anyhow::Result<Arc<home_ha::Client>> {
    let token = config.ha_token.clone();
    let client = match &config.ha_ca_file {
        Some(path) => {
            home_ha::Client::new_with_ca_file(&config.ha_url, token, config.ha_timeout, path)
        }
        None => home_ha::Client::new(&config.ha_url, token, config.ha_timeout),
    };
    Ok(Arc::new(client.context("Home Assistant client")?))
}

fn scheduler_client(
    config: &ControlConfig,
) -> Result<home_proto::SchedulerClient, home_proto::SchedulerError> {
    config.scheduler_ca_file.as_ref().map_or_else(
        || {
            home_proto::SchedulerClient::new(
                &config.scheduler_url,
                config.scheduler_token.clone(),
                config.scheduler_timeout,
            )
        },
        |path| {
            home_proto::SchedulerClient::new_with_ca_file(
                &config.scheduler_url,
                config.scheduler_token.clone(),
                config.scheduler_timeout,
                path,
            )
        },
    )
}

fn log_configuration(config: &ControlConfig) {
    info!(
        mode = if config.execute { "execute" } else { "catalog-only" },
        consumer_id = %config.consumer_id,
        queue = %config.queue,
        listen = %config.listen,
        scheduler_endpoint = %config.scheduler_url,
        ha_url = %config.ha_url,
        cache_ttl = ?config.cache_ttl,
        poll_interval = ?config.poll_interval,
        max_backoff = ?config.max_backoff,
        batch_size = config.batch_size,
        lease_seconds = config.lease_seconds,
        max_attempts = config.max_attempts,
        control_token = presence(&config.control_token),
        scheduler_token = presence(&config.scheduler_token),
        ha_token = presence(&config.ha_token),
        "ha-control configuration"
    );
}

fn presence(token: &home_secret::Token) -> &'static str {
    if token.expose().is_empty() {
        "unset"
    } else {
        "set"
    }
}

#[cfg(test)]
mod tests {
    use super::VERSION;

    #[test]
    fn version_is_reported() {
        assert!(!VERSION.is_empty());
    }
}
