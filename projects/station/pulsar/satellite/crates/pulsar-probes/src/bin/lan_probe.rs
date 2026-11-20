#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::io::Write as _;
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand};
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

const DECISION_WAIT: Duration = Duration::from_secs(5);
const PEERS_AFTER: Duration = Duration::from_secs(1);
const MDNS_ANSWERED: &str = "pulsar_mdns::answered";
const ARBITER_CLAIM: &str = "pulsar_arbiter::claim";

macro_rules! say {
    ($($arg:tt)*) => {{
        let _ = writeln!(std::io::stdout(), $($arg)*);
    }};
}

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

fn parse_busy(s: &str) -> Result<bool, String> {
    match s {
        "0" | "false" => Ok(false),
        "1" | "true" | "busy" => Ok(true),
        _ => Err(format!("busy: {s}")),
    }
}

#[derive(Subcommand)]
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
        #[arg(value_parser = parse_busy, default_value = "0")]
        busy: bool,
    },
    ClaimOnPeer {
        station: String,
        port: u16,
        wake: String,
        level: f32,
        #[arg(default_value_t = 60)]
        timeout_s: u64,
        #[arg(default_value_t = 0)]
        delay_ms: u64,
    },
    Listen {
        port: u16,
        seconds: u64,
    },
}

#[derive(Parser)]
#[command(about = "LAN arbitration and mDNS probe")]
struct Cli {
    #[command(subcommand)]
    mode: Mode,
}

fn main() -> ExitCode {
    let mode = Cli::parse().mode;
    let (diag_tx, diag) = unbounded_channel();
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_filter(LevelFilter::WARN),
        )
        .with(Capture { tx: diag_tx })
        .init();
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

async fn next_claim(diag: &mut UnboundedReceiver<Diag>) -> Option<Claim> {
    while let Some(d) = diag.recv().await {
        if let Diag::Claim(c) = d {
            return Some(c);
        }
    }
    None
}

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

async fn next_signal(s: &mut Option<Signal>) -> Option<()> {
    match s {
        Some(s) => s.recv().await,
        None => std::future::pending().await,
    }
}
