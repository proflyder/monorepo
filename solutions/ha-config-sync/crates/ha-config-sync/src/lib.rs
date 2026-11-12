//! Generic Home Assistant configuration delivery.
//!
//! The service compares the files it carries with the host through a
//! forced-command SSH agent, applies drift, validates the configuration through
//! the Home Assistant REST API, reloads what changed and rolls back on any
//! failure. Project files, endpoints and secrets arrive only through
//! [`config`].
//!
//! The agent protocol is documented in `solutions/ha-config-sync/README.md` and
//! implemented by `agent/ha-config-agent.sh`.

pub mod agent;
pub mod app;
pub mod config;
pub mod ha;
pub mod manifest;
pub mod metrics;
pub mod protocol;
pub mod sync;

/// Reported implementation version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
