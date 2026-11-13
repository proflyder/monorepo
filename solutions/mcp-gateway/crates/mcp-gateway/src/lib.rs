//! The MCP gateway: pure domain, shared clients, tool groups and the HTTP
//! surface.
//!
//! * [`domain`] — the pure, I/O-free core: catalog, speech, redaction, alert
//!   matching, status/report assembly and in-memory state with an injected
//!   clock. The golden vectors in `solutions/mcp-gateway/testdata/vectors` are
//!   the port contract.
//! * [`kubernetes`] — a small read-mostly Kubernetes REST client with the
//!   in-cluster ServiceAccount token re-read on every request and exactly one
//!   mutation (the rollout restart annotation patch).
//! * [`ssh`] — the bb home-dispatcher bridge: one forced command over SSH, a
//!   pinned `known_hosts` file, host-key algorithms derived from the pinned key
//!   types, and a 1 MiB answer limit.
//! * [`bb`] — the typed dispatcher client and the personal-only spoken
//!   allow-list on top of the SSH bridge.
//! * [`voice`] — the `/v1/voice/*` routing and long-poll surface.
//! * [`scheduler`] — the typed `home.v1.SchedulerService` client for the time
//!   tools.
//! * [`tools`] — the extensible `ToolGroup` registry and the infra tool groups.
//! * [`mcp`] / [`server`] — the `rmcp` server and the axum HTTP surface.
//! * [`config`] / [`app`] / [`health`] — the process around them.

pub mod app;
pub mod bb;
pub mod checks;
pub mod config;
pub mod domain;
pub mod health;
pub mod kubernetes;
pub mod mcp;
pub mod metrics;
pub mod push;
pub mod scheduler;
pub mod server;
pub mod ssh;
pub mod tools;
pub mod voice;

/// How a failed call may be retried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// A network failure, timeout, throttling or a `5xx`: retrying later may
    /// succeed.
    Transient,
    /// A `4xx`, a malformed answer or an invalid argument: retrying the same
    /// call cannot succeed.
    Permanent,
}
