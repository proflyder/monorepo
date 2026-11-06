//! Composition root: configuration, telemetry, the HTTP proxy and the
//! background workers.
//!
//! Start-up order:
//!
//! * the listener is bound first;
//! * the dispatcher updates poller starts when routing and updates are on;
//! * one queue consumer per profile starts only when the queue and routing are
//!   both on (its gate leases only the current profile's queue);
//! * the router exists only when the full HA + gateway path is configured, so
//!   a partially configured deployment keeps the plain sanitiser behaviour.
//!
//! Shutdown stops accepting requests first, then cancels the workers and joins
//! them, so an in-flight request can still start its result announcer.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use home_runtime::{Supervisor, shutdown_signal};
use home_telemetry::{LevelFilter, TelemetryBuilder};
use home_worker::{Worker, WorkerConfig};
use tokio::net::TcpListener;

use crate::api::{Options as ProxyOptions, Proxy};
use crate::config::{Config, LogLevel};
use crate::domain::clock::{Clock, SystemClock};
use crate::domain::profile::Profile;
use crate::relay::{DEFAULT_TIMEOUT, Upstream};
use crate::services::{
    AnnouncementHandler, Gateway, HaAccess, HaAnnouncer, Producer, ProfileReader, Results,
    ResultsOptions, SchedulerAccess, SchedulerQueue, StationGate, Updates, UpdatesOptions,
    VoiceRouter, lease_request,
};

/// Reported implementation version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
const WORKER_POLL: Duration = Duration::from_secs(1);
const WORKER_RETRY_MIN: Duration = Duration::from_secs(2);
const WORKER_RETRY_MAX: Duration = Duration::from_secs(60);

/// Starts the service and blocks until SIGTERM/SIGINT or a listener failure.
///
/// # Errors
///
/// Returns a configuration, telemetry, bind or serve failure.
pub async fn run() -> anyhow::Result<()> {
    let config = Config::load().context("configuration")?;
    TelemetryBuilder::new()
        .with_max_level(level_filter(config.log_level))
        .with_json()
        .init()
        .context("telemetry")?;

    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let supervisor = Supervisor::new();

    let ha_access = Arc::new(HaAccess::new(
        &config.ha_url,
        config.ha_timeout,
        config.ha_token.provider(),
    ));
    let profile_reader = Arc::new(ProfileReader::new(
        Arc::clone(&ha_access),
        &config.profile_entity,
        config.profile_cache,
        Arc::clone(&clock),
    ));
    let announcer = Arc::new(HaAnnouncer::new(
        Arc::clone(&ha_access),
        &config.announce_script,
        &config.satellite,
    ));

    let scheduler = (!config.scheduler_url.is_empty()).then(|| {
        Arc::new(SchedulerAccess::new(
            &config.scheduler_url,
            config.gateway_timeout,
            config.scheduler_token.provider(),
        ))
    });
    let producer = scheduler.as_ref().map(|access| {
        Arc::new(Producer::new(
            Arc::clone(access),
            config.queue_enabled(),
            Arc::clone(&clock),
        ))
    });

    let mut voice_router = None;
    let mut updates = None;
    let mut consumer_jobs = Vec::new();
    if config.routing_enabled() {
        let gateway = Arc::new(
            Gateway::new(
                &config.gateway_url,
                config.gateway_token.provider(),
                config.gateway_timeout,
            )
            .context("gateway client")?,
        );
        let results = Arc::new(Results::new(
            Arc::clone(&gateway),
            Arc::clone(&announcer),
            producer.clone(),
            ResultsOptions {
                max_chars: config.announce_max_chars,
                timeout: config.result_timeout,
                wait: config.result_wait,
                retry: config.result_retry,
            },
            supervisor.clone(),
        ));
        voice_router = Some(Arc::new(VoiceRouter::new(
            Arc::clone(&profile_reader),
            Arc::clone(&gateway),
            Some(results),
        )));
        if config.updates_enabled() {
            updates = Some(Updates::new(
                Arc::clone(&gateway),
                Arc::clone(&profile_reader),
                Arc::clone(&announcer),
                producer.clone(),
                Arc::clone(&clock),
                UpdatesOptions {
                    rescan: config.updates_rescan,
                    wait: config.result_wait,
                    max_chars: config.announce_max_chars,
                    ttl: config.announce_ttl,
                    retry_min: config.updates_retry,
                    ..UpdatesOptions::default()
                },
            ));
        }
        if config.queue_enabled_and_routed()
            && let Some(access) = &scheduler
        {
            for profile in [Profile::Work, Profile::Personal] {
                let queue = SchedulerQueue::new(Arc::clone(access));
                let handler = AnnouncementHandler::new(
                    Arc::clone(&announcer),
                    Arc::clone(&profile_reader),
                    &config.satellite,
                    &config.media_player,
                    config.announce_ttl,
                    Arc::clone(&clock),
                    WORKER_POLL,
                );
                let gate = StationGate::new(
                    Arc::clone(&profile_reader),
                    &config.satellite,
                    &config.media_player,
                    profile,
                );
                let worker = Worker::new(
                    queue,
                    handler,
                    WorkerConfig {
                        lease: lease_request(&config.queue_consumer, profile),
                        poll_interval: WORKER_POLL,
                        retry_min: WORKER_RETRY_MIN,
                        retry_max: WORKER_RETRY_MAX,
                    },
                )
                .with_gate(gate);
                consumer_jobs.push((profile, worker));
            }
        }
    }

    let upstream = Upstream::new(&config.upstream, DEFAULT_TIMEOUT).context("upstream client")?;
    let proxy = Arc::new(Proxy::new(ProxyOptions {
        upstream,
        reasoning_effort: config.reasoning_effort.clone(),
        max_body: crate::relay::MAX_BODY,
        cancel_phrases: crate::domain::cancel::DEFAULT_CANCEL_PHRASES
            .iter()
            .map(|phrase| (*phrase).to_owned())
            .collect(),
        router: voice_router,
        clock: Arc::clone(&clock),
    }));

    let listener = TcpListener::bind(config.listen().context("listen address")?)
        .await
        .context("bind listen address")?;
    let address = listener.local_addr().context("listen address")?;
    tracing::info!(
        service = "voice-proxy",
        version = VERSION,
        listen = %address,
        upstream = %config.upstream,
        routing = config.routing_enabled(),
        updates = config.updates_enabled(),
        queue = config.queue_enabled(),
        "voice-proxy listening"
    );
    if config.routing_enabled() {
        tracing::info!(profile_entity = %config.profile_entity, "voice routing enabled");
    }
    if updates.is_some() {
        tracing::info!(rescan = config.updates_rescan, "updates poller enabled");
    }
    if !consumer_jobs.is_empty() {
        tracing::info!(consumer = %config.queue_consumer, "queue consumer enabled");
    }

    let cancel = supervisor.token();
    if let Some(updates) = updates {
        let cancel = cancel.clone();
        supervisor.spawn("updates", async move { updates.run(cancel).await });
    }
    for (profile, worker) in consumer_jobs {
        let cancel = cancel.clone();
        let name = match profile {
            Profile::Work => "queue-work",
            Profile::Personal => "queue-personal",
        };
        supervisor.spawn(name, async move { worker.run(cancel).await });
    }

    let server = home_http::serve(listener, proxy.router(), cancel.clone());
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
    let unfinished = supervisor.shutdown(SHUTDOWN_TIMEOUT).await;
    if !unfinished.is_empty() {
        tracing::warn!(
            ?unfinished,
            "tasks did not stop before the shutdown timeout"
        );
    }
    tracing::info!(event = "stopped", "voice-proxy stopped");
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
