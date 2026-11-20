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

async fn healthcheck() -> anyhow::Result<()> {
    let config = voice_proxy::config::Config::load().context("healthcheck: configuration")?;
    voice_proxy::health::probe(&config.upstream).await?;
    Ok(())
}

fn status() -> anyhow::Result<()> {
    let config = voice_proxy::config::Config::load().context("status: configuration")?;
    let stdout = std::io::stdout();
    serde_json::to_writer_pretty(stdout.lock(), &config.redacted()).context("status: encode")?;
    println!();
    Ok(())
}
