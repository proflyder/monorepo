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

pub const WINDOW_MS: i64 = 250;
pub const LOOKBACK_MS: i64 = 500;
pub const RESEND_MS: i64 = 40;
pub const COPIES: i32 = 3;
pub const KEEP_MS: i64 = 2000;
pub const DEDUP_MS: i64 = RESEND_MS * COPIES as i64 * 2;
pub const RX_BUF: usize = 256;
const DRAIN_MAX: usize = 64;
pub const MAX_RECENT: usize = 32;

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

pub fn open(port: u16) -> Result<UdpSocket, ArbiterSocketError> {
    let sock = Socket::new(Domain::IPV4, Type::DGRAM.nonblocking(), None)
        .map_err(ArbiterSocketError::Socket)?;
    sock.set_reuse_address(true)
        .map_err(ArbiterSocketError::ReuseAddress)?;
    sock.set_broadcast(true)
        .map_err(ArbiterSocketError::Broadcast)?;
    sock.bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port).into())
        .map_err(|source| ArbiterSocketError::Bind { port, source })?;
    UdpSocket::from_std(sock.into()).map_err(ArbiterSocketError::Socket)
}

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
    dest: SocketAddrV4,
    pub recent: Vec<Received>,
    mine: Claim,
    datagram: Option<String>,
    claim_ms: i64,
    copies_left: i32,
    pending: Option<ClaimId>,
    pub timers: Timers<Timer>,
    epoch: Instant,
    events: UnboundedSender<ArbiterEvent>,
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
        self.drain();
        match key {
            Timer::Resend => self.resend(),
            Timer::Decide => self.decide(),
        }
    }

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

pub fn accept(datagram: &[u8], own_station: &str) -> Option<Claim> {
    if datagram.len() >= RX_BUF - 1 {
        return None;
    }
    wire::parse(datagram).filter(|c| c.station != own_station)
}
