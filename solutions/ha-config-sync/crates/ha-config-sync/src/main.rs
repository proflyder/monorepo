//! `ha-config-sync` entry point.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    ha_config_sync::app::run().await
}
