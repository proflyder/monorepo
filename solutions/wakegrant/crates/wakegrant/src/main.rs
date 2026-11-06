//! Command-line entry point. Everything real lives in the library; the only
//! extra behaviour is the `-healthcheck` probe used by the distroless image.

use std::time::Duration;

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

/// Loads the same configuration as the server (so a broken Secret mount fails
/// the probe) and calls `/healthz` on loopback. No secret is printed.
async fn healthcheck() -> anyhow::Result<()> {
    let config = wakegrant::config::Config::load().context("healthcheck: configuration")?;
    let url = format!("{}/healthz", config.healthcheck_url());
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(4))
        .build()
        .context("healthcheck: http client")?;
    let response = client
        .get(&url)
        .send()
        .await
        .context("healthcheck: request failed")?;
    anyhow::ensure!(
        response.status() == reqwest::StatusCode::OK,
        "healthcheck: health returned HTTP {}",
        response.status()
    );
    Ok(())
}
