//! Scheduler crate.
//!
//! The domain, store and API layers mirror the former Go implementation byte
//! for byte: the `testdata/planner/*.json` and `testdata/ids.json` vectors under
//! `solutions/scheduler` are the shared golden tables.
//!
//! [`config`], [`engine`], [`app`] and [`health`] turn those layers into the
//! deployed process. They are the only modules that touch the environment, the
//! clock, the async runtime or the network beyond the Connect handler.

#![deny(missing_debug_implementations)]

pub mod api;
pub mod app;
pub mod config;
pub mod domain;
pub mod engine;
pub mod health;
pub mod store;
