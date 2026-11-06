//! Upstream transport and the SSE protocol subset.

pub mod sse;
mod upstream;

pub use upstream::{DEFAULT_TIMEOUT, Fetch, MAX_BODY, Upstream, UpstreamError};
