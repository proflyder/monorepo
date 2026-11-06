//! Platform clients of the MCP gateway.
//!
//! * [`kubernetes`] — a small read-mostly Kubernetes REST client with the
//!   in-cluster ServiceAccount token re-read on every request and exactly one
//!   mutation (the rollout restart annotation patch). The gateway deliberately
//!   does not depend on `kube`/`k8s-openapi`: a second HTTP stack would grow the
//!   image for four GETs and one PATCH.
//! * [`ssh`] — the bb home-dispatcher bridge: one forced command over SSH, a
//!   pinned `known_hosts` file, host-key algorithms derived from the pinned key
//!   types, and a 1 MiB answer limit.
//!
//! Both port `solutions/mcp-gateway/internal/{kubernetes,bb}` one-to-one and are
//! classified as [`ErrorKind::Transient`] or [`ErrorKind::Permanent`] the same
//! way as the rest of the home services.

pub mod kubernetes;
pub mod ssh;

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
