//! Command entry point: the gateway, plus the distroless `-healthcheck` probe.

use std::process::ExitCode;

use tokio::runtime::Builder;

fn main() -> ExitCode {
    // `-healthcheck` is synchronous on purpose: it builds its own current-thread
    // runtime, so it must not start inside `#[tokio::main]` (a nested
    // `block_on` panics and would turn the liveness probe into a crash loop).
    if matches!(
        std::env::args().nth(1).as_deref(),
        Some("-healthcheck" | "--healthcheck")
    ) {
        return mcp_gateway::health::run_healthcheck();
    }
    let runtime = match Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("mcp-gateway runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    runtime.block_on(async {
        match mcp_gateway::app::run().await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("mcp-gateway stopped: {error:#}");
                ExitCode::FAILURE
            }
        }
    })
}
