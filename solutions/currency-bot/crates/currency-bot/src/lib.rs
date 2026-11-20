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
