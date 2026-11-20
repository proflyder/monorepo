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
