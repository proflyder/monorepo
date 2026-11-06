//! Wake word arbitration between pulsar stations on one LAN: every station that hears the wake
//! word broadcasts a claim with its level (UDP) and waits a short window for its peers' claims;
//! only the best claim answers. Yandex does the same in its cloud (TSpotterValidation /
//! multiactivation in maind), which is unavailable here.
//!
//! The wire format and the decision are pure functions (`wire`, `strategy`); the actor owns the
//! broadcast socket and the timers. There is no own thread: the actor runs on the daemon's
//! `current_thread` runtime.

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

/// Id of one claim, returned by `ArbiterHandle::claim` and echoed in `Decided`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ClaimId(u64);

impl fmt::Display for ClaimId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Claim {
    /// DeviceIdentity::name of the sender.
    pub station: String,
    /// Wake word id: "alisa" / "yandex".
    pub wake: String,
    /// The sender is answering / announcing / ringing a timer: keeps the turn.
    pub busy: bool,
    /// AudioFrontend level, 0.1 dB steps.
    pub level_db: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ArbiterEvent {
    /// `won`: this station answers. `best`: the winning claim (ours when won). `peers`: other
    /// stations whose claim for the same wake word counted.
    Decided {
        claim_id: ClaimId,
        won: bool,
        best: Claim,
        peers: usize,
    },
}

/// A socket or one of its options could not be prepared; `spawn` then answers alone.
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

/// Where a claim goes: the broadcast actor, or the solo strategy when the socket is unavailable.
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
    /// Whether the socket opened in `spawn`; false = every claim wins alone.
    pub fn ok(&self) -> bool {
        matches!(self.link, Link::Actor(_))
    }

    /// Broadcasts a claim (3 copies, 40 ms apart), replacing any pending one. With `decide`,
    /// `Decided` with the returned id follows after the window; without, the claim only keeps
    /// the peers quiet (busy claim while a timer rings). Ids grow from 1.
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

    /// Drops the pending decision and the copies not yet sent. A `Decided` the actor already
    /// queued may still arrive after this call: the caller forgets the claim id on cancel and
    /// matches events by id.
    pub fn cancel(&self) {
        if let Link::Actor(cmds) = &self.link {
            let _ = cmds.send(actor::Cmd::Cancel);
        }
    }
}

/// Listens for claims on UDP `port` as `station`. If the socket cannot be opened (logged as a
/// warning), `ok()` is false and every deciding claim wins at once with no peers.
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
