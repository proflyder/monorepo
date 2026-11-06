//! wakegrant service: one-time `logon`/`unlock` grants for a machine woken by
//! Home Assistant.
//!
//! The wire contract is `packages/contracts/wakegrant/PROTOCOL.md`. The crate
//! is split into a pure [`domain`] (no I/O), a [`store`] over the shared
//! single-writer SQLite actor, the axum [`api`], typed [`config`], the
//! [`app`] composition root and the image's [`health`] probe.

#![forbid(unsafe_code)]

pub mod api;
pub mod app;
pub mod config;
pub mod domain;
pub mod health;
pub mod store;

#[cfg(test)]
mod api_tests;
#[cfg(test)]
mod contract_tests;
