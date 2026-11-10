//! Composition root of `ha-scenario`: the definition reader, the control
//! catalog and the `scenario` queue executor.
//!
//! `SCENARIO_EXECUTE=false` (the default) loads configuration and validates the
//! definition/token wiring but leases nothing. `true` starts the durable loop.
//! A FIRE runs in its own task and acknowledges itself, so a STOP in a later
//! lease can cancel a long delay immediately; the executor registry guarantees
//! one instance per scenario.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use home_runtime::{Supervisor, shutdown_signal};
use home_telemetry::TelemetryBuilder;
use home_worker::{
    Ack, AckOutcome as QueueOutcome, Action, Event, LeaseRequest, Outcome, Queue, SchedulerQueue,
};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::discovery::Provider;
use crate::scenario::executor::{Executor, ScenarioSource};
use crate::scenario::ledger::ScenarioLedger;
use crate::scenario_config::ScenarioConfig;

/// Reported implementation version, matching the crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

/// Start `ha-scenario` and block until SIGTERM/SIGINT.
pub async fn run() -> anyhow::Result<()> {
    let offset = home_telemetry::local_offset();
    let config = ScenarioConfig::load().context("configuration")?;
    TelemetryBuilder::new()
        .with_local_offset(offset)
        .with_json()
        .init()
        .context("telemetry")?;

    let config = Arc::new(config);
    log_configuration(&config);

    let ha = build_ha(&config)?;
    let catalog = Arc::new(Provider::new(
        Arc::clone(&ha),
        config.rules(),
        Vec::new(),
        config.catalog_ttl,
    ));
    let scenarios: Arc<dyn ScenarioSource> = Arc::new(scenario_client(&config)?);
    let ledger = Arc::new(match &config.ledger_path {
        Some(path) => ScenarioLedger::open(path).context("open scenario ledger")?,
        None => ScenarioLedger::memory(),
    });
    let executor = Arc::new(Executor::new(
        Arc::clone(&config),
        scenarios,
        catalog,
        Arc::clone(&ha),
        ledger,
    ));

    let supervisor = Supervisor::new();
    let cancel = supervisor.token();

    if config.execute {
        let queue = SchedulerQueue::new(scheduler_client(&config)?);
        let worker_config = Arc::clone(&config);
        let worker_executor = Arc::clone(&executor);
        let worker_supervisor = supervisor.clone();
        let worker_cancel = cancel.clone();
        supervisor.spawn("scenario-executor", async move {
            run_worker(
                worker_executor,
                queue,
                worker_config,
                worker_supervisor,
                worker_cancel,
            )
            .await;
        });
        info!(queue = %config.queue, "scenario executor starting");
    } else {
        warn!("SCENARIO_EXECUTE is not true: scenario events are not executed");
    }

    tokio::select! {
        () = shutdown_signal() => info!("shutdown requested"),
        () = cancel.cancelled() => {}
    }
    // Interrupt delays and stop tasks before the supervisor waits for them.
    executor.cancel_all();
    let unfinished = supervisor.shutdown(SHUTDOWN_TIMEOUT).await;
    if !unfinished.is_empty() {
        warn!(
            ?unfinished,
            "tasks did not stop before the shutdown timeout"
        );
    }
    info!("ha-scenario stopped");
    Ok(())
}

async fn run_worker(
    executor: Arc<Executor<home_ha::Client>>,
    queue: SchedulerQueue,
    config: Arc<ScenarioConfig>,
    supervisor: Supervisor,
    cancel: CancellationToken,
) {
    let mut backoff = config.poll_interval;
    loop {
        if cancel.is_cancelled() {
            return;
        }
        let request = LeaseRequest {
            consumer_id: config.consumer_id.clone(),
            queue: config.queue.clone(),
            lease_seconds: config.lease_seconds,
            limit: config.batch_size,
        };
        let leased = tokio::select! {
            result = queue.lease(request) => result,
            () = cancel.cancelled() => return,
        };
        let events = match leased {
            Ok(events) => {
                backoff = config.poll_interval;
                events
            }
            Err(error) => {
                warn!(%error, backoff = ?backoff, "scenario lease failed; backing off");
                if !sleep(backoff, &cancel).await {
                    return;
                }
                backoff = std::cmp::min(backoff.saturating_mul(2), config.max_backoff);
                continue;
            }
        };
        if events.is_empty() {
            if !sleep(config.poll_interval, &cancel).await {
                return;
            }
            continue;
        }
        for event in events {
            if cancel.is_cancelled() {
                return;
            }
            if event.action == Action::Fire {
                spawn_fire(&executor, &queue, &config, &supervisor, event);
                continue;
            }
            let outcome = executor.handle(&event).await;
            ack(&queue, &config, &event, outcome).await;
        }
    }
}

fn spawn_fire(
    executor: &Arc<Executor<home_ha::Client>>,
    queue: &SchedulerQueue,
    config: &Arc<ScenarioConfig>,
    supervisor: &Supervisor,
    event: Event,
) {
    let executor = Arc::clone(executor);
    let queue = queue.clone();
    let consumer_id = config.consumer_id.clone();
    supervisor.spawn("scenario-run", async move {
        let outcome = executor.handle(&event).await;
        ack_with(&queue, &consumer_id, &event, outcome).await;
    });
}

async fn ack(queue: &SchedulerQueue, config: &ScenarioConfig, event: &Event, outcome: Outcome) {
    ack_with(queue, &config.consumer_id, event, outcome).await;
}

async fn ack_with(queue: &SchedulerQueue, consumer_id: &str, event: &Event, outcome: Outcome) {
    let (outcome, detail) = match outcome {
        Outcome::Delivered(detail) => (QueueOutcome::Delivered, detail),
        Outcome::Failed(detail) => (QueueOutcome::Failed, detail),
        Outcome::Skipped(detail) => (QueueOutcome::Skipped, detail),
        // No ack: the lease expires and the scheduler returns the event.
        Outcome::Retry => return,
    };
    let ack = Ack {
        event_id: event.id.clone(),
        consumer_id: consumer_id.to_owned(),
        outcome,
        detail,
    };
    if let Err(error) = queue.ack(ack).await {
        // A concurrent STOP or an expired lease may have finalized the event
        // already; the terminal state is what matters, not this ack.
        warn!(%error, event_id = %event.id, "scenario ack was rejected");
    }
}

/// Sleep unless shutdown interrupts it. Returns false when cancelled.
async fn sleep(duration: Duration, cancel: &CancellationToken) -> bool {
    tokio::select! {
        () = tokio::time::sleep(duration) => true,
        () = cancel.cancelled() => false,
    }
}

fn build_ha(config: &ScenarioConfig) -> anyhow::Result<Arc<home_ha::Client>> {
    let token = config.ha_token.clone();
    let client = config.ha_ca_file.as_ref().map_or_else(
        || home_ha::Client::new(&config.ha_url, token.clone(), config.ha_timeout),
        |path| {
            home_ha::Client::new_with_ca_file(
                &config.ha_url,
                token.clone(),
                config.ha_timeout,
                path,
            )
        },
    );
    Ok(Arc::new(client.context("Home Assistant client")?))
}

fn scenario_client(config: &ScenarioConfig) -> anyhow::Result<home_proto::ScenarioClient> {
    let client = config.scheduler_ca_file.as_ref().map_or_else(
        || {
            home_proto::ScenarioClient::new(
                &config.scheduler_url,
                config.scheduler_token.clone(),
                config.scheduler_timeout,
            )
        },
        |path| {
            home_proto::ScenarioClient::new_with_ca_file(
                &config.scheduler_url,
                config.scheduler_token.clone(),
                config.scheduler_timeout,
                path,
            )
        },
    );
    client.context("scenario client")
}

fn scheduler_client(config: &ScenarioConfig) -> anyhow::Result<home_proto::SchedulerClient> {
    let client = config.scheduler_ca_file.as_ref().map_or_else(
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
    );
    client.context("scheduler client")
}

fn log_configuration(config: &ScenarioConfig) {
    info!(
        mode = if config.execute { "execute" } else { "shadow" },
        consumer_id = %config.consumer_id,
        queue = %config.queue,
        scheduler_endpoint = %config.scheduler_url,
        ha_url = %config.ha_url,
        timezone = %config.timezone,
        catalog_ttl = ?config.catalog_ttl,
        poll_interval = ?config.poll_interval,
        max_backoff = ?config.max_backoff,
        batch_size = config.batch_size,
        lease_seconds = config.lease_seconds,
        max_attempts = config.max_attempts,
        ledger = config
            .ledger_path
            .as_ref()
            .map_or("in-memory".to_owned(), |path| path.display().to_string()),
        scheduler_token = presence(&config.scheduler_token),
        ha_token = presence(&config.ha_token),
        "ha-scenario configuration"
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
