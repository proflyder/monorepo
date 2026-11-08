//! Composition root: configuration, telemetry, SQLite store, engine and the
//! authenticated Connect API.
//!
//! Shutdown is ordered: the listener stops accepting and drains in-flight
//! requests, then the engine task is cancelled and joined, then the SQLite
//! writer thread is closed.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use home_runtime::{Supervisor, shutdown_signal};
use home_telemetry::TelemetryBuilder;
use tokio::net::TcpListener;

use crate::api::{self, AlarmFacade, Service};
use crate::config::Config;
use crate::domain::SystemClock;
use crate::engine::{DEFAULT_PURGE_INTERVAL, Engine};
use crate::store::{Config as StoreConfig, Store};

/// Reported implementation version (`GetStatus.version`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

/// Starts the service and blocks until SIGTERM/SIGINT or a listener failure.
pub async fn run() -> anyhow::Result<()> {
    let config = Config::load().context("configuration")?;
    TelemetryBuilder::new()
        .with_json()
        .init()
        .context("telemetry")?;

    let clock = Arc::new(SystemClock);
    let store = Arc::new(
        Store::open(&config.db_path, store_config(&config)).context("open scheduler store")?,
    );
    let engine = Engine::new(clock.clone(), config.tick_interval, DEFAULT_PURGE_INTERVAL);
    let status_engine = engine.clone();
    let service = Service::new(
        Arc::clone(&store),
        clock,
        VERSION,
        Arc::new(move || status_engine.status()),
        config.allowed_consumers.clone(),
    )
    .with_scenario_execution(config.scenario_execution);
    let router = api::router(
        service.clone(),
        AlarmFacade::new(service),
        config.token.expose(),
    );

    let listener = TcpListener::bind(&config.listen)
        .await
        .with_context(|| format!("bind {}", config.listen))?;
    let address = listener.local_addr().context("listen address")?;
    tracing::info!(
        service = "scheduler",
        version = VERSION,
        addr = %address,
        db = %config.db_path.display(),
        tick_ms = config.tick_interval.as_millis(),
        grace_seconds = config.missed_grace.as_secs(),
        ring_window_seconds = config.ring_window.as_secs(),
        consumers_restricted = !config.allowed_consumers.is_empty(),
        scenario_execution = config.scenario_execution,
        "scheduler listening"
    );

    let supervisor = Supervisor::new();
    let cancel = supervisor.token();
    let engine_store = Arc::clone(&store);
    let engine_cancel = cancel.clone();
    let engine_task = engine.clone();
    supervisor.spawn("engine", async move {
        engine_task.run(engine_store, engine_cancel).await;
    });

    let server = home_http::serve(listener, router, cancel.clone());
    tokio::pin!(server);
    let serve_result = tokio::select! {
        result = &mut server => result,
        () = shutdown_signal() => {
            cancel.cancel();
            server.await
        }
    };
    // The listener is gone; stop the engine and close the SQLite writer before
    // propagating a serving failure.
    cancel.cancel();
    let unfinished = supervisor.shutdown(SHUTDOWN_TIMEOUT).await;
    if !unfinished.is_empty() {
        tracing::warn!(
            ?unfinished,
            "tasks did not stop before the shutdown timeout"
        );
    }

    match Arc::try_unwrap(store) {
        Ok(store) => store.close().context("close scheduler store")?,
        Err(_still_shared) => {
            tracing::warn!("store still shared at shutdown; dropping it");
        }
    }
    tracing::info!(event = "stopped", "scheduler stopped");
    serve_result.context("serve")?;
    Ok(())
}

fn store_config(config: &Config) -> StoreConfig {
    StoreConfig {
        default_timezone: config.timezone.clone(),
        missed_grace: seconds(config.missed_grace),
        ring_window: seconds(config.ring_window),
        max_timers: config.max_timers,
        max_timer_duration: seconds(config.max_timer_duration),
        max_payload_bytes: config.max_payload_bytes,
        max_attempts: config.max_attempts,
        default_snooze_minutes: 9,
        event_retention: seconds(config.event_retention),
        consumer_liveness: 90,
        max_lease: 60 * 60,
    }
}

fn seconds(duration: Duration) -> i64 {
    i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_config_matches_the_go_defaults() {
        let config = Config::load_from(
            &|key| match key {
                "SCHEDULER_TOKEN" => Some("t".to_owned()),
                "SCHEDULER_TIMEZONE" => Some("Asia/Almaty".to_owned()),
                _ => None,
            },
            &|_path| Ok(Vec::new()),
        )
        .expect("config");
        let store = store_config(&config);
        assert_eq!(store.default_timezone, "Asia/Almaty");
        assert_eq!(store.missed_grace, 600);
        assert_eq!(store.ring_window, 900);
        assert_eq!(store.max_timers, 64);
        assert_eq!(store.max_payload_bytes, 16 * 1024);
        assert_eq!(store.max_attempts, 5);
        assert_eq!(store.event_retention, 30 * 24 * 60 * 60);
        assert_eq!(store.consumer_liveness, 90);
        assert_eq!(store.max_lease, 60 * 60);
        assert_eq!(store.default_snooze_minutes, 9);
    }
}
