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
