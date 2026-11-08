//! Command entry point: the intercom service plus the distroless
//! `-healthcheck` and `-status` probes used by the deployment.

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("-healthcheck" | "--healthcheck") => intercom::health::run_healthcheck().await,
        Some("-status" | "--status") => intercom::health::run_status().await,
        _ => match intercom::app::run().await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("intercom stopped: {error:#}");
                ExitCode::FAILURE
            }
        },
    }
}
