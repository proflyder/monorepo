//! Command-line entry point. Everything real lives in the library; the extra
//! behaviour is the `-healthcheck` / `-status` pair the distroless image needs.

use anyhow::Context as _;

fn argument() -> Option<String> {
    std::env::args().nth(1)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match argument().as_deref() {
        Some("-healthcheck" | "--healthcheck") => return healthcheck().await,
        Some("-status" | "--status") => return status(),
        _ => {}
    }
    voice_proxy::app::run().await
}

/// Loads the same configuration as the server (so a broken environment or an
/// unreadable Secret mount fails the probe) and calls the upstream model list.
async fn healthcheck() -> anyhow::Result<()> {
    let config = voice_proxy::config::Config::load().context("healthcheck: configuration")?;
    voice_proxy::health::probe(&config.upstream).await?;
    Ok(())
}

/// Prints the redacted JSON summary; never a token value.
fn status() -> anyhow::Result<()> {
    let config = voice_proxy::config::Config::load().context("status: configuration")?;
    let stdout = std::io::stdout();
    serde_json::to_writer_pretty(stdout.lock(), &config.redacted()).context("status: encode")?;
    println!();
    Ok(())
}
