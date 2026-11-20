#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

pub mod coordinator;
pub mod flasher;
mod roles;
pub mod router;
mod service;
#[cfg(test)]
mod test_util;

pub use pulsar_zbhci::bytes;
pub use pulsar_zbhci::link::Link;
pub use roles::{Mode, ROLES, Role, parse_mode, role};
pub use service::{
    Decision, Flasher, HardwareFlasher, HardwareRadio, Radio, Status, ZigbeeConfig, ZigbeeEvent,
    ZigbeeHandle, decide, spawn,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RadioPaths {
    pub tty: String,
    pub reset: String,
    pub boot: String,
}
