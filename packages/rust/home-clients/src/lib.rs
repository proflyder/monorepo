//! Read-only HTTP clients for the home observability stack.
//!
//! Three narrow clients, each mirroring the former Go implementation of the MCP
//! gateway one-to-one:
//!
//! * [`prometheus`] — one instant query (`/api/v1/query`);
//! * [`alertmanager`] — active, unsilenced, uninhibited alerts (`/api/v2/alerts`);
//! * [`loki`] — a backward range query (`/loki/api/v1/query_range`).
//!
//! They share one failure type. [`Error::kind`] is [`ErrorKind::Transient`] for
//! network failures, timeouts, `408`, `425`, `429` and `5xx` (a retry may
//! succeed) and [`ErrorKind::Permanent`] for every other `4xx`, a malformed
//! answer, a rejected query and an invalid argument. The classification is the
//! same as `packages/rust/home-ha` and `packages/go/httpapi`.
//!
//! Every client builds its own `reqwest` client through `home_tls`: the
//! workspace `reqwest` is compiled with `rustls-tls-manual-roots-no-provider`,
//! so the shared builder installs the ring provider and adds the operating
//! system's public roots. Redirects are not followed, matching the Go clients
//! (a `3xx` is a status error).

use std::time::Duration;

pub mod alertmanager;
pub mod loki;
pub mod prometheus;

/// How a failed call may be retried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// A network failure, timeout, throttling or a `5xx`: retrying later may
    /// succeed.
    Transient,
    /// A `4xx`, a malformed answer, a rejected query or an invalid argument:
    /// retrying the same call cannot succeed.
    Permanent,
}

/// One read-only backend call failure.
///
/// It carries the operation and a safe status/detail, never request headers or
/// response bodies.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{operation}: {detail}")]
    Invalid {
        operation: &'static str,
        detail: &'static str,
    },
    #[error("{operation}: returned HTTP {status}")]
    Status {
        operation: &'static str,
        status: u16,
    },
    #[error("{operation}: failed: {detail}")]
    Api {
        operation: &'static str,
        detail: String,
    },
    #[error("{operation}: request failed")]
    Transport {
        operation: &'static str,
        #[source]
        source: reqwest::Error,
    },
    #[error("{operation}: invalid JSON response")]
    Decode {
        operation: &'static str,
        #[source]
        source: serde_json::Error,
    },
    #[error("{operation}: public TLS roots: {source}")]
    Tls {
        operation: &'static str,
        #[source]
        source: home_tls::Error,
    },
}

impl Error {
    /// Whether a retry may succeed.
    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::Transport { .. } => ErrorKind::Transient,
            Self::Status { status, .. } => match *status {
                408 | 425 | 429 => ErrorKind::Transient,
                status if status >= 500 => ErrorKind::Transient,
                _ => ErrorKind::Permanent,
            },
            Self::Invalid { .. } | Self::Api { .. } | Self::Decode { .. } | Self::Tls { .. } => {
                ErrorKind::Permanent
            }
        }
    }

    /// Convenience predicate for a worker loop.
    #[must_use]
    pub fn is_transient(&self) -> bool {
        self.kind() == ErrorKind::Transient
    }
}

/// Build the shared `reqwest` client.
///
/// The workspace builds `reqwest` with `rustls-tls-manual-roots-no-provider`;
/// `home_tls` installs the ring provider and adds the operating system's root
/// bundle, so an `https://` backend (Prometheus behind a private TLS name)
/// verifies. Redirects are disabled: a query must not silently change its
/// destination, and a `3xx` surfaces as [`Error::Status`].
pub(crate) fn build_http(
    operation: &'static str,
    timeout: Duration,
) -> Result<reqwest::Client, Error> {
    home_tls::public_https_client()
        .map_err(|source| Error::Tls { operation, source })?
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|source| Error::Transport { operation, source })
}

/// Normalize a backend root: trim whitespace and every trailing slash.
pub(crate) fn normalize_base_url(base_url: &str) -> String {
    base_url.trim().trim_end_matches('/').to_owned()
}

/// Decode a JSON body, mapping a failure to [`Error::Decode`].
pub(crate) fn decode<T: serde::de::DeserializeOwned>(
    operation: &'static str,
    body: &[u8],
    cap: usize,
) -> Result<T, Error> {
    // The Go clients read at most `cap` bytes (`io.LimitReader`); a longer body
    // is truncated and then fails to decode, so the outcome is identical.
    let body = &body[..body.len().min(cap)];
    serde_json::from_slice(body).map_err(|source| Error::Decode { operation, source })
}
