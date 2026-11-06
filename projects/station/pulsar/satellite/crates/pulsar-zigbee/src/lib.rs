//! The Station Midi's Zigbee radio in one of its roles, and the orchestration around it.
//!
//! The radio is a Telink B91 on `/dev/ttyS2` (115200 8N1, no flow control) whose reset and boot
//! lines the kernel's `zb_control` driver exports as sysfs value files. It runs one of pulsar's
//! firmwares (`radio/`): the coordinator (a network co-processor for ZHA and Zigbee2MQTT) or the
//! router (a repeater in somebody else's network).
//!
//! # Internal API
//!
//! Everything below runs inside the Zigbee service's actor, on the runtime thread; nothing here
//! spawns tasks. The pieces mirror the C++ objects, with the C++ callbacks turned around:
//!
//! - `pulsar-zbhci`'s [`Link`] is the serial link. The service opens it, and while identifying
//!   the firmware it owns the link itself (boot → 0x0900, frame 0x8900 → firmware version).
//! - [`coordinator::Coordinator`] and [`router::Router`] take the link over by value (the C++
//!   role objects replaced the link callbacks in `start()`); events the link has already queued
//!   then go to the role object. Their actor loop is `w = role.next().await` (cancel-safe, may
//!   sit in a `select!` beside other branches) followed by `role.handle(w, …)`.
//! - The coordinator's C++ hooks are `pulsar_deconz::Hooks`, passed into every call that can run
//!   them, with the coordinator itself as an argument. The deCONZ server calls back into it
//!   (`form`, `send`, …) from inside a hook, as the C++ server did. Hooks the C++ code ran
//!   synchronously (`changed`, the `form` result, received indications) run inside the call;
//!   hooks it posted to the loop (send results, the coordinator's own ZDO answers) run from the
//!   next `next()` / `handle()`: after the current dispatch, before the next link event. (C++
//!   ran them after the whole batch of frames read in one go; frames of one read are here
//!   dispatched one per `next()`, with the posted hooks in between.)
//! - Wiring: the deCONZ server implements `pulsar_deconz::Hooks` (it may call
//!   `coord.form(&cfg, self)` from inside `changed`); the service wraps it and keeps a `changed`
//!   flag that its `Hooks::changed` sets, and after each `coord.handle(w, …)` reads the flag to
//!   publish its own status.
//! - The router reports C++ `changed()` as the `bool` returned by [`router::Router::join`] and
//!   [`router::Router::handle`].
//! - [`flasher::radio_flash`] is blocking (~25 s for an image) and has no stop flag: run it on
//!   its own thread after dropping the link; at shutdown the owner does not wait for it (the
//!   thread is detached; an upload cut short leaves a silent radio that the next start writes
//!   again).
//!
//! The crate is the orchestration: roles, coordinator, router, flashing and the service. The
//! zbhci transport lives in `pulsar-zbhci`, the deCONZ protocol in `pulsar-deconz`. The
//! controller builds [`ZigbeeConfig`]; this crate knows nothing about `pulsar-model`.
//!
//! Roles (`state.conf` `zigbee_role=`): `off` (the radio is not touched; the default), `router`
//! (a repeater in somebody else's network) and `pulsar-coordinator` (the deCONZ server).
//!
//! Service ports: [`Radio`] opens the link, [`Flasher`] writes an image; tests replace both.
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

/// The radio's tty and its `zb_control` reset / boot value files (active low, 1 asserted).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RadioPaths {
    pub tty: String,
    pub reset: String,
    pub boot: String,
}
