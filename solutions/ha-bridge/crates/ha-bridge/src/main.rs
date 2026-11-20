use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("-healthcheck" | "--healthcheck") => ha_bridge::health::run_healthcheck().await,
        _ => match ha_bridge::app::run().await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("ha-bridge: {error:#}");
                ExitCode::FAILURE
            }
        },
    }
}
