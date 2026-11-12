//! Composition root: configuration, the agent and Home Assistant clients, the
//! Prometheus listener and the periodic sync loop.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use axum::Router;
use axum::extract::State;
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::get;
use tokio::net::TcpListener;

use crate::agent::SshAgent;

use crate::ha::Ha;
use crate::manifest::Plan;
use crate::metrics::Metrics;
use crate::protocol::load_desired;
use crate::sync::Syncer;

/// Start the service and block until a signal or a fatal listener error.
pub async fn run() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .json()
        .with_max_level(tracing::Level::INFO)
        .with_target(true)
        .init();

    let config = crate::config::from_env().context("configuration")?;
    let desired = load_desired(&config.desired_dir)
        .with_context(|| format!("load desired files from {}", config.desired_dir.display()))?;
    let plan = Plan::load(&config.manifest_file, &desired)
        .with_context(|| format!("load manifest {}", config.manifest_file.display()))?;

    let metrics = Arc::new(Metrics::new(config.mode));
    let agent = SshAgent::new(&config.ssh);
    let ha =
        Ha::new(config.ha_url.clone(), config.ha_token.clone()).context("Home Assistant client")?;
    let mut syncer = Syncer::new(
        agent,
        ha,
        desired,
        plan,
        Arc::clone(&metrics),
        config.mode,
        config.restart_guard_entity.clone(),
    );

    let listener = TcpListener::bind(&config.metrics_addr)
        .await
        .with_context(|| format!("bind {}", config.metrics_addr))?;
    let metrics_addr = listener.local_addr().context("metrics address")?;
    tokio::spawn(async move {
        if let Err(error) = serve_metrics(listener, metrics).await {
            tracing::error!(error = %error, "metrics listener stopped");
        }
    });

    tracing::info!(
        version = crate::VERSION,
        mode = config.mode.as_str(),
        interval_seconds = config.sync_interval.as_secs(),
        metrics_addr = %metrics_addr,
        "ha-config-sync started"
    );

    let mut interval = tokio::time::interval(config.sync_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = interval.tick() => {
                let report = syncer.cycle().await;
                tracing::info!(
                    outcome = ?report.outcome,
                    drift = report.drift,
                    "sync cycle finished"
                );
            }
            _ = shutdown_signal() => {
                tracing::info!("shutdown requested");
                break;
            }
        }
    }
    Ok(())
}

async fn serve_metrics(listener: TcpListener, metrics: Arc<Metrics>) -> anyhow::Result<()> {
    let app = Router::new()
        .route(
            "/metrics",
            get(|State(metrics): State<Arc<Metrics>>| async move {
                (
                    [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
                    metrics.render(),
                )
                    .into_response()
            }),
        )
        .route("/healthz", get(|| async { "ok" }))
        .with_state(metrics);
    axum::serve(listener, app).await.context("serve metrics")
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
}

/// Default period, re-exported for documentation tests.
pub const DEFAULT_INTERVAL: Duration = crate::config::DEFAULT_SYNC_INTERVAL;

/// Re-export for callers that only need the mode label.
pub use crate::config::Mode as SyncMode;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Mode;

    #[test]
    fn the_default_mode_is_apply() {
        assert_eq!(Mode::Apply.as_str(), "apply");
        assert_eq!(SyncMode::Observe.as_str(), "observe");
        assert_eq!(DEFAULT_INTERVAL.as_secs(), 300);
    }
}
