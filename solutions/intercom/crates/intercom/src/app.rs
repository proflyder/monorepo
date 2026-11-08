//! Composition root: configuration, telemetry, in-memory state and the
//! authenticated Connect API.
//!
//! Shutdown is ordered: the listener stops accepting, then the expiry ticker is
//! cancelled and joined by the supervisor.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context as _;
use home_runtime::{Supervisor, shutdown_signal};
use home_telemetry::TelemetryBuilder;
use tokio::net::TcpListener;
use tokio::time::{MissedTickBehavior, interval};

use crate::api::{self, Api, TokenAuth};
use crate::config::Config;
use crate::domain::{Intercom, SystemClock, Timeouts};

/// Reported implementation version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);
const EXPIRY_TICK: Duration = Duration::from_millis(500);

/// Starts the service and blocks until SIGTERM/SIGINT or a listener failure.
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
    let auth = TokenAuth::new(
        home_secret::Token::new(config.phone_token.expose()),
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
