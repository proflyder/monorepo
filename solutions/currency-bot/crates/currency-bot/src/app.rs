//! Composition root: configuration, telemetry, SQLite store, the two HTTP
//! listeners and the two periodic jobs.
//!
//! Shutdown is ordered: both listeners stop accepting and drain, background
//! webhook work is drained, the jobs are cancelled, then the SQLite writer
//! thread is closed.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use home_runtime::{Supervisor, shutdown_signal};
use home_telemetry::{LevelFilter, TelemetryBuilder};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::api::{ApiState, StoreReadiness, api_router, webhook_router};
use crate::config::Config;
use crate::domain::clock::{Clock, SystemClock};
use crate::domain::schedule::{next_hourly, next_weekly};
use crate::metrics::Metrics;
use crate::pipeline::Pipeline;
use crate::sources::kurs::Kurs;
use crate::store::Store;
use crate::telegram::TelegramSender;
use home_config::LogLevel;

/// Reported implementation version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);
const WEBHOOK_DRAIN_TIMEOUT: Duration = Duration::from_secs(10);
const WEBHOOK_RETRY_INITIAL: Duration = Duration::from_secs(1);
const WEBHOOK_RETRY_MAX: Duration = Duration::from_secs(60);
const WEBHOOK_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(15);
const JOB_HOURLY: &str = "hourly";
const JOB_CLEANUP: &str = "cleanup";

/// Starts the service and blocks until SIGTERM/SIGINT or a listener failure.
pub async fn run() -> anyhow::Result<()> {
    let config = Config::load().context("configuration")?;
    TelemetryBuilder::new()
        .with_max_level(level_filter(config.log_level))
        .with_json()
        .init()
        .context("telemetry")?;

    let store = Arc::new(Store::open(&config.db_path).context("open currency store")?);
    let metrics = Arc::new(Metrics::new());
    let kurs = Arc::new(Kurs::new(config.kurs_url.clone()).map_err(anyhow::Error::msg)?);
    let sender = Arc::new(
        TelegramSender::new(&config.telegram_api, config.bot_token.clone())
            .map_err(anyhow::Error::msg)?,
    );
    let pipeline = Arc::new(Pipeline {
        source: kurs,
        store: Arc::clone(&store),
        sender: Arc::clone(&sender) as Arc<dyn crate::pipeline::MessageSender>,
        metrics: Arc::clone(&metrics),
        clock: Arc::new(SystemClock),
    });

    let supervisor = Supervisor::new();
    let cancel = supervisor.token();
    let tasks = TaskTracker::new();
    let state = Arc::new(ApiState {
        token: config.api_token.clone(),
        webhook_secret: config.webhook_secret.clone(),
        allowed_chats: config.allowed_chats.clone(),
        default_chat: config
            .allowed_chats
            .first()
            .cloned()
            .expect("configuration requires one chat"),
        pipeline: Arc::clone(&pipeline),
        store: Arc::clone(&store),
        sender: Arc::clone(&sender) as Arc<dyn crate::pipeline::MessageSender>,
        metrics: Arc::clone(&metrics),
        clock: Arc::new(SystemClock),
        readiness: Arc::new(StoreReadiness::new(Arc::clone(&store))),
        tasks: tasks.clone(),
        cancel: cancel.clone(),
        dedup: std::sync::Mutex::new(crate::api::UpdateDedup::default()),
        dispatch_inline: false,
    });

    let api_listener = TcpListener::bind(&config.http_addr)
        .await
        .with_context(|| format!("bind {}", config.http_addr))?;
    let webhook_listener = TcpListener::bind(&config.webhook_addr)
        .await
        .with_context(|| format!("bind {}", config.webhook_addr))?;
    tracing::info!(
        service = "currency-bot",
        version = VERSION,
        http_addr = %api_listener.local_addr().context("api address")?,
        webhook_addr = %webhook_listener.local_addr().context("webhook address")?,
        db = %config.db_path.display(),
        timezone = %config.timezone,
        chats = config.allowed_chats.len(),
        hourly_minute = config.hourly_minute,
        cleanup_weekday = config.cleanup_weekday,
        cleanup_hour = config.cleanup_hour,
        retention_days = config.retention_days,
        "currency-bot listening"
    );

    if let Some(url) = config.webhook_url.clone() {
        let sender = Arc::clone(&sender);
        let secret = config.webhook_secret.clone();
        let cancel = cancel.clone();
        supervisor.spawn("webhook", async move {
            register_webhook(sender, url, secret, cancel).await;
        });
    }

    let chat_id = state.default_chat.clone();
    {
        let pipeline = Arc::clone(&pipeline);
        let timezone = config.location.clone();
        let cancel = cancel.clone();
        let minute = config.hourly_minute;
        supervisor.spawn("hourly", async move {
            hourly_job(pipeline, timezone, minute, chat_id, cancel).await;
        });
    }
    {
        let store = Arc::clone(&store);
        let metrics = Arc::clone(&metrics);
        let timezone = config.location.clone();
        let cancel = cancel.clone();
        let weekday = config.cleanup_weekday;
        let hour = config.cleanup_hour;
        let retention_days = config.retention_days;
        supervisor.spawn("cleanup", async move {
            cleanup_job(
                store,
                metrics,
                timezone,
                weekday,
                hour,
                retention_days,
                cancel,
            )
            .await;
        });
    }

    let api_server = home_http::serve(api_listener, api_router(Arc::clone(&state)), cancel.clone());
    let webhook_server = home_http::serve(
        webhook_listener,
        webhook_router(Arc::clone(&state)),
        cancel.clone(),
    );
    tokio::pin!(api_server);
    tokio::pin!(webhook_server);
    let serve_result = tokio::select! {
        result = &mut api_server => result.context("serve api"),
        result = &mut webhook_server => result.context("serve webhook"),
        () = shutdown_signal() => {
            cancel.cancel();
            let api = api_server.await.context("serve api");
            let webhook = webhook_server.await.context("serve webhook");
            api.and(webhook)
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
    tasks.close();
    if tokio::time::timeout(WEBHOOK_DRAIN_TIMEOUT, tasks.wait())
        .await
        .is_err()
    {
        tracing::warn!("webhook tasks did not finish before the shutdown timeout");
    }
    drop(state);
    drop(pipeline);
    drop(sender);
    match Arc::try_unwrap(store) {
        Ok(store) => store.shutdown().context("close currency store")?,
        Err(_still_shared) => tracing::warn!("store still shared at shutdown; dropping it"),
    }
    tracing::info!(event = "stopped", "currency-bot stopped");
    serve_result
}

fn level_filter(level: LogLevel) -> LevelFilter {
    match level {
        LogLevel::Debug => LevelFilter::DEBUG,
        LogLevel::Info => LevelFilter::INFO,
        LogLevel::Warn => LevelFilter::WARN,
        LogLevel::Error => LevelFilter::ERROR,
    }
}

async fn hourly_job(
    pipeline: Arc<Pipeline>,
    timezone: jiff::tz::TimeZone,
    minute: u8,
    chat_id: String,
    cancel: CancellationToken,
) {
    loop {
        let now = SystemClock.now();
        let Some(next) = next_hourly(now, &timezone, minute) else {
            tracing::error!("cannot compute the next hourly run");
            return;
        };
        if !sleep_until(next.timestamp(), &cancel).await {
            return;
        }
        tracing::info!(
            event = "job_started",
            job = JOB_HOURLY,
            "hourly job started"
        );
        if let Err(error) = pipeline.run(JOB_HOURLY, &chat_id, false, true).await {
            tracing::error!(event = "job_failed", job = JOB_HOURLY, error = %error, "hourly job failed");
        }
    }
}

async fn cleanup_job(
    store: Arc<Store>,
    metrics: Arc<Metrics>,
    timezone: jiff::tz::TimeZone,
    weekday: i8,
    hour: u8,
    retention_days: i64,
    cancel: CancellationToken,
) {
    loop {
        let now = SystemClock.now();
        let Some(next) = next_weekly(now, &timezone, weekday, hour) else {
            tracing::error!("cannot compute the next cleanup run");
            return;
        };
        if !sleep_until(next.timestamp(), &cancel).await {
            return;
        }
        tracing::info!(
            event = "job_started",
            job = JOB_CLEANUP,
            "cleanup job started"
        );
        metrics.inc_job_run(JOB_CLEANUP);
        let retention =
            Duration::from_secs(u64::try_from(retention_days.max(0)).unwrap_or(0) * 24 * 60 * 60);
        match store.clean_old_records(retention, SystemClock.now()) {
            Ok(removed) => tracing::info!(
                event = "cleanup_completed",
                job = JOB_CLEANUP,
                removed,
                "cleanup job completed"
            ),
            Err(error) => {
                metrics.inc_job_failure(JOB_CLEANUP);
                tracing::error!(event = "job_failed", job = JOB_CLEANUP, error = %error, "cleanup job failed");
            }
        }
    }
}

/// Sleep until `deadline`; `false` means the service is shutting down.
async fn sleep_until(deadline: jiff::Timestamp, cancel: &CancellationToken) -> bool {
    let now = SystemClock.now();
    let delay_ms = deadline
        .as_millisecond()
        .saturating_sub(now.as_millisecond())
        .max(0);
    let delay = Duration::from_millis(u64::try_from(delay_ms).unwrap_or(0));
    tokio::select! {
        () = cancel.cancelled() => false,
        () = tokio::time::sleep(delay) => !cancel.is_cancelled(),
    }
}

/// Retry `setWebhook` with exponential backoff in `[1s, 60s]` until it
/// succeeds or the process shuts down. A Telegram outage at start-up must not
/// crashloop the pod or affect readiness.
async fn register_webhook(
    sender: Arc<TelegramSender>,
    url: String,
    secret: home_secret::Secret,
    cancel: CancellationToken,
) {
    let mut delay = WEBHOOK_RETRY_INITIAL;
    loop {
        let attempt = tokio::select! {
            () = cancel.cancelled() => return,
            result = tokio::time::timeout(
                WEBHOOK_ATTEMPT_TIMEOUT,
                sender.set_webhook(&url, &secret, &["message"]),
            ) => result,
        };
        match attempt {
            Ok(Ok(())) => {
                tracing::info!("telegram webhook registered");
                return;
            }
            Ok(Err(error)) => {
                tracing::error!(error = %error, retry_in = ?delay, "failed to register telegram webhook; retrying");
            }
            Err(_elapsed) => {
                tracing::error!(retry_in = ?delay, "telegram webhook registration timed out; retrying");
            }
        }
        tokio::select! {
            () = cancel.cancelled() => return,
            () = tokio::time::sleep(delay) => {}
        }
        delay = (delay * 2).min(WEBHOOK_RETRY_MAX);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_levels_map_to_the_telemetry_filter() {
        assert_eq!(level_filter(LogLevel::Debug), LevelFilter::DEBUG);
        assert_eq!(level_filter(LogLevel::Info), LevelFilter::INFO);
        assert_eq!(level_filter(LogLevel::Warn), LevelFilter::WARN);
        assert_eq!(level_filter(LogLevel::Error), LevelFilter::ERROR);
    }
}
