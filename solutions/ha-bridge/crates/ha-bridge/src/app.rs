//! Composition root: configuration, telemetry, the executor worker and the
//! distroless probe entry points.
//!
//! Default mode is dry-run: [`run`] describes the configuration, reads the
//! scheduler status and optionally probes Home Assistant, then exits without
//! leasing, executing or acknowledging anything. `BRIDGE_EXECUTE=true` starts
//! the long-lived worker.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use home_runtime::{Supervisor, shutdown_signal};
use home_telemetry::TelemetryBuilder;
use home_worker::{LeaseRequest, SchedulerQueue, Worker, WorkerConfig};
use tracing::{info, warn};

use crate::bridge::{Bridge, HaClient};
use crate::config::Config;
use crate::health::scheduler_client;
use crate::ledger::Ledger;

/// Reported implementation version, matching the crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

/// Start the bridge and block until SIGTERM/SIGINT.
pub async fn run() -> anyhow::Result<()> {
    // Capture the local offset before the multi-thread runtime starts.
    let offset = home_telemetry::local_offset();
    let config = Config::load().context("configuration")?;
    TelemetryBuilder::new()
        .with_local_offset(offset)
        .with_json()
        .init()
        .context("telemetry")?;

    let config = Arc::new(config);
    if config.execute {
        execute(Arc::clone(&config)).await
    } else {
        describe(&config).await
    }
}

async fn execute(config: Arc<Config>) -> anyhow::Result<()> {
    log_configuration(&config);
    info!(queue = %config.queue, "executor starting");

    let scheduler =
        scheduler_client(&config, config.scheduler_timeout).context("scheduler client")?;
    let ha = build_ha(&config)?;
    let ledger = open_ledger(&config)?;

    let bridge = Bridge::new(Arc::clone(&config), ha, Arc::new(ledger));
    let worker = Worker::new(
        SchedulerQueue::new(scheduler),
        bridge,
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

    let supervisor = Supervisor::new();
    let cancel = supervisor.token();
    let worker_cancel = cancel.clone();
    supervisor.spawn("executor", async move {
        worker.run(worker_cancel).await;
    });

    shutdown_signal().await;
    info!(queue = %config.queue, "shutdown requested");
    let unfinished = supervisor.shutdown(SHUTDOWN_TIMEOUT).await;
    if !unfinished.is_empty() {
        warn!(
            ?unfinished,
            "tasks did not stop before the shutdown timeout"
        );
    }
    info!("ha-bridge stopped");
    Ok(())
}

async fn describe(config: &Config) -> anyhow::Result<()> {
    log_configuration(config);
    info!("dry-run: no events will be leased, executed or acknowledged");

    let client = scheduler_client(config, config.scheduler_timeout).context("scheduler client")?;
    let status = client.get_status().await.context("read scheduler status")?;
    info!(
        version = %status.version,
        schema_version = status.schema_version,
        pending_events = status.pending_events,
        leased_events = status.leased_events,
        enabled_schedules = status.enabled_schedules,
        running_timers = status.running_timers,
        scenario_execution_supported = status.scenario_execution_supported,
        "scheduler status"
    );
    for queue in &status.queues {
        info!(
            name = %queue.name,
            pending = queue.pending,
            leased = queue.leased,
            "scheduler queue"
        );
    }
    for consumer in &status.consumers {
        info!(
            consumer_id = %consumer.consumer_id,
            alive = consumer.alive,
            last_seen_unix = consumer.last_seen_unix,
            last_ack_unix = consumer.last_ack_unix,
            "scheduler consumer"
        );
    }

    if config.ha_token.expose().is_empty() {
        warn!("HA token not configured; skipping Home Assistant reachability probe");
    } else {
        let ha = build_ha(config)?;
        ha.ping().await.context("home assistant reachability")?;
        info!(
            alarm_script = %config.alarm_script,
            announce_script = %config.announce_script,
            stop_script = %config.stop_script,
            "home assistant reachable"
        );
    }

    info!("dry-run finished: no scheduler events were leased or acknowledged");
    Ok(())
}

fn build_ha(config: &Config) -> anyhow::Result<Arc<home_ha::Client>> {
    let token = config.ha_token.clone();
    let client = match &config.ha_ca_file {
        Some(path) => {
            home_ha::Client::new_with_ca_file(&config.ha_url, token, config.ha_timeout, path)
        }
        None => home_ha::Client::new(&config.ha_url, token, config.ha_timeout),
    };
    Ok(Arc::new(client.context("Home Assistant client")?))
}

fn open_ledger(config: &Config) -> anyhow::Result<Ledger> {
    config.ledger_path.as_ref().map_or_else(
        || {
            warn!("BRIDGE_LEDGER_PATH is unset; duplicate suppression lasts only for this process");
            Ok(Ledger::memory())
        },
        |path| Ledger::open(path).context("open receipt ledger"),
    )
}

fn log_configuration(config: &Config) {
    let ledger = config.ledger_path.as_ref().map_or_else(
        || "memory only".to_owned(),
        |path| format!("file ({})", path.display()),
    );
    info!(
        mode = if config.execute { "execute" } else { "dry-run" },
        consumer_id = %config.consumer_id,
        queue = %config.queue,
        scheduler_endpoint = %config.scheduler_url,
        ha_url = %config.ha_url,
        batch_size = config.batch_size,
        lease_seconds = config.lease_seconds,
        poll_interval = ?config.poll_interval,
        max_backoff = ?config.max_backoff,
        alarm_script = %config.alarm_script,
        announce_script = %config.announce_script,
        stop_script = %config.stop_script,
        ledger = %ledger,
        scheduler_token = presence(&config.scheduler_token),
        ha_token = presence(&config.ha_token),
        "ha-bridge configuration"
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
