pub mod agent;
pub mod app;
pub mod config;
pub mod ha;
pub mod manifest;
pub mod metrics;
pub mod protocol;
pub mod sync;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
