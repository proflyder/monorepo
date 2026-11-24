use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("-healthcheck" | "--healthcheck") => secrets_gateway::app::healthcheck_exit().await,
        _ => match secrets_gateway::app::run().await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("secrets-gateway stopped: {error:#}");
                ExitCode::FAILURE
            }
        },
    }
}
