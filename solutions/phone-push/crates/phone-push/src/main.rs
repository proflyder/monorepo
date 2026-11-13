//! Command entry point: the push service plus the distroless `-healthcheck`.

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("-healthcheck" | "--healthcheck") => phone_push::app::healthcheck_exit(),
        _ => match phone_push::app::run().await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("phone-push stopped: {error:#}");
                ExitCode::FAILURE
            }
        },
    }
}
