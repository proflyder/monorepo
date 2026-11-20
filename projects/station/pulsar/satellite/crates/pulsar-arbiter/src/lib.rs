mod actor;
mod strategy;
#[cfg(test)]
mod tests;
mod wire;

use std::cell::Cell;
use std::fmt;
use std::io;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::rc::Rc;

use home_actor::Handle;
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ClaimId(u64);

impl ClaimId {
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for ClaimId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Claim {
    pub station: String,
    pub wake: String,
    pub busy: bool,
    pub level_db: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ArbiterEvent {
    Decided {
        claim_id: ClaimId,
        won: bool,
        best: Claim,
        peers: usize,
    },
}

#[derive(Debug, Error)]
pub enum ArbiterSocketError {
    #[error("arbiter: socket: {}", home_linux::io_error_text(.0))]
    Socket(#[source] io::Error),
    #[error("arbiter: SO_REUSEADDR: {}", home_linux::io_error_text(.0))]
    ReuseAddress(#[source] io::Error),
    #[error("arbiter: SO_BROADCAST: {}", home_linux::io_error_text(.0))]
    Broadcast(#[source] io::Error),
    #[error("arbiter: bind :{port}: {}", home_linux::io_error_text(source))]
    Bind {
        port: u16,
        #[source]
        source: io::Error,
    },
}

#[derive(Clone)]
pub(crate) enum Link {
    Actor(Handle<actor::Cmd>),
    Solo(strategy::Solo),
}

#[derive(Clone)]
pub struct ArbiterHandle {
    next_id: Rc<Cell<u64>>,
    link: Link,
}

impl ArbiterHandle {
    pub fn ok(&self) -> bool {
        matches!(self.link, Link::Actor(_))
    }

    pub fn claim(&self, wake: &str, level_db: f32, busy: bool, decide: bool) -> ClaimId {
        let id = ClaimId(self.next_id.get() + 1);
        self.next_id.set(id.0);
        match &self.link {
            Link::Actor(cmds) => {
                let _ = cmds.send(actor::Cmd::Claim {
                    id,
                    wake: wake.to_owned(),
                    level_db,
                    busy,
                    decide,
                });
            }
            Link::Solo(solo) => solo.claim(id, wake, level_db, busy, decide),
        }
        id
    }

    pub fn cancel(&self) {
        if let Link::Actor(cmds) = &self.link {
            let _ = cmds.send(actor::Cmd::Cancel);
        }
    }
}

pub fn spawn(
    station: String,
    port: u16,
) -> (
    ArbiterHandle,
    tokio::sync::mpsc::UnboundedReceiver<ArbiterEvent>,
) {
    match actor::open(port) {
        Ok(sock) => {
            tracing::info!("arbiter: listening on udp :{port} as '{station}'");
            actor::launch(station, sock, SocketAddrV4::new(Ipv4Addr::BROADCAST, port))
        }
        Err(e) => {
            tracing::warn!("{e}; this station answers without arbitration");
            let (events, events_rx) = tokio::sync::mpsc::unbounded_channel();
            let handle = ArbiterHandle {
                next_id: Rc::new(Cell::new(0)),
                link: Link::Solo(strategy::Solo::new(station, events)),
            };
            (handle, events_rx)
        }
    }
}
