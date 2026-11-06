//! Composition root: configuration, telemetry, SQLite store, the HTTP service
//! and the background cleanup task.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use home_runtime::{Supervisor, shutdown_signal};
use home_telemetry::{LevelFilter, TelemetryBuilder};
use tokio::net::TcpListener;

use crate::api::{Options, Service};
use crate::config::{Config, LogLevel};
use crate::domain::{Clock, SystemClock};
use crate::store::Store;

/// Reported implementation version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const CLEANUP_INTERVAL: Duration = Duration::from_secs(300);

/// Starts the service and blocks until SIGTERM/SIGINT or a listener failure.
pub async fn run() -> anyhow::Result<()> {
    let config = Config::load().context("configuration")?;
    TelemetryBuilder::new()
        .with_max_level(level_filter(config.log_level))
        .with_json()
        .init()
        .context("telemetry")?;

    let store = Arc::new(Store::open(&config.db_path).context("open grant store")?);
    let service = Arc::new(Service::new(Options {
        keys: config.machines.clone(),
        issuer_token: home_secret::Token::new(config.issuer_token.expose()),
        store: Arc::clone(&store),
        max_ttl: config.max_ttl,
        clock: Arc::new(SystemClock),
    }));

    let listener = TcpListener::bind(config.listen)
        .await
        .with_context(|| format!("bind {}", config.listen))?;
    let address = listener.local_addr().context("listen address")?;
    tracing::info!(
        service = "wakegrant",
        version = VERSION,
        listen = %address,
        db_path = %config.db_path.display(),
        machines = config.machines.len(),
        max_ttl = %config.max_ttl.as_secs(),
        "wakegrant listening"
    );

    let supervisor = Supervisor::new();
    let cancel = supervisor.token();
    supervisor.spawn("cleanup", cleanup(Arc::clone(&store), cancel.clone()));

    let server = home_http::serve_with_connect_info(listener, service.router(), cancel.clone());
    tokio::pin!(server);
    tokio::select! {
        result = &mut server => {
            result.context("serve")?;
        }
        () = shutdown_signal() => {
            cancel.cancel();
            server.await.context("serve")?;
        }
    }
    let unfinished = supervisor.shutdown(Duration::from_secs(5)).await;
    if !unfinished.is_empty() {
        tracing::warn!(
            ?unfinished,
            "tasks did not stop before the shutdown timeout"
        );
    }
    tracing::info!(event = "stopped", "wakegrant stopped");
    Ok(())
}

fn level_filter(level: LogLevel) -> LevelFilter {
    match level {
        LogLevel::Debug => LevelFilter::DEBUG,
        LogLevel::Info => LevelFilter::INFO,
        LogLevel::Warn => LevelFilter::WARN,
        LogLevel::Error => LevelFilter::ERROR,
    }
}

/// Removes consumed and expired grants. The Go service left them forever; a
/// periodic delete keeps the PVC bounded without changing the wire contract.
async fn cleanup(store: Arc<Store>, cancel: tokio_util::sync::CancellationToken) {
    let mut interval = tokio::time::interval(CLEANUP_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            () = cancel.cancelled() => return,
            _ = interval.tick() => {
                match store.cleanup(SystemClock.now()).await {
                    Ok(0) => {}
                    Ok(removed) => tracing::debug!(removed, "removed old grants"),
                    Err(error) => tracing::warn!(%error, "grant cleanup failed"),
                }
            }
        }
    }
}
