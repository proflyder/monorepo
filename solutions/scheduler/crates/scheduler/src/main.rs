use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("-healthcheck" | "--healthcheck") => scheduler::health::run_healthcheck().await,
        Some("-status" | "--status") => scheduler::health::run_status().await,
        Some("-service-token-sha256" | "--service-token-sha256") => {
            match scheduler::config::Config::load() {
                Ok(config) => {
                    println!(
                        "{}",
                        home_app_auth::sha256_hex(config.service_token.expose())
                    );
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("service-token-sha256: {error}");
                    ExitCode::FAILURE
                }
            }
        }
        _ => match scheduler::app::run().await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("scheduler stopped: {error:#}");
                ExitCode::FAILURE
            }
        },
    }
}
