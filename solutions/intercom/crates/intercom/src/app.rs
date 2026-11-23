use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use home_app_auth::{AccessCache, AccessSnapshot, Modules, TokenRecord};
use home_runtime::{Supervisor, shutdown_signal};
use home_telemetry::TelemetryBuilder;
use tokio::net::TcpListener;
use tokio::time::{MissedTickBehavior, interval};

use crate::api::{self, Api, TokenAuth};
use crate::config::Config;
use crate::domain::{Intercom, SystemClock, Timeouts};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);
const EXPIRY_TICK: Duration = Duration::from_millis(500);

pub async fn run() -> anyhow::Result<()> {
    let config = Config::load().context("configuration")?;
    TelemetryBuilder::new()
        .with_json()
        .init()
        .context("telemetry")?;

    let timeouts = Timeouts {
        ring: config.ring_timeout,
        connect: config.connect_timeout,
        max_call: config.max_call_duration,
        ended_retention: config.ended_retention,
    };
    let intercom = Arc::new(Intercom::new(
        &config.phone,
        &config.station,
        timeouts,
        config.ice_servers.clone(),
        Arc::new(SystemClock),
    ));
    let api = Api::new(Arc::clone(&intercom), VERSION);
    let access = AccessCache::new(legacy_snapshot(&config));
    if !config.access_url.is_empty() {
        match fetch_access(&config.access_url, config.service_token.expose()).await {
            Ok(snapshot) => access.replace(snapshot),
            Err(error) => tracing::warn!(%error, "intercom access snapshot is unavailable"),
        }
    }
    let auth = TokenAuth::new(
        access.clone(),
        &config.phone.id,
        home_secret::Token::new(config.station_token.expose()),
        &config.station.id,
    );
    let router = api::router(api, auth);

    let listener = TcpListener::bind(&config.listen)
        .await
        .with_context(|| format!("bind {}", config.listen))?;
    let address = listener.local_addr().context("listen address")?;
    tracing::info!(
        service = "intercom",
        version = VERSION,
        addr = %address,
        phone = %config.phone.id,
        station = %config.station.id,
        ring_ms = config.ring_timeout.as_millis(),
        connect_ms = config.connect_timeout.as_millis(),
        ice_servers = config.ice_servers.len(),
        "intercom listening"
    );

    let supervisor = Supervisor::new();
    let cancel = supervisor.token();
    if !config.access_url.is_empty() {
        let url = config.access_url.clone();
        let token = config.service_token.expose().to_owned();
        let cache = access.clone();
        let refresh_cancel = cancel.clone();
        supervisor.spawn("access-refresh", async move {
            let mut tick = interval(REFRESH);
            tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    () = refresh_cancel.cancelled() => break,
                    _ = tick.tick() => {
                        match fetch_access(&url, &token).await {
                            Ok(snapshot) => cache.replace(snapshot),
                            Err(error) => tracing::warn!(%error, "intercom access refresh failed"),
                        }
                    }
                }
            }
        });
    }
    let ticker = Arc::clone(&intercom);
    let ticker_cancel = cancel.clone();
    supervisor.spawn("expiry", async move {
        let mut tick = interval(EXPIRY_TICK);
        tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                () = ticker_cancel.cancelled() => break,
                _ = tick.tick() => ticker.tick(),
            }
        }
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
    tracing::info!(event = "stopped", "intercom stopped");
    serve_result.context("serve")?;
    Ok(())
}

const REFRESH: Duration = Duration::from_secs(30);
const ACCESS_TIMEOUT: Duration = Duration::from_secs(5);

fn legacy_snapshot(config: &Config) -> AccessSnapshot {
    AccessSnapshot {
        revision: 0,
        tokens: vec![TokenRecord {
            id: "legacy".to_owned(),
            name: "mama".to_owned(),
            sha256: home_app_auth::sha256_hex(config.phone_token.expose()),
            owner: false,
            modules: Modules::member()
                .ids()
                .into_iter()
                .map(str::to_owned)
                .collect(),
        }],
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
