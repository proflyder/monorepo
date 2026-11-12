//! Composition root: configuration, telemetry, backends, registry and HTTP.

use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use home_runtime::{Supervisor, shutdown_signal};
use home_telemetry::TelemetryBuilder;
use tokio::net::TcpListener;

use crate::bb::{self, BbApi, BbResolver, Dispatcher};
use crate::checks;
use crate::config::Config;
use crate::domain::catalog::Catalog;
use crate::domain::clock::SystemClock;
use crate::domain::state::{AgentMode, KeyedStore, PendingStore};
use crate::metrics::Metrics;
use crate::server;
use crate::ssh::{Options as SshOptions, SshRunner};
use crate::tools::bb::{BbActGroup, BbReadGroup};
use crate::tools::infra_fix::FixGroup;
use crate::tools::infra_read::ReadGroup;
use crate::tools::infra_report::ReportGroup;
use crate::tools::{
    AlertLister, Deps, LogQuerier, PromQuerier, Registry, ReportSender, TimerScheduler,
};
use crate::voice;

/// Reported implementation version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

/// Start the gateway and block until SIGTERM/SIGINT or a listener failure.
pub async fn run() -> anyhow::Result<()> {
    let config = Config::load()
        .map_err(anyhow::Error::msg)
        .context("configuration")?;
    TelemetryBuilder::new()
        .with_json()
        .init()
        .context("telemetry")?;

    let catalog = if config.catalog_file.is_empty() {
        None
    } else {
        let data = std::fs::read_to_string(&config.catalog_file)
            .with_context(|| format!("read catalog {}", config.catalog_file))?;
        Some(Arc::new(
            Catalog::parse(&data).context("parse infra catalog")?,
        ))
    };

    let metrics = Arc::new(Metrics::new());
    let clock = Arc::new(SystemClock);
    let prom: Option<Arc<dyn PromQuerier>> = match non_empty(&config.prometheus_url) {
        Some(url) => Some(Arc::new(
            home_clients::prometheus::Client::new(url, config.query_timeout)
                .map_err(|error| anyhow::anyhow!("prometheus client: {error}"))?,
        )),
        None => None,
    };
    let alerts: Option<Arc<dyn AlertLister>> = match non_empty(&config.alertmanager_url) {
        Some(url) => Some(Arc::new(
            home_clients::alertmanager::Client::new(url, config.query_timeout)
                .map_err(|error| anyhow::anyhow!("alertmanager client: {error}"))?,
        )),
        None => None,
    };
    let loki: Option<Arc<dyn LogQuerier>> = match non_empty(&config.loki_url) {
        Some(url) => Some(Arc::new(
            home_clients::loki::Client::new(url, config.query_timeout)
                .map_err(|error| anyhow::anyhow!("loki client: {error}"))?,
        )),
        None => None,
    };
    let report: Option<Arc<dyn ReportSender>> = if config.telegram_token.is_empty() {
        None
    } else {
        let token = home_secret::Token::new(config.telegram_token.clone());
        Some(Arc::new(
            home_telegram::Sender::new(
                &config.telegram_api_url,
                token,
                &config.telegram_chat_id,
                config.query_timeout,
            )
            .map_err(|error| anyhow::anyhow!("telegram sender: {error}"))?,
        ))
    };

    let scheduler: Option<Arc<dyn crate::scheduler::SchedulerApi>> =
        if config.groups.iter().any(|name| name == "time") {
            Some(Arc::new(
                crate::scheduler::Client::new(
                    &config.scheduler_url,
                    config.scheduler_token.clone(),
                    config.query_timeout,
                )
                .map_err(anyhow::Error::msg)?,
            ))
        } else {
            None
        };

    let bb_targets = if config.bb_targets_file.is_empty() {
        None
    } else {
        Some(Arc::new(
            BbResolver::load(&config.bb_targets_file).map_err(anyhow::Error::msg)?,
        ))
    };
    let bb: Option<Arc<dyn BbApi>> = if config.bb_address.is_empty() {
        None
    } else {
        let runner = SshRunner::new(SshOptions {
            address: config.bb_address.clone(),
            user: config.bb_user.clone(),
            key_file: std::path::PathBuf::from(&config.bb_key_file),
            known_hosts_file: std::path::PathBuf::from(&config.bb_known_hosts_file),
            timeout: Some(config.bb_timeout),
        });
        Some(Arc::new(Dispatcher::new(
            Arc::new(runner),
            config.bb_timeout,
        )))
    };

    let deps = Arc::new(Deps {
        catalog,
        prom,
        alerts,
        loki,
        kube: Some(Arc::new(crate::kubernetes::Client::new(
            crate::kubernetes::Options {
                base_url: non_empty(&config.kube_api_url),
                token_file: non_empty_path(&config.kube_token_file),
                ca_file: non_empty_path(&config.kube_ca_file),
                timeout: Some(config.query_timeout),
                ..crate::kubernetes::Options::default()
            },
        )?)),
        report,
        checks: Some(Arc::new(
            checks::Runner::new(&config.ha_url, &config.ha_token)
                .map_err(|error| anyhow::anyhow!("checks runner: {error}"))?,
        )),
        pending: Arc::new(PendingStore::new(config.pending_ttl, clock.clone())),
        timers: Arc::new(TimerScheduler),
        metrics: Arc::clone(&metrics),
        clock: clock.clone(),
        location: config.location.clone(),
        timezone_name: config.timezone.clone(),
        scheduler,
        cooldown: config.cooldown,
        self_restart_delay: config.self_restart_delay,
        query_timeout: config.query_timeout,
        status_timeout: config.status_timeout,
        report_timeout: config.report_timeout,
        log_byte_cap: Arc::new(std::sync::atomic::AtomicUsize::new(config.log_byte_cap)),
        bb,
        bb_targets,
        bb_pending: Arc::new(KeyedStore::<bb::SpawnAction>::new(
            config.pending_ttl,
            clock,
        )),
        bb_mode: Arc::new(AgentMode::new()),
    });

    let voice = if config.voice_token.is_empty() {
        None
    } else {
        let bridge = deps
            .bb
            .clone()
            .expect("the voice route configuration requires the bb bridge");
        Some(voice::router(voice::Options {
            dispatcher: Arc::new(voice::BbDispatcher::new(bridge)),
            work_thread_id: config.bb_dispatcher_thread_id.clone(),
            mode: Arc::clone(&deps.bb_mode),
            metrics: Arc::clone(&metrics),
            clock: Arc::clone(&deps.clock),
            limiter: Arc::new(voice::TokenBucket::production()),
            poll_interval: Duration::from_secs(1),
            max_wait: voice::DEFAULT_MAX_WAIT,
            updates_wait: voice::updates_wait_for(config.bb_timeout),
        }))
    };

    let registry = Registry::new(vec![
        Arc::new(ReadGroup::new(Arc::clone(&deps))),
        Arc::new(FixGroup::new(Arc::clone(&deps))),
        Arc::new(ReportGroup::new(Arc::clone(&deps))),
        Arc::new(BbReadGroup::new(Arc::clone(&deps))),
        Arc::new(BbActGroup::new(Arc::clone(&deps))),
        Arc::new(crate::tools::time::TimeGroup::new(Arc::clone(&deps))),
    ]);
    let groups = registry
        .select(&config.groups)
        .map_err(anyhow::Error::msg)
        .context("select tool groups")?;

    let router = server::router(server::Options {
        auth_token: config.auth_token.clone(),
        voice_token: (!config.voice_token.is_empty()).then(|| config.voice_token.clone()),
        voice,
        groups,
        deps: Arc::clone(&deps),
        metrics: Arc::clone(&metrics),
        version: VERSION.to_owned(),
        ready: true,
    });

    let listen = listen_address(&config.http_addr)?;
    let listener = TcpListener::bind(listen)
        .await
        .with_context(|| format!("bind {}", config.http_addr))?;
    tracing::info!(
        service = "mcp-gateway",
        version = VERSION,
        addr = %config.http_addr,
        groups = %config.groups.join(","),
        "mcp-gateway listening"
    );

    let supervisor = Supervisor::new();
    let cancel = supervisor.token();
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
    let unfinished = supervisor.shutdown(SHUTDOWN_TIMEOUT).await;
    if !unfinished.is_empty() {
        tracing::warn!(
            ?unfinished,
            "tasks did not stop before the shutdown timeout"
        );
    }
    serve_result.context("serve")?;
    tracing::info!(event = "stopped", "mcp-gateway stopped");
    Ok(())
}

fn non_empty(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.trim().to_owned())
}

fn non_empty_path(value: &str) -> Option<std::path::PathBuf> {
    non_empty(value).map(std::path::PathBuf::from)
}

fn listen_address(raw: &str) -> anyhow::Result<String> {
    let (host, port) = raw
        .rsplit_once(':')
        .ok_or_else(|| anyhow::anyhow!("invalid MCP_HTTP_ADDR: {raw:?}"))?;
    let host = match host {
        "" | "0.0.0.0" => "0.0.0.0",
        "[::]" | "::" => "[::]",
        other => other,
    };
    Ok(format!("{host}:{port}"))
}

/// `mcp-gateway -healthcheck` exit code.
pub fn healthcheck_exit() -> ExitCode {
    crate::health::run_healthcheck()
}
