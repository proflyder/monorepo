//! The UDP actor: broadcast socket, claim copies and the decision window. Peer claims are parsed
//! and ordered by the pure `wire`/`strategy` modules; timers follow the C++ `WakeArbiter` loop.

use std::cell::Cell;
use std::io::{self, Read as _};
use std::net::{Ipv4Addr, SocketAddrV4};
use std::rc::Rc;

use pulsar_core::Timers;
use socket2::{Domain, SockRef, Socket, Type};
use tokio::net::UdpSocket;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::time::Instant;
use tracing::{debug, info};

use crate::strategy::{Received, decision};
use crate::wire;
use crate::{ArbiterEvent, ArbiterHandle, ArbiterSocketError, Claim, ClaimId, Link};

/// Spotter confirmations of one utterance on stations a few metres apart land within ~100 ms of
/// each other; the window adds Wi-Fi latency on top. It delays the earcon by as much.
pub const WINDOW_MS: i64 = 250;
/// A peer may confirm (and claim) before we do.
pub const LOOKBACK_MS: i64 = 500;
/// Broadcast over Wi-Fi is sent at a basic rate without ACKs or retries.
pub const RESEND_MS: i64 = 40;
pub const COPIES: i32 = 3;
pub const KEEP_MS: i64 = 2000;
/// Copies of one claim arrive within this of each other.
pub const DEDUP_MS: i64 = RESEND_MS * COPIES as i64 * 2;
/// recv buffer of the C++ arbiter; 255 bytes and more are not a claim.
pub const RX_BUF: usize = 256;
/// Queued datagrams handled before a timer fires.
const DRAIN_MAX: usize = 64;
/// Peer claims kept at most (one per station; more only from a flood of names).
pub const MAX_RECENT: usize = 32;

/// `tracing` target of the per-claim diagnostics `lan_probe` reads.
pub const CLAIM_TARGET: &str = "pulsar_arbiter::claim";

pub enum Cmd {
    Claim {
        id: ClaimId,
        wake: String,
        level_db: f32,
        busy: bool,
        decide: bool,
    },
    Cancel,
}

/// Opens the broadcast socket.
pub fn open(port: u16) -> Result<UdpSocket, ArbiterSocketError> {
    let sock = Socket::new(Domain::IPV4, Type::DGRAM.nonblocking(), None)
        .map_err(ArbiterSocketError::Socket)?;
    // SO_REUSEADDR: a second binary on the same station (a probe on another API port) also
    // hears the claims.
    sock.set_reuse_address(true)
        .map_err(ArbiterSocketError::ReuseAddress)?;
    sock.set_broadcast(true)
        .map_err(ArbiterSocketError::Broadcast)?;
    sock.bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port).into())
        .map_err(|source| ArbiterSocketError::Bind { port, source })?;
    UdpSocket::from_std(sock.into()).map_err(ArbiterSocketError::Socket)
}

/// Starts the actor on an open socket; claims go to `dest`.
pub fn launch(
    station: String,
    sock: UdpSocket,
    dest: SocketAddrV4,
) -> (ArbiterHandle, UnboundedReceiver<ArbiterEvent>) {
    let (events, events_rx) = unbounded_channel();
    let arbiter = Arbiter::new(station, sock, dest, events);
    let (cmds, cmds_rx) = home_actor::channel();
    pulsar_core::supervisor().spawn_local("arbiter", arbiter.run(cmds_rx));
    let handle = ArbiterHandle {
        next_id: Rc::new(Cell::new(0)),
        link: Link::Actor(cmds),
    };
    (handle, events_rx)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Timer {
    Resend,
    Decide,
}

pub struct Arbiter {
    station: String,
    sock: UdpSocket,
    /// 255.255.255.255:<port>
    dest: SocketAddrV4,
    /// One entry per peer station: its latest claim.
    pub recent: Vec<Received>,
    mine: Claim,
    /// `mine` as sent; None when too long for the C++ arbiter's 128-byte buffer (never sent).
    datagram: Option<String>,
    claim_ms: i64,
    copies_left: i32,
    /// Claim id awaiting its decision.
    pending: Option<ClaimId>,
    pub timers: Timers<Timer>,
    epoch: Instant,
    events: UnboundedSender<ArbiterEvent>,
    /// Test-only tap of accepted peer claims; production diagnostics go through `tracing`.
    #[cfg(test)]
    trace: Option<UnboundedSender<Claim>>,
}

impl Arbiter {
    pub fn new(
        station: String,
        sock: UdpSocket,
        dest: SocketAddrV4,
        events: UnboundedSender<ArbiterEvent>,
    ) -> Self {
        Self {
            mine: Claim {
                station: station.clone(),
                wake: String::new(),
                busy: false,
                level_db: -100.0,
            },
            station,
            sock,
            dest,
            recent: Vec::new(),
            datagram: None,
            claim_ms: 0,
            copies_left: 0,
            pending: None,
            timers: Timers::new(),
            epoch: Instant::now(),
            events,
            #[cfg(test)]
            trace: None,
        }
    }

    #[cfg(test)]
    pub fn trace_claims(&mut self, trace: UnboundedSender<Claim>) {
        self.trace = Some(trace);
    }

    pub async fn run(mut self, mut cmds: UnboundedReceiver<Cmd>) {
        let mut buf = [0u8; RX_BUF];
        loop {
            tokio::select! {
                biased;
                key = self.timers.next() => self.on_timer(key),
                cmd = cmds.recv() => match cmd {
                    Some(cmd) => self.on_cmd(cmd),
                    None => break,
                },
                r = self.sock.recv(&mut buf) => self.on_datagram(r, &buf),
            }
        }
    }

    pub fn on_timer(&mut self, key: Timer) {
        // The C++ loop served ready sockets before due timers: a claim already queued must
        // count before our next copy and before the decision.
        self.drain();
        match key {
            Timer::Resend => self.resend(),
            Timer::Decide => self.decide(),
        }
    }

    /// Handles the datagrams already queued on the socket (at most `DRAIN_MAX`). A plain
    /// recv: tokio's readiness may not have seen them yet.
    fn drain(&mut self) {
        let mut buf = [0u8; RX_BUF];
        for _ in 0..DRAIN_MAX {
            let r = (&*SockRef::from(&self.sock)).read(&mut buf);
            match r {
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => return,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                r => self.on_datagram(r, &buf),
            }
        }
    }

    pub fn on_cmd(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Claim {
                id,
                wake,
                level_db,
                busy,
                decide,
            } => self.claim(id, wake, level_db, busy, decide),
            Cmd::Cancel => self.cancel(),
        }
    }

    /// Milliseconds on the monotonic clock (tokio's, so tests can pause it).
    fn now_ms(&self) -> i64 {
        (Instant::now() - self.epoch).as_millis() as i64
    }

    fn claim(&mut self, id: ClaimId, wake: String, level_db: f32, busy: bool, decide: bool) {
        self.cancel();
        self.mine = Claim {
            station: self.station.clone(),
            wake,
            busy,
            level_db,
        };
        self.datagram = wire::format(&self.mine);
        self.claim_ms = self.now_ms();
        self.send();
        self.copies_left = COPIES - 1;
        self.timers
            .set_repeating(Timer::Resend, RESEND_MS as u64, RESEND_MS as u64);
        if decide {
            self.pending = Some(id);
            self.timers.set(Timer::Decide, WINDOW_MS as u64);
        }
    }

    fn cancel(&mut self) {
        self.timers.cancel(Timer::Resend);
        self.timers.cancel(Timer::Decide);
        self.pending = None;
    }

    fn resend(&mut self) {
        self.send();
        self.copies_left -= 1;
        if self.copies_left <= 0 {
            self.timers.cancel(Timer::Resend);
        }
    }

    fn send(&self) {
        let Some(d) = &self.datagram else { return };
        // A plain sendto: tokio's try_send_to drops datagrams until the reactor has first seen
        // the socket writable.
        if let Err(e) = SockRef::from(&self.sock).send_to(d.as_bytes(), &self.dest.into()) {
            debug!("arbiter: sendto: {}", home_linux::io_error_text(&e));
        }
    }

    pub fn on_datagram(&mut self, r: io::Result<usize>, buf: &[u8]) {
        let n = match r {
            Ok(n) => n,
            Err(e) => {
                if e.kind() != io::ErrorKind::Interrupted {
                    debug!("arbiter: recv: {}", home_linux::io_error_text(&e));
                }
                return;
            }
        };
        let Some(c) = accept(&buf[..n], &self.station) else {
            return;
        };
        let now = self.now_ms();
        let resent = self.recent.iter().any(|r| {
            r.claim.station == c.station
                && r.claim.wake == c.wake
                && r.claim.busy == c.busy
                && r.claim.level_db == c.level_db
                && now - r.at_ms <= DEDUP_MS
        });
        if resent {
            return;
        }
        self.recent
            .retain(|r| now - r.at_ms <= KEEP_MS && r.claim.station != c.station);
        info!(
            target: CLAIM_TARGET,
            station = c.station.as_str(),
            wake = c.wake.as_str(),
            busy = c.busy,
            level_db = f64::from(c.level_db),
            "claim"
        );
        #[cfg(test)]
        if let Some(trace) = &self.trace {
            let _ = trace.send(c.clone());
        }
        self.recent.push(Received {
            claim: c,
            at_ms: now,
        });
        // Bounds a flood of distinct station names; the oldest claim goes first.
        if self.recent.len() > MAX_RECENT {
            self.recent.remove(0);
        }
    }

    fn decide(&mut self) {
        let Some(claim_id) = self.pending.take() else {
            return;
        };
        let (won, best, peers) = decision(
            &self.mine,
            self.claim_ms,
            &self.recent,
            WINDOW_MS,
            LOOKBACK_MS,
        );
        let _ = self.events.send(ArbiterEvent::Decided {
            claim_id,
            won,
            best: best.clone(),
            peers,
        });
    }
}

/// A received datagram (cut to the receive buffer) as a peer's claim: a datagram that fills the
/// buffer may have been cut, and our own broadcasts loop back.
pub fn accept(datagram: &[u8], own_station: &str) -> Option<Claim> {
    if datagram.len() >= RX_BUF - 1 {
        return None;
    }
    wire::parse(datagram).filter(|c| c.station != own_station)
}
