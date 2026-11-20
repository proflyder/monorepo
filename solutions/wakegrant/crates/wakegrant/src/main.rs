use anyhow::Context as _;

fn is_healthcheck() -> bool {
    std::env::args()
        .nth(1)
        .is_some_and(|argument| matches!(argument.as_str(), "-healthcheck" | "--healthcheck"))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if is_healthcheck() {
        return healthcheck().await;
    }
    wakegrant::app::run().await
}

async fn healthcheck() -> anyhow::Result<()> {
    let config = wakegrant::config::Config::load().context("healthcheck: configuration")?;
    wakegrant::health::probe(&config.healthcheck_url()).await?;
    Ok(())
}
