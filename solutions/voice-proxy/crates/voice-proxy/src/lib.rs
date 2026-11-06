//! voice-proxy: an OpenAI-compatible passthrough that makes assistant replies
//! safe for Piper TTS.
//!
//! The crate is split into a pure [`domain`] (no I/O, time through [`domain::Clock`]),
//! the typed [`openai`] wire shapes, the [`relay`] upstream transport with its
//! SSE subset, the [`services`] that perform I/O, the [`config`] contract, the
//! [`app`] composition root and the image's [`health`] probe.

#![forbid(unsafe_code)]

pub mod api;
pub mod app;
pub mod config;
pub mod domain;
pub mod health;
pub mod openai;
pub mod relay;
pub mod services;

#[cfg(test)]
mod contract_tests;
