//! Command entry point: the currency bot service and the distroless
//! `-healthcheck` probe used by the deployment.

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("-healthcheck" | "--healthcheck") => currency_bot::health::run_healthcheck().await,
        _ => match currency_bot::app::run().await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("currency-bot stopped: {error:#}");
                ExitCode::FAILURE
            }
        },
    }
}
