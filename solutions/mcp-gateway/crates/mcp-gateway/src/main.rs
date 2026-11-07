//! Command entry point: the gateway, plus the distroless `-healthcheck` probe.

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("-healthcheck" | "--healthcheck") => mcp_gateway::health::run_healthcheck(),
        _ => match mcp_gateway::app::run().await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("mcp-gateway stopped: {error:#}");
                ExitCode::FAILURE
            }
        },
    }
}
