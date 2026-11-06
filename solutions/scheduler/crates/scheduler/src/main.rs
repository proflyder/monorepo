//! Command entry point: the scheduler service, plus the distroless
//! `-healthcheck` and `-status` probes used by the deployment and smoke tests.

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("-healthcheck" | "--healthcheck") => scheduler::health::run_healthcheck().await,
        Some("-status" | "--status") => scheduler::health::run_status().await,
        _ => match scheduler::app::run().await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("scheduler stopped: {error:#}");
                ExitCode::FAILURE
            }
        },
    }
}
