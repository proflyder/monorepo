//! Station BLE provisioning gateway.
//!
//! The deployed binary is `ble-gateway`; `bletest` is a small D-Bus client for
//! exercising the exported GATT application on the station. The library exposes
//! the same modules to unit tests so the gesture machine, the wpa_supplicant
//! client and the HTTP contract are covered without hardware.

pub mod bluetooth;
pub mod button;
pub mod cli;
pub mod control;
pub mod credentials;
pub mod gateway;
pub mod gatt;
pub mod status;
pub mod wifi;

#[cfg(test)]
mod test_support;

pub use cli::Config;
pub use gateway::{Gateway, Timings};
