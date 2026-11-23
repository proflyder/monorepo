use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use home_app_auth::{AccessCache, AccessSnapshot};
use home_runtime::{Supervisor, shutdown_signal};
use home_telemetry::TelemetryBuilder;
use home_worker::{LeaseRequest, SchedulerQueue, Worker, WorkerConfig};
use tokio::net::TcpListener;
use tracing::{info, warn};

use crate::catalog_api;
use crate::control::Control;
use crate::control_config::ControlConfig;
use crate::discovery::{CatalogSource, Provider};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

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
        config.toggles(),
        config.cache_ttl,
    ));
    let source: Arc<dyn CatalogSource> = provider;
    let access = AccessCache::new(fetch_initial_access(&config).await);
    let router = home_http::with_timeout(
        catalog_api::router(
            Arc::clone(&source),
            access.clone(),
            format!(
                "{}/v1/access/snapshot",
                config.scheduler_url.trim_end_matches('/')
            ),
            config.scheduler_token.expose().to_owned(),
        ),
        REQUEST_TIMEOUT,
    );

    let listener = TcpListener::bind(&config.listen)
        .await
        .with_context(|| format!("bind {}", config.listen))?;
    let address = listener.local_addr().context("listen address")?;
    info!(
        version = VERSION,
        addr = %address,
        scenario_label = %config.scenario_label,
        profile_label = %config.profile_label,
        exclude = ?config.exclude,
        execute = config.execute,
        "catalog listening"
    );

    let supervisor = Supervisor::new();
    let cancel = supervisor.token();
    let url = format!(
        "{}/v1/access/snapshot",
        config.scheduler_url.trim_end_matches('/')
    );
    let token = config.scheduler_token.expose().to_owned();
    let cache = access;
    let refresh_cancel = cancel.clone();
    supervisor.spawn("access-refresh", async move {
        let mut tick = tokio::time::interval(ACCESS_REFRESH);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                () = refresh_cancel.cancelled() => break,
                _ = tick.tick() => {
                    match fetch_access(&url, &token).await {
                        Ok(snapshot) => cache.replace(snapshot),
                        Err(error) => tracing::warn!(%error, "catalog access refresh failed"),
                    }
                }
            }
        }
    });

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

const ACCESS_REFRESH: Duration = Duration::from_secs(30);
const ACCESS_TIMEOUT: Duration = Duration::from_secs(5);

async fn fetch_initial_access(config: &ControlConfig) -> AccessSnapshot {
    let url = format!(
        "{}/v1/access/snapshot",
        config.scheduler_url.trim_end_matches('/')
    );
    match fetch_access(&url, config.scheduler_token.expose()).await {
        Ok(snapshot) => snapshot,
        Err(error) => {
            tracing::warn!(%error, "catalog access snapshot is unavailable");
            AccessSnapshot::default()
        }
    }
}

async fn fetch_access(url: &str, token: &str) -> Result<AccessSnapshot, String> {
    let client = reqwest::Client::builder()
        .timeout(ACCESS_TIMEOUT)
        .build()
        .map_err(|error| format!("access client: {error}"))?;
    let response = client
        .get(url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|_| "access request failed".to_owned())?;
    if !response.status().is_success() {
        return Err(format!(
            "access returned HTTP {}",
            response.status().as_u16()
        ));
    }
    let body = response
        .bytes()
        .await
        .map_err(|_| "access body".to_owned())?;
    AccessSnapshot::parse(&body).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::VERSION;

    #[test]
    fn version_is_reported() {
        assert!(!VERSION.is_empty());
    }
}
