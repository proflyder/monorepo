//! The Midi's Zigbee radio in one of its roles:
//!   off:         pulsar leaves the radio alone (whatever firmware it has keeps running); the
//!                default, so an install never writes the radio on its own;
//!   coordinator: pulsar's coordinator firmware (radio/coordinator, installed next to the daemon),
//!                a deCONZ coordinator for ZHA and Zigbee2MQTT (Coordinator + DeconzServer);
//!   router:      pulsar's router firmware (radio/router), a repeater in another network (Router).
//! At start the running firmware is identified (0x0900) before the role object is built. When it
//! is not the role's image (another role, another build: a pulsar update brought a new image), or
//! the radio is silent (an interrupted upload), the role's image is written through the radio's
//! bootloader (flasher). At most two uploads per start; an image the radio did not take is
//! reported (`FlashOutcome`, persisted by the controller as `failed_version`) and not written
//! again on later starts. Each firmware keeps its own network in its own flash area, so
//! switching back and forth keeps them.
//!
//! The actor's state is an explicit enum: every phase owns its data (the starting link, the
//! running role, the progress of an upload), so no branch has to guess what the other fields
//! hold.

use std::cell::{Cell, RefCell};
use std::fmt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use pulsar_core::Timers;
use pulsar_deconz::Server as DeconzServer;
use pulsar_zbhci::RadioError;
use pulsar_zbhci::bytes::get_be32;
use pulsar_zbhci::link::{Link, LinkEvent, SysfsLines};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tracing::{error, info, warn};

use pulsar_deconz::{ApsFrame, Hooks};

use crate::coordinator::Coordinator;
use crate::flasher::{FlashError, radio_flash, ybl_load};
use crate::router::Router;
use crate::{Mode, RadioPaths, role};

/// Reset + boot + 0x0900 normally take < 1 s.
const ANSWER_TIMEOUT_MS: u64 = 10_000;
/// A silent radio is asked again.
const RETRY_MS: u64 = 60_000;
/// Per start: the image, and one retry.
const MAX_FLASHES: u32 = 2;
const APP_INFO_REQ: u16 = 0x0900;
const APP_INFO_RSP: u16 = 0x8900;
/// Yandex's gateway.ybl.
const GATEWAY_FAMILY: u8 = 0x15;

pub struct ZigbeeConfig {
    pub lines: RadioPaths,
    /// zigbee-coordinator.ybl
    pub coordinator_image: PathBuf,
    /// zigbee-router.ybl
    pub router_image: PathBuf,
    pub deconz_port: u16,
    /// Image version given up on in an earlier start (0: none).
    pub failed_version: u32,
}

/// C++ `changed()` and `outcome(failed)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZigbeeEvent {
    /// `status()` / `flashing()` may have changed.
    Changed,
    /// The role runs (0), or the radio did not take image version `failed_version`: persist it.
    FlashOutcome { failed_version: u32 },
}

#[derive(Clone, Copy)]
enum Cmd {
    Join,
    Leave,
}

/// One line for Home Assistant. The text is the Home Assistant status sensor's contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Off,
    Starting,
    Flashing {
        label: &'static str,
        percent: i32,
    },
    Failed {
        error: String,
    },
    Coordinator {
        /// The radio reported its network state.
        ready: bool,
        forming: bool,
        formed: bool,
        pan: u16,
        channel: u8,
    },
    Router {
        ready: bool,
        joining: bool,
        joined: bool,
        pan: u16,
        nwk: u16,
        channel: u8,
        /// Last failure, "" = none.
        error: String,
    },
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Off => f.write_str("off"),
            Self::Starting => f.write_str("starting"),
            Self::Flashing { label, percent } => {
                write!(f, "writing {label} firmware: {percent} %")
            }
            Self::Failed { error } => write!(f, "error: {error}"),
            Self::Coordinator {
                ready,
                forming,
                formed,
                pan,
                channel,
            } => {
                if !ready {
                    return f.write_str("coordinator: radio not ready");
                }
                if *forming {
                    return f.write_str("coordinator: forming a network");
                }
                if !formed {
                    return f.write_str("coordinator: no network");
                }
                write!(f, "coordinator: PAN 0x{pan:04X}, channel {channel}")
            }
            Self::Router {
                ready,
                joining,
                joined,
                pan,
                nwk,
                channel,
                error,
            } => {
                if !ready {
                    return f.write_str("router: radio not ready");
                }
                if *joining {
                    return f.write_str("router: joining");
                }
                if *joined {
                    return write!(
                        f,
                        "router: in network 0x{pan:04X} as 0x{nwk:04X}, channel {channel}"
                    );
                }
                if error.is_empty() {
                    f.write_str("router: not in a network")
                } else {
                    write!(f, "router: {error}")
                }
            }
        }
    }
}

struct Shared {
    status: RefCell<Status>,
    flashing: Cell<bool>,
}

#[derive(Clone)]
pub struct ZigbeeHandle {
    mode: Mode,
    shared: Rc<Shared>,
    cmds: UnboundedSender<Cmd>,
}

impl ZigbeeHandle {
    /// Constant: a role change restarts the daemon.
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// An image is being written (the actor starts it on the radio's answer or its timeout).
    pub fn flashing(&self) -> bool {
        self.shared.flashing.get()
    }

    /// Одна строка для Home Assistant, например «router: in network 0x1A2B as 0x0001, channel 15».
    /// Начальный статус задан синхронно в `spawn`, дальше его меняет актор.
    pub fn status(&self) -> Status {
        self.shared.status.borrow().clone()
    }

    /// Router role only: join the network that currently allows joining. The status changes
    /// once the actor ran the command (then `Changed`), not inside this call as in C++.
    pub fn join(&self) {
        let _ = self.cmds.send(Cmd::Join);
    }

    /// Router role only: leave the current network (no `Changed`, as in C++).
    pub fn leave(&self) {
        let _ = self.cmds.send(Cmd::Leave);
    }
}

/// Opens the radio's link. Tests replace it.
pub trait Radio {
    fn open(&mut self, lines: &RadioPaths) -> Result<Link, RadioError>;
}

/// Writes an image through the bootloader. Runs on its own thread, so it must be shareable.
pub trait Flasher: Send + Sync {
    fn flash(
        &self,
        lines: &RadioPaths,
        image: &[u8],
        progress: &mut dyn FnMut(f32),
    ) -> Result<(), FlashError>;
}

/// The real tty and sysfs lines.
pub struct HardwareRadio;

impl Radio for HardwareRadio {
    fn open(&mut self, lines: &RadioPaths) -> Result<Link, RadioError> {
        Link::open(
            &lines.tty,
            Box::new(SysfsLines::new(lines.reset.clone(), lines.boot.clone())),
        )
    }
}

pub struct HardwareFlasher;

impl Flasher for HardwareFlasher {
    fn flash(
        &self,
        lines: &RadioPaths,
        image: &[u8],
        progress: &mut dyn FnMut(f32),
    ) -> Result<(), FlashError> {
        radio_flash(
            &lines.tty,
            &mut SysfsLines::new(lines.reset.clone(), lines.boot.clone()),
            image,
            progress,
        )
    }
}

/// Starts the role. The initial status ("off", "starting" or "error: cannot open the radio") is
/// there when this returns, with its `Changed` already queued.
pub fn spawn(cfg: ZigbeeConfig, mode: Mode) -> (ZigbeeHandle, UnboundedReceiver<ZigbeeEvent>) {
    spawn_with(
        cfg,
        mode,
        Box::new(HardwareRadio),
        Arc::new(HardwareFlasher),
    )
}

pub fn spawn_with(
    cfg: ZigbeeConfig,
    mode: Mode,
    radio: Box<dyn Radio>,
    flasher: Arc<dyn Flasher>,
) -> (ZigbeeHandle, UnboundedReceiver<ZigbeeEvent>) {
    let (ev_tx, ev_rx) = unbounded_channel();
    let (cmd_tx, cmd_rx) = unbounded_channel();
    let (flash_tx, flash_rx) = unbounded_channel();
    let shared = Rc::new(Shared {
        status: RefCell::new(Status::Off),
        flashing: Cell::new(false),
    });
    let mut svc = Service {
        cfg,
        mode,
        radio,
        flasher,
        shared: shared.clone(),
        events: ev_tx,
        state: State::Off,
        flashes: 0,
        timers: Timers::new(),
        thread: None,
        flash_tx,
    };
    svc.start();
    pulsar_core::supervisor().spawn_local("zigbee", svc.run_actor(cmd_rx, flash_rx));
    (
        ZigbeeHandle {
            mode,
            shared,
            cmds: cmd_tx,
        },
        ev_rx,
    )
}

/// What the actor owns at this moment.
enum State {
    Off,
    /// The radio is open and is being asked which firmware runs.
    Starting(Box<Link>),
    Running(Role),
    Flashing {
        progress: f32,
    },
    Failed {
        error: String,
    },
}

enum Role {
    Coordinator {
        coord: Box<Coordinator>,
        deconz: Box<DeconzServer>,
    },
    Router(Box<Router>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Timer {
    /// The radio must identify its firmware in time.
    Answer,
    /// A silent radio is asked again.
    Retry,
}

enum FlashMsg {
    Progress(f32),
    Done(Result<(), FlashError>),
}

enum Wake {
    Timer(Timer),
    Link(LinkEvent),
    Coord(crate::coordinator::Wake),
    Deconz(pulsar_deconz::Wake),
    Router(crate::router::Wake),
    Flash(FlashMsg),
    Cmd(Option<Cmd>),
}

struct Service {
    cfg: ZigbeeConfig,
    mode: Mode,
    radio: Box<dyn Radio>,
    flasher: Arc<dyn Flasher>,
    shared: Rc<Shared>,
    events: UnboundedSender<ZigbeeEvent>,
    state: State,
    /// Images written since start.
    flashes: u32,
    timers: Timers<Timer>,
    thread: Option<std::thread::JoinHandle<()>>,
    flash_tx: UnboundedSender<FlashMsg>,
}

/// The coordinator's hooks: the deCONZ server's, plus the service's `changed()`.
struct CoordHooks<'a> {
    deconz: &'a mut DeconzServer,
    changed: bool,
}

impl Hooks for CoordHooks<'_> {
    fn received(&mut self, coord: &mut dyn pulsar_deconz::Ncp, f: &ApsFrame) {
        self.deconz.received(coord, f);
    }

    fn sent(&mut self, coord: &mut dyn pulsar_deconz::Ncp, handle: u32, status: u8) {
        self.deconz.sent(coord, handle, status);
    }

    fn changed(&mut self, coord: &mut dyn pulsar_deconz::Ncp) {
        self.deconz.changed(coord);
        self.changed = true;
    }

    fn formed(&mut self, coord: &mut dyn pulsar_deconz::Ncp, ok: bool) {
        self.deconz.formed(coord, ok);
    }
}

/// What to do with the firmware the radio reported (C++ `on_firmware`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// The role's image runs: `FlashOutcome { 0 }`, then the role.
    Run,
    /// Same family, but the role's image was not taken earlier: keep what runs.
    Keep,
    /// Write the role's image.
    Flash,
}

/// `want`: the role's image version, None without a readable image (then only the family
/// counts).
pub fn decide(version: u32, want: Option<u32>, mode: Mode, failed_version: u32) -> Decision {
    // The role's image decides, as maind does it for the gateway: another version of the same
    // firmware is replaced too (a pulsar update brings a new build).
    let same_family = (version >> 24) as u8 == role(mode).family;
    let matches = want.map_or(same_family, |w| version == w);
    if matches {
        Decision::Run
    } else if same_family && want.unwrap_or(0) == failed_version {
        Decision::Keep
    } else {
        Decision::Flash
    }
}

fn family_name(version: u32) -> &'static str {
    match (version >> 24) as u8 {
        GATEWAY_FAMILY => "Yandex's gateway",
        f if f == role(Mode::Router).family => "router",
        f if f == role(Mode::Coordinator).family => "coordinator",
        _ => "unknown",
    }
}

impl Service {
    fn image_path(&self) -> &Path {
        if self.mode == Mode::Router {
            &self.cfg.router_image
        } else {
            &self.cfg.coordinator_image
        }
    }

    /// C++ `changed()`: publish the state, then tell the controller.
    fn changed(&mut self) {
        self.publish();
        let _ = self.events.send(ZigbeeEvent::Changed);
    }

    fn outcome(&mut self, failed_version: u32) {
        let _ = self
            .events
            .send(ZigbeeEvent::FlashOutcome { failed_version });
    }

    /// The getters' state, as C++ `status()` / `flashing()` would answer now.
    fn publish(&self) {
        let status = self.status();
        if *self.shared.status.borrow() != status {
            *self.shared.status.borrow_mut() = status;
        }
        self.shared.flashing.set(self.is_flashing());
    }

    fn is_flashing(&self) -> bool {
        matches!(self.state, State::Flashing { .. })
    }

    fn start(&mut self) {
        info!("zigbee: role {}", role(self.mode).name);
        if self.mode == Mode::Off {
            self.state = State::Off;
            return self.changed();
        }
        self.bring_up();
    }

    /// Opens the radio and asks which firmware runs; the role object comes once it is the
    /// right one.
    fn bring_up(&mut self) {
        match self.radio.open(&self.cfg.lines) {
            Ok(link) => {
                self.state = State::Starting(Box::new(link));
                self.timers.set(Timer::Answer, ANSWER_TIMEOUT_MS);
                self.changed();
            }
            Err(e) => {
                error!("zigbee: {e}");
                self.fail("cannot open the radio".into());
            }
        }
    }

    fn on_starting_link(&mut self, ev: LinkEvent) {
        let State::Starting(link) = &mut self.state else {
            return;
        };
        match ev {
            LinkEvent::Boot => link.send(APP_INFO_REQ, &[], None),
            // [version BE32][manufacturer BE16][image type BE16][size BE32], the same from
            // every firmware
            LinkEvent::Frame { ty, payload } if ty == APP_INFO_RSP && payload.len() >= 12 => {
                let version = get_be32(&payload);
                self.on_firmware(version);
            }
            _ => {}
        }
    }

    fn on_firmware(&mut self, version: u32) {
        if !matches!(self.state, State::Starting(_)) {
            return;
        }
        self.timers.cancel(Timer::Answer);
        let path = self.image_path().to_path_buf();
        let want = ybl_load(&path).ok().map(|(_, info)| info.version);
        info!("zigbee: radio firmware 0x{version:08x}");
        match decide(version, want, self.mode, self.cfg.failed_version) {
            Decision::Run => {
                self.outcome(0);
                self.run();
            }
            Decision::Keep => {
                // The radio did not take the new build.
                warn!(
                    "zigbee: keeping radio firmware 0x{version:08x} (0x{:08x} was not taken)",
                    want.unwrap_or(0)
                );
                self.run();
            }
            Decision::Flash => {
                info!(
                    "zigbee: radio runs {} firmware, the {} role needs {} 0x{:08x}",
                    family_name(version),
                    role(self.mode).name,
                    path.display(),
                    want.unwrap_or(0)
                );
                self.flash();
            }
        }
    }

    fn run(&mut self) {
        let State::Starting(link) = std::mem::replace(&mut self.state, State::Off) else {
            return;
        };
        let link = *link;
        if self.mode == Mode::Router {
            let mut router = Router::new(link);
            router.start();
            self.state = State::Running(Role::Router(Box::new(router)));
            return self.changed();
        }
        let mut coord = Coordinator::new(link);
        let mut deconz = DeconzServer::new(self.cfg.deconz_port);
        deconz.attach(&mut coord);
        if !deconz.start() {
            self.fail("cannot open the deCONZ port".into());
            return;
        }
        let mut hooks = CoordHooks {
            deconz: &mut deconz,
            changed: false,
        };
        coord.start(&mut hooks);
        self.state = State::Running(Role::Coordinator {
            coord: Box::new(coord),
            deconz: Box::new(deconz),
        });
        // One Changed, whether or not the start already reported one.
        self.changed();
    }

    fn flash(&mut self) {
        if self.is_flashing() {
            return;
        }
        // The link must be gone before the bootloader drives the same port.
        self.state = State::Off;
        let path = self.image_path().to_path_buf();
        let (image, info) = match ybl_load(&path) {
            Ok(v) => v,
            Err(e) => return self.fail(e.to_string()),
        };
        if info.version == self.cfg.failed_version {
            // Given up on in an earlier start: not written on every restart. A new image
            // (update) or a role change tries again.
            return self.fail(format!(
                "the radio did not take firmware 0x{:08x}; not written again",
                info.version
            ));
        }
        if self.flashes >= MAX_FLASHES {
            self.outcome(info.version);
            return self.fail(format!(
                "the radio does not take firmware 0x{:08x}",
                info.version
            ));
        }
        self.flashes += 1;
        self.state = State::Flashing { progress: 0.0 };
        info!(
            "zigbee: writing {} (version 0x{:08x}, {} bytes) to the radio",
            path.display(),
            info.version,
            image.len()
        );
        self.changed();
        let lines = self.cfg.lines.clone();
        let flash = self.flasher.clone();
        let tx = self.flash_tx.clone();
        let spawned = std::thread::Builder::new()
            .name("zigbee-flash".into())
            .spawn(move || {
                let mut last = -1i32;
                let r = flash.flash(&lines, &image, &mut |p| {
                    // Steps of 5 %: a status change per step is enough for Home Assistant.
                    let pct = (p * 100.0) as i32;
                    if pct / 5 == last / 5 {
                        return;
                    }
                    last = pct;
                    let _ = tx.send(FlashMsg::Progress(p));
                });
                let _ = tx.send(FlashMsg::Done(r));
            });
        match spawned {
            Ok(t) => self.thread = Some(t),
            Err(e) => {
                let _ = self
                    .flash_tx
                    .send(FlashMsg::Done(Err(FlashError::Thread(e.to_string()))));
            }
        }
    }

    fn on_flash(&mut self, m: FlashMsg) {
        match m {
            FlashMsg::Progress(p) => {
                if let State::Flashing { progress } = &mut self.state {
                    *progress = p;
                    self.changed();
                }
            }
            FlashMsg::Done(r) => {
                if let Some(t) = self.thread.take() {
                    let _ = t.join();
                }
                match r {
                    Ok(()) => info!("zigbee: radio firmware written"),
                    Err(e) => error!("zigbee: writing the radio failed: {e}"),
                }
                // Either way the radio runs some application again (or none): identify it.
                self.state = State::Off;
                self.bring_up();
            }
        }
    }

    fn fail(&mut self, why: String) {
        error!("zigbee: {why}");
        self.state = State::Failed { error: why };
        self.changed();
    }

    fn on_timer(&mut self, t: Timer) {
        match t {
            Timer::Answer => {
                if !matches!(self.state, State::Starting(_)) {
                    return;
                }
                warn!("zigbee: the radio does not identify its firmware");
                // Most likely an interrupted upload left no application, which only writing the
                // image fixes.
                self.flash();
                if matches!(self.state, State::Failed { .. }) {
                    // Writing refused: maybe it answers later.
                    self.timers.set(Timer::Retry, RETRY_MS);
                }
            }
            Timer::Retry => self.bring_up(),
        }
    }

    fn on_cmd(&mut self, c: Cmd) {
        let State::Running(Role::Router(router)) = &mut self.state else {
            return;
        };
        match c {
            Cmd::Join => {
                if router.join() {
                    self.changed();
                }
            }
            Cmd::Leave => router.leave(),
        }
    }

    fn status(&self) -> Status {
        match &self.state {
            State::Off => Status::Off,
            State::Starting(_) => Status::Starting,
            State::Flashing { progress } => Status::Flashing {
                label: role(self.mode).label,
                percent: (progress * 100.0) as i32,
            },
            State::Failed { error } => Status::Failed {
                error: error.clone(),
            },
            State::Running(Role::Coordinator { coord, .. }) => {
                let ready = coord.ready();
                let n = coord.network();
                Status::Coordinator {
                    ready,
                    forming: n.forming,
                    formed: n.formed,
                    pan: n.pan,
                    channel: n.channel,
                }
            }
            State::Running(Role::Router(router)) => {
                let s = router.state();
                Status::Router {
                    ready: s.ready,
                    joining: s.joining,
                    joined: s.joined,
                    pan: s.pan,
                    nwk: s.nwk,
                    channel: s.channel,
                    error: s.error.clone(),
                }
            }
        }
    }

    async fn next(
        &mut self,
        cmds: &mut UnboundedReceiver<Cmd>,
        flash_rx: &mut UnboundedReceiver<FlashMsg>,
    ) -> Wake {
        let Self { timers, state, .. } = self;
        match state {
            State::Starting(link) => tokio::select! {
                biased;
                t = timers.next() => Wake::Timer(t),
                ev = link.next() => Wake::Link(ev),
                Some(m) = flash_rx.recv() => Wake::Flash(m),
                c = cmds.recv() => Wake::Cmd(c),
            },
            State::Running(Role::Coordinator { coord, deconz }) => tokio::select! {
                biased;
                t = timers.next() => Wake::Timer(t),
                w = coord.next() => Wake::Coord(w),
                w = deconz.next() => Wake::Deconz(w),
                Some(m) = flash_rx.recv() => Wake::Flash(m),
                c = cmds.recv() => Wake::Cmd(c),
            },
            State::Running(Role::Router(router)) => tokio::select! {
                biased;
                t = timers.next() => Wake::Timer(t),
                w = router.next() => Wake::Router(w),
                Some(m) = flash_rx.recv() => Wake::Flash(m),
                c = cmds.recv() => Wake::Cmd(c),
            },
            State::Off | State::Flashing { .. } | State::Failed { .. } => tokio::select! {
                biased;
                t = timers.next() => Wake::Timer(t),
                Some(m) = flash_rx.recv() => Wake::Flash(m),
                c = cmds.recv() => Wake::Cmd(c),
            },
        }
    }

    fn handle(&mut self, w: Wake) {
        // Firmware identification and the deCONZ host do not change the status by themselves
        // (what they change goes through changed()).
        let republish = !matches!(w, Wake::Link(_) | Wake::Deconz(_));
        match w {
            Wake::Timer(t) => self.on_timer(t),
            Wake::Link(ev) => self.on_starting_link(ev),
            Wake::Coord(w) => {
                let State::Running(Role::Coordinator { coord, deconz }) = &mut self.state else {
                    return;
                };
                let mut hooks = CoordHooks {
                    deconz,
                    changed: false,
                };
                coord.handle(w, &mut hooks);
                if hooks.changed {
                    self.changed();
                }
            }
            Wake::Deconz(w) => {
                if let State::Running(Role::Coordinator { coord, deconz }) = &mut self.state {
                    deconz.handle(w, &mut **coord);
                }
            }
            Wake::Router(w) => {
                let changed = match &mut self.state {
                    State::Running(Role::Router(router)) => router.handle(w),
                    _ => false,
                };
                if changed {
                    self.changed();
                }
            }
            Wake::Flash(m) => self.on_flash(m),
            Wake::Cmd(Some(c)) => self.on_cmd(c),
            Wake::Cmd(None) => {}
        }
        // The getters also follow changes C++ reported without changed() (a leave, the
        // router's state on a frame that is not news); no Changed for them, as in C++.
        if republish {
            self.publish();
        }
    }

    async fn run_actor(
        mut self,
        mut cmds: UnboundedReceiver<Cmd>,
        mut flash_rx: UnboundedReceiver<FlashMsg>,
    ) {
        loop {
            match self.next(&mut cmds, &mut flash_rx).await {
                Wake::Cmd(None) => break,
                w @ Wake::Deconz(_) => {
                    self.handle(w);
                    // A flooding LAN client must not starve the other actors.
                    tokio::task::coop::consume_budget().await;
                }
                w => self.handle(w),
            }
        }
        // radio_flash cannot be stopped: an upload still running is left to its detached thread
        // (dropping the JoinHandle), which the process exit ends. That leaves the radio without
        // an application; the next start finds it silent and writes the image again (C++ joined
        // it in the destructor, and the 5 s teardown watchdog cut it the same way).
        self.thread = None;
        self.state = State::Off;
    }
}

#[cfg(test)]
mod tests;
