use std::process::ExitCode;

use anyhow::Context;
use ble_gateway::cli::Config;
use ble_gateway::gateway::{Gateway, normalize_token};
use ble_gateway::wifi::WpaClient;
use clap::Parser;
use home_runtime::{Supervisor, shutdown_signal};
use home_telemetry::{LevelFilter, TelemetryBuilder, local_offset};
use tracing::{error, info};

/// From Cargo.toml through the rules_rust `version` attribute; `--version` prints it.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Optional `--define=ble_gateway_git_rev=<rev>`; `unknown` for plain builds.
const GIT_REV: &str = match option_env!("BLE_GATEWAY_GIT_REV") {
    Some(revision) => revision,
    None => "unknown",
};

fn main() -> ExitCode {
    let config = Config::parse();
    let level = std::env::var("RUST_LOG")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| config.log_level.parse().unwrap_or(LevelFilter::INFO));
    let mut telemetry = TelemetryBuilder::new()
        .with_max_level(level)
        .with_local_offset(local_offset());
    if let Some(path) = &config.log {
        telemetry = telemetry.with_file(path);
    }
    if let Err(cause) = telemetry.init() {
        eprintln!("log: {cause}");
        return ExitCode::FAILURE;
    }
    info!(
        version = VERSION,
        revision = GIT_REV,
        "ble-gateway starting"
    );
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(cause) => {
            eprintln!("runtime: {cause}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(run(config)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(cause) => {
            error!("{cause:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run(config: Config) -> anyhow::Result<()> {
    let token = home_secret::load_or_create_with(&config.token_file, normalize_token, || {
        home_secret::numeric(6)
    })
    .with_context(|| format!("token file {}", config.token_file.display()))?;
    info!(path = %config.token_file.display(), "device token ready");
    if config.test_wpa {
        let message = WpaClient::new(&config.wpa_ctrl)
            .test_wpa()
            .await
            .context("temporary wpa_supplicant network")?;
        info!(%message, "test-wpa");
        return Ok(());
    }
    let supervisor = Supervisor::new();
    let gateway = Gateway::start(config, token, supervisor.clone()).await?;
    shutdown_signal().await;
    info!("shutdown signal; unregistering BLE");
    gateway.disable_ble().await;
    supervisor
        .shutdown(std::time::Duration::from_secs(30))
        .await;
    Ok(())
}
