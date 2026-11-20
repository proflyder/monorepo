#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

pub mod api_key;
pub mod entity;
pub mod frame;
pub mod pb;
pub mod session;
pub mod timezone;

pub const API_MAJOR: u32 = 1;
pub const API_MINOR: u32 = 15;
pub const ESPHOME_VERSION: &str = "2026.9.0";
pub const PROJECT_NAME: &str = "pulsar.satellite";

#[cfg(test)]
mod tests;
