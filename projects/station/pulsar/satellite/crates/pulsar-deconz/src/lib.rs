//! The deCONZ coordinator protocol of the Station Midi's Zigbee radio.
//!
//! Home Assistant (ZHA, zigpy-deconz) and Zigbee2MQTT (adapter "deconz") see the radio as a
//! ConBee/RaspBee coordinator: SLIP frames with a checksum ([`slip`]), the ConBee command and
//! parameter table ([`params`]), and one TCP client at a time ([`server::Server`]). The host
//! chooses the network: PAN ID, extended PAN ID, key, channel, coordinator IEEE address and frame
//! counter are written before switching the network state to CONNECTED; parameters the host does
//! not write keep the radio's current values. Only the global trust center link key is supported
//! (no install codes).
//!
//! # Ownership and hooks
//!
//! The server is driven by its owner (the Zigbee service): `w = server.next().await` followed by
//! `server.handle(w, &mut coord)`. It talks to the coordinator through [`types::Ncp`] and is
//! itself the coordinator's [`types::Hooks`]:
//!
//! - hooks the C++ server ran synchronously (`received`, the `form` result) run inside the call;
//! - hooks the C++ coordinator posted to the loop (send results, its own ZDO answers) run from
//!   the coordinator's next `next()` / `handle()`: after the current dispatch, before the next
//!   link event. The deCONZ server therefore does not need to know about the link at all.
//!
//! `pulsar-deconz` has no dependency on the zigbee crate. The transport is any
//! [`tokio::io::AsyncRead`] + [`tokio::io::AsyncWrite`] connection from an [`server::Acceptor`]
//! (`TcpAcceptor` in production, an in-memory acceptor in tests), so the whole protocol is
//! exercised over `tokio::io::duplex`.
//!
//! Client quirks the server is built around are facts about the clients, see
//! `docs/zigbee.md`, «Клиенты deCONZ».
#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

pub mod host;
pub mod params;
pub mod server;
pub mod slip;
pub mod types;

pub use server::{Acceptor, Conn, Server, TcpAcceptor, Wake};
pub use types::{
    ApsFrame, Hooks, Ncp, Network, NetworkConfig, NetworkKey, STATUS_NO_ACK, SimpleDescriptor,
    get_descriptor, put_descriptor,
};
