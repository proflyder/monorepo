//! WebRTC signaling for phone <-> station calls.
//!
//! The service keeps ephemeral call state, fans events out to per-endpoint
//! `Watch` streams and relays SDP/ICE. It never sees or stores media. See
//! `packages/proto/home/v1/intercom.proto` for the wire contract and
//! `solutions/intercom/README.md` for the sequence.

pub mod api;
pub mod app;
pub mod config;
pub mod domain;
pub mod health;

pub use app::VERSION;
