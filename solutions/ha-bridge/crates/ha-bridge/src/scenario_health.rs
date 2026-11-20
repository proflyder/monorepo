use std::process::ExitCode;
use std::time::Duration;

use crate::scenario_config::ScenarioConfig;

const TIMEOUT: Duration = Duration::from_secs(5);

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
