//! Probe of the LAN services next to a running daemon: use alternative arbitration ports and the
//! `_pulsartest._tcp` service type only (docs/stations.md).
//!
//!   lan_probe mdns <host> <service> <port> <seconds> [txt...]
//!       responder for <host>.local with one service; prints every question it answered
//!   lan_probe claim <station> <port> <wake> <level> [busy]
//!       claims at once, prints the decision
//!   lan_probe claim-on-peer <station> <port> <wake> <level> [timeout_s=60] [delay_ms=0]
//!       claims as soon as another station's claim arrives (same 250 ms window without
//!       synchronising the starts), or delay_ms later (300 = the early-claim rule: the peer's
//!       claim is then older than the window but within the 500 ms lookback); prints the decision
//!   lan_probe listen <port> <seconds>
//!       prints every claim received
#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::io::Write as _;
use std::process::ExitCode;
use std::time::Duration;

use pulsar_arbiter::{ArbiterEvent, Claim, spawn};
use pulsar_mdns::{MdnsService, spawn as mdns_spawn};
use tokio::signal::unix::{Signal, SignalKind, signal};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::time::{Instant, sleep, timeout};
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;

const USAGE: &str = "usage:
  lan_probe mdns <host> <service> <port> <seconds> [txt...]
  lan_probe claim <station> <port> <wake> <level> [busy]
  lan_probe claim-on-peer <station> <port> <wake> <level> [timeout_s=60] [delay_ms=0]
  lan_probe listen <port> <seconds>";

/// A decision comes 250 ms after the claim.
const DECISION_WAIT: Duration = Duration::from_secs(5);
/// Peer claims are still printed this long after the decision.
const PEERS_AFTER: Duration = Duration::from_secs(1);
const MDNS_ANSWERED: &str = "pulsar_mdns::answered";
const ARBITER_CLAIM: &str = "pulsar_arbiter::claim";

/// println! that ignores a closed stdout (println! would abort on EPIPE).
macro_rules! say {
    ($($arg:tt)*) => {{
        let _ = writeln!(std::io::stdout(), $($arg)*);
    }};
}

/// Diagnostics captured from `tracing`: the services stay free of probe-only channels.
enum Diag {
    Answered {
        from: String,
        name: String,
        qtype: u16,
        reply: String,
    },
    Claim(Claim),
}

#[derive(Default)]
struct Fields {
    from: Option<String>,
    name: Option<String>,
    qtype: Option<u64>,
    reply: Option<String>,
    station: Option<String>,
    wake: Option<String>,
    busy: Option<bool>,
    level_db: Option<f64>,
}

impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "from" => self.from = Some(value.to_owned()),
            "name" => self.name = Some(value.to_owned()),
            "reply" => self.reply = Some(value.to_owned()),
            "station" => self.station = Some(value.to_owned()),
            "wake" => self.wake = Some(value.to_owned()),
            _ => {}
        }
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        if field.name() == "qtype" {
            self.qtype = Some(value);
        }
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        if field.name() == "level_db" {
            self.level_db = Some(value);
        }
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        if field.name() == "busy" {
            self.busy = Some(value);
        }
    }

    fn record_debug(&mut self, _field: &Field, _value: &dyn std::fmt::Debug) {}
}

struct Capture {
    tx: UnboundedSender<Diag>,
}

impl<S: Subscriber> Layer<S> for Capture {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let target = event.metadata().target();
        let mut f = Fields::default();
        event.record(&mut f);
        let diag = if target == MDNS_ANSWERED && f.from.is_some() {
            Diag::Answered {
                from: f.from.unwrap_or_default(),
                name: f.name.unwrap_or_default(),
                qtype: f.qtype.unwrap_or(0) as u16,
                reply: f.reply.unwrap_or_default(),
            }
        } else if target == ARBITER_CLAIM && f.station.is_some() {
            Diag::Claim(Claim {
                station: f.station.unwrap_or_default(),
                wake: f.wake.unwrap_or_default(),
                busy: f.busy.unwrap_or(false),
                level_db: f.level_db.unwrap_or(-100.0) as f32,
            })
        } else {
            return;
        };
        let _ = self.tx.send(diag);
    }
}

enum Mode {
    Mdns {
        host: String,
        service: String,
        port: u16,
        seconds: u64,
        txt: Vec<String>,
    },
    Claim {
        station: String,
        port: u16,
        wake: String,
        level: f32,
        busy: bool,
    },
    ClaimOnPeer {
        station: String,
        port: u16,
        wake: String,
        level: f32,
        timeout_s: u64,
        delay_ms: u64,
    },
    Listen {
        port: u16,
        seconds: u64,
    },
}

fn parse_args(a: &[String]) -> Option<Mode> {
    let s = |i: usize| a.get(i).cloned();
    let num = |i: usize| a.get(i)?.parse().ok();
    match (a.first()?.as_str(), a.len()) {
        ("mdns", 5..) => Some(Mode::Mdns {
            host: s(1)?,
            service: s(2)?,
            port: a[3].parse().ok()?,
            seconds: num(4)?,
            txt: a[5..].to_vec(),
        }),
        ("claim", 5..=6) => Some(Mode::Claim {
            station: s(1)?,
            port: a[2].parse().ok()?,
            wake: s(3)?,
            level: a[4].parse().ok()?,
            busy: match a.get(5).map(String::as_str) {
                None | Some("0") => false,
                Some("1" | "busy") => true,
                Some(_) => return None,
            },
        }),
        ("claim-on-peer", 5..=7) => Some(Mode::ClaimOnPeer {
            station: s(1)?,
            port: a[2].parse().ok()?,
            wake: s(3)?,
            level: a[4].parse().ok()?,
            timeout_s: if a.len() > 5 { num(5)? } else { 60 },
            delay_ms: if a.len() > 6 { num(6)? } else { 0 },
        }),
        ("listen", 3) => Some(Mode::Listen {
            port: a[1].parse().ok()?,
            seconds: num(2)?,
        }),
        _ => None,
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(mode) = parse_args(&args) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let (diag_tx, diag) = unbounded_channel();
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_filter(LevelFilter::WARN),
        )
        .with(Capture { tx: diag_tx })
        .init();
    // Как в демоне: см. docs/architecture.md, «Модель исполнения», п. 1.
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(4)
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("tokio runtime: {e}");
            return ExitCode::FAILURE;
        }
    };
    let local = tokio::task::LocalSet::new();
    let code = local.block_on(&rt, run(mode, diag));
    // The responder's goodbye goes out when its actor stops.
    local.block_on(
        &rt,
        pulsar_core::supervisor().shutdown(Duration::from_secs(3)),
    );
    {
        let _rt = rt.enter();
        drop(local);
    }
    rt.shutdown_timeout(Duration::from_secs(1));
    ExitCode::from(code)
}

async fn run(mode: Mode, mut diag: UnboundedReceiver<Diag>) -> u8 {
    match mode {
        Mode::Mdns {
            host,
            service,
            port,
            seconds,
            txt,
        } => run_mdns(host, service, port, seconds, txt, &mut diag).await,
        Mode::Claim {
            station,
            port,
            wake,
            level,
            busy,
        } => {
            let (h, mut events) = spawn(station.clone(), port);
            let id = h.claim(&wake, level, busy, true);
            let claimed = Instant::now();
            say!(
                "claim {id}: {station} wake={wake} busy={} level={level:.1}",
                u8::from(busy)
            );
            let decided = report_decision(&mut events, &mut diag, claimed).await;
            if decided && h.ok() { 0 } else { 1 }
        }
        Mode::ClaimOnPeer {
            station,
            port,
            wake,
            level,
            timeout_s,
            delay_ms,
        } => {
            let (h, mut events) = spawn(station.clone(), port);
            if !h.ok() {
                return 1;
            }
            say!("waiting {timeout_s} s for a peer claim on udp :{port}");
            let peer = timeout(Duration::from_secs(timeout_s), next_claim(&mut diag)).await;
            let Ok(Some(peer)) = peer else {
                say!("no peer claim");
                return 1;
            };
            if delay_ms > 0 {
                sleep(Duration::from_millis(delay_ms)).await;
            }
            let id = h.claim(&wake, level, false, true);
            let claimed = Instant::now();
            say!("peer {}", describe(&peer));
            say!("claim {id} after {delay_ms} ms: {station} wake={wake} busy=0 level={level:.1}");
            if report_decision(&mut events, &mut diag, claimed).await {
                0
            } else {
                1
            }
        }
        Mode::Listen { port, seconds } => {
            let (h, _events) = spawn(String::new(), port);
            if !h.ok() {
                return 1;
            }
            say!("listening on udp :{port} for {seconds} s");
            let start = Instant::now();
            let n = print_for(&mut diag, seconds, |d| {
                if let Diag::Claim(c) = d {
                    say!("+{} ms {}", start.elapsed().as_millis(), describe(&c));
                }
            })
            .await;
            say!("{n} claims");
            0
        }
    }
}

async fn run_mdns(
    host: String,
    service: String,
    port: u16,
    seconds: u64,
    txt: Vec<String>,
    diag: &mut UnboundedReceiver<Diag>,
) -> u8 {
    let h = match mdns_spawn(&host) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    h.add_service(MdnsService {
        service,
        instance: host.clone(),
        port,
        txt,
    });
    say!("mdns: advertising {host}.local for {seconds} s");
    let n = print_for(diag, seconds, |d| {
        if let Diag::Answered {
            from,
            name,
            qtype,
            reply,
        } = d
        {
            say!("answered {name} {} from {from}: {reply}", qtype_name(qtype));
        }
    })
    .await;
    say!("mdns: {n} questions answered, sending goodbye");
    0
}

fn qtype_name(t: u16) -> String {
    match t {
        1 => "A".into(),
        12 => "PTR".into(),
        16 => "TXT".into(),
        33 => "SRV".into(),
        255 => "ANY".into(),
        t => format!("TYPE{t}"),
    }
}

fn describe(c: &Claim) -> String {
    format!(
        "{} wake={} busy={} level={:.1}",
        c.station,
        c.wake,
        u8::from(c.busy),
        c.level_db
    )
}

/// Waits for the next peer claim; other diagnostics are skipped.
async fn next_claim(diag: &mut UnboundedReceiver<Diag>) -> Option<Claim> {
    while let Some(d) = diag.recv().await {
        if let Diag::Claim(c) = d {
            return Some(c);
        }
    }
    None
}

/// Prints the decision and every peer claim received until 1 s after it, with its time
/// relative to our claim: a peer that missed the window shows up late instead of not at all.
async fn report_decision(
    events: &mut UnboundedReceiver<ArbiterEvent>,
    diag: &mut UnboundedReceiver<Diag>,
    claimed: Instant,
) -> bool {
    let mut decided = false;
    let end = sleep(DECISION_WAIT);
    tokio::pin!(end);
    loop {
        tokio::select! {
            biased;
            Some(d) = diag.recv() => {
                if let Diag::Claim(c) = d {
                    let ms = claimed.elapsed().as_millis();
                    say!("peer +{ms} ms after own claim: {}", describe(&c));
                }
            }
            Some(ArbiterEvent::Decided { claim_id, won, best, peers }) = events.recv(),
                if !decided =>
            {
                let ms = claimed.elapsed().as_millis();
                say!(
                    "Decided claim {claim_id} (+{ms} ms): {} best={} peers={peers}",
                    if won { "won" } else { "lost" },
                    describe(&best)
                );
                decided = true;
                end.as_mut().reset(Instant::now() + PEERS_AFTER);
            }
            () = &mut end => break,
        }
    }
    if !decided {
        say!("no decision");
    }
    decided
}

/// Prints every diagnostic of `rx` until `seconds` pass or SIGINT / SIGTERM arrives; returns how
/// many were printed.
async fn print_for(
    rx: &mut UnboundedReceiver<Diag>,
    seconds: u64,
    mut print: impl FnMut(Diag),
) -> usize {
    let mut term = signal(SignalKind::terminate()).ok();
    let mut int = signal(SignalKind::interrupt()).ok();
    let deadline = sleep(Duration::from_secs(seconds));
    tokio::pin!(deadline);
    let mut n = 0;
    loop {
        tokio::select! {
            () = &mut deadline => break,
            Some(()) = next_signal(&mut term) => break,
            Some(()) = next_signal(&mut int) => break,
            Some(item) = rx.recv() => {
                n += 1;
                print(item);
            }
        }
    }
    n
}

/// Pending forever when the handler could not be installed.
async fn next_signal(s: &mut Option<Signal>) -> Option<()> {
    match s {
        Some(s) => s.recv().await,
        None => std::future::pending().await,
    }
}
