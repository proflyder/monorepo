#![allow(non_snake_case)]

#[allow(clippy::pedantic, clippy::nursery)]
#[cfg(windows)]
pub mod bindings;
pub mod config;
pub mod credential;
pub mod ffi_guard;
pub mod gate;
pub mod kerb;
pub mod log;
pub mod provider;
pub mod sync;
pub mod worker;
pub mod worker_core;

#[cfg(windows)]
pub mod dll;
pub mod http;
pub mod wide;
