//! `ha-control` entry point: the catalog server and control executor, plus the
//! distroless `-healthcheck` probe used by the readiness and liveness
//! executions.

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("-healthcheck" | "--healthcheck") => {
            ha_bridge::control_health::run_healthcheck().await
        }
        _ => match ha_bridge::control_app::run().await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("ha-control: {error:#}");
                ExitCode::FAILURE
            }
        },
    }
}
