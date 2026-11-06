//! Pure domain of the MCP gateway.
//!
//! Everything in this module is a portfolio of pure functions and in-memory
//! state machines: no HTTP, no filesystem, no clock reads except through an
//! injected [`Clock`]. The Go implementation lives in
//! `solutions/mcp-gateway/internal/{catalog,tools,pending,agentmode,redact}`;
//! the golden vectors in `solutions/mcp-gateway/testdata/vectors` are the port
//! contract and are read from disk by the tests, never copied.
//!
//! * [`catalog`] — deployment catalog YAML, validation with the exact Go error
//!   texts, and spoken-name resolution.
//! * [`speech`] — the spoken-line normalizer (markdown, URLs, 160 runes).
//! * [`redact`] — secret redaction for logs and reports.
//! * [`alert`] — Alertmanager alert ↔ catalog service matching.
//! * [`status`] — the `infra_status` aggregate (issues and verdict).
//! * [`report`] — the full Telegram report (headline, problems first, 3500
//!   runes) built from pre-fetched observations.
//! * [`state`] — pending actions with TTL, the agent-mode switch and the
//!   restart cooldown.

pub mod alert;
pub mod catalog;
pub mod clock;
pub mod redact;
pub mod report;
pub mod speech;
pub mod state;
pub mod status;

#[cfg(test)]
mod vectors;

pub use clock::{Clock, FixedClock, SystemClock};
pub use redact::PLACEHOLDER;
pub use speech::{MAX_SPEECH_RUNES, speech};
