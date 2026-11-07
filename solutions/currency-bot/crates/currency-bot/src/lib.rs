//! currency-bot: kurs.kz rate snapshots, threshold alerts and a Telegram bot.
//!
//! The crate mirrors the former Go service: a pure [`domain`] (parser, IQR,
//! best-five, thresholds, dedup, message and schedules), the [`store`] on the
//! shared single-writer SQLite actor, the [`api`] listeners, the
//! [`pipeline`], [`config`], [`metrics`] and the [`app`] composition root.
//!
//! `testdata/contract` and `testdata/vectors` under `solutions/currency-bot`
//! are the frozen Go contract and are replayed unchanged.

#![forbid(unsafe_code)]

pub mod api;
pub mod app;
pub mod config;
pub mod domain;
pub mod health;
pub mod metrics;
pub mod pipeline;
pub mod sources;
pub mod store;
pub mod telegram;

#[cfg(test)]
mod contract_tests;
#[cfg(test)]
mod testdata;
