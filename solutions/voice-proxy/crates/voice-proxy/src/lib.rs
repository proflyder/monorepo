#![forbid(unsafe_code)]

pub mod api;
pub mod app;
pub mod chat_relay;
pub mod config;
pub mod domain;
pub mod health;
pub mod openai;
pub mod relay;
pub mod services;

#[cfg(test)]
mod contract_tests;
