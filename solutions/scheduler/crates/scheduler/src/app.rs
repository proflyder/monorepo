use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use home_runtime::{Supervisor, shutdown_signal};
use home_telemetry::TelemetryBuilder;
use tokio::net::TcpListener;

use crate::api::vpn::VpnSettings;
use crate::api::{self, AlarmFacade, Service, VpnAccess};
use crate::config::Config;
use crate::domain::SystemClock;
use crate::engine::{DEFAULT_PURGE_INTERVAL, Engine};
use crate::store::{Config as StoreConfig, Store};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

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
    let vpn_access = vpn_access(&config);
    let router = api::router(
        service.clone(),
        AlarmFacade::new(service),
        config.token.expose(),
        vpn_access,
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

fn vpn_access(config: &Config) -> VpnAccess {
    vpn_access_from(config, &|path| std::fs::read(path))
}

fn vpn_access_from(
    config: &Config,
    read_file: &dyn Fn(&std::path::Path) -> std::io::Result<Vec<u8>>,
) -> VpnAccess {
    let settings = config
        .vpn_settings_file
        .as_deref()
        .and_then(|path| load_vpn_settings(path, read_file));
    let sync_token = config
        .vpn_sync_token_file
        .as_deref()
        .and_then(|path| read_file(path).ok())
        .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_owned())
        .filter(|token| !token.is_empty());
    tracing::info!(
        vpn_profiles = settings.is_some(),
        vpn_sync = sync_token.is_some(),
        "VPN API"
    );
    VpnAccess {
        settings,
        sync_token,
    }
}

fn load_vpn_settings(
    path: &std::path::Path,
    read_file: &dyn Fn(&std::path::Path) -> std::io::Result<Vec<u8>>,
) -> Option<Arc<VpnSettings>> {
    let Ok(bytes) = read_file(path) else {
        tracing::warn!(path = %path.display(), "VPN settings file is missing; VPN API disabled");
        return None;
    };
    VpnSettings::parse(&bytes)
        .inspect_err(|error| tracing::warn!(%error, "VPN settings are invalid; VPN API disabled"))
        .ok()
        .map(Arc::new)
}

fn store_config(config: &Config) -> StoreConfig {
    StoreConfig {
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

    fn config_with_vpn() -> Config {
        Config::load_from(
            &|key| match key {
                "SCHEDULER_TOKEN" => Some("t".to_owned()),
                "SCHEDULER_VPN_SETTINGS_FILE" => Some("/run/vpn.json".to_owned()),
                "SCHEDULER_VPN_SYNC_TOKEN_FILE" => Some("/run/vpn_sync_token".to_owned()),
                _ => None,
            },
            &|_path| Ok(Vec::new()),
        )
        .expect("config")
    }

    #[test]
    fn missing_vpn_files_disable_the_vpn_api_without_failing() {
        let access = vpn_access_from(&config_with_vpn(), &|_path| {
            Err(std::io::Error::from(std::io::ErrorKind::NotFound))
        });
        assert!(access.settings.is_none());
        assert!(access.sync_token.is_none());
    }

    #[test]
    fn vpn_files_enable_the_vpn_api() {
        let settings = br#"{"pool":"10.77.2.0/24","endpoint":"vpn.example.com:51820",
            "serverPublicKey":"c2VydmVyLXNlcnZlci1zZXJ2ZXItc2VydmVyLXNlcnY=",
            "dns":["192.0.2.1"],"allowedIps":["0.0.0.0/0"],
            "ocserv":{"url":"https://vpn.example.com/","group":"full"}}"#;
        let access = vpn_access_from(&config_with_vpn(), &|path| {
            if path.ends_with("vpn.json") {
                Ok(settings.to_vec())
            } else {
                Ok(b"sync-token\n".to_vec())
            }
        });
        assert!(access.settings.is_some());
        assert_eq!(access.sync_token.as_deref(), Some("sync-token"));
    }

    #[test]
    fn store_config_matches_the_go_defaults() {
        let config = Config::load_from(
            &|key| match key {
                "SCHEDULER_TOKEN" => Some("t".to_owned()),
                _ => None,
            },
            &|_path| Ok(Vec::new()),
        )
        .expect("config");
        let store = store_config(&config);
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
