//! Distroless `ha-scenario -healthcheck` probe.
//!
//! The executor has no listener, so readiness means "the authenticated
//! scheduler API answers and reports a healthy tick loop", exactly like
//! `ha-bridge`. The shared probe lives in [`crate::health::probe`].

use std::process::ExitCode;
use std::time::Duration;

use crate::scenario_config::ScenarioConfig;

/// A single probe stays below the Kubernetes timeout.
const TIMEOUT: Duration = Duration::from_secs(5);

/// `ha-scenario -healthcheck`.
pub async fn run_healthcheck() -> ExitCode {
    let config = match ScenarioConfig::load() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("healthcheck: configuration: {error}");
            return ExitCode::FAILURE;
        }
    };
    crate::health::probe(
        &config.scheduler_url,
        &config.scheduler_token,
        config.scheduler_ca_file.as_deref(),
        TIMEOUT,
    )
    .await
}
