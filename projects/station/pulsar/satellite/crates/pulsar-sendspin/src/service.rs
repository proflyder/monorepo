//! The Sendspin player service: one sendspin-cpp client in server-initiated mode (the library
//! listens on ws://<ip>:<port>/sendspin; the controller advertises `_sendspin._tcp` over mDNS).
//!
//! Threads: the library's protocol task, the player's sync (decode) task and the visualizer's
//! drain thread are its own; its main loop (`SendspinClient::loop()`) runs on the runtime thread
//! from the actor's 10 ms tick. Every main loop callback goes through [`EventHandler`]: the sink
//! configuration, the log line, the published `SendspinEvent` and the stream edge all happen in
//! one place. Decoded PCM goes from the sync task straight to the ALSA sink; visualizer frames go
//! from the drain thread straight to the event channel.

use std::ffi::OsStr;
use std::rc::Rc;
use std::sync::Arc;

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tracing::{error, info, warn};

use crate::SendspinConfig;
use crate::actor::{Actor, Cmd, EventHandler, Link, SinkPort, transport_command};
use crate::event::LibEvent;
use crate::ffi::{self, Client, Codec, Format, Listener, Scale, VisType};
use crate::proto::Visualizer;
use crate::sink::Sink;
use crate::state::Status;
use crate::store::{FileStore, IdentityStore};
use crate::{SendspinEvent, Transport, volume_percent};

/// Player formats, preferred first.
const FORMATS: [Format; 5] = [
    Format {
        codec: Codec::Flac,
        channels: 2,
        rate: 48000,
        bits: 16,
    },
    Format {
        codec: Codec::Flac,
        channels: 2,
        rate: 44100,
        bits: 16,
    },
    Format {
        codec: Codec::Opus,
        channels: 2,
        rate: 48000,
        bits: 16,
    },
    Format {
        codec: Codec::Pcm,
        channels: 2,
        rate: 48000,
        bits: 16,
    },
    Format {
        codec: Codec::Pcm,
        channels: 2,
        rate: 44100,
        bits: 16,
    },
];

const VIS_TYPES: [VisType; 3] = [VisType::Spectrum, VisType::Loudness, VisType::Beat];

type SsClient = Client<Ctx>;

/// Lives on the runtime thread (`!Send`). The actor stops (goodbye to the server, library threads
/// joined) once every handle is dropped.
#[derive(Clone)]
pub struct SendspinHandle {
    client: Rc<SsClient>,
    status: Arc<Status>,
    cmd: UnboundedSender<Cmd>,
    sink: Arc<Sink>,
}

/// State the library's threads reach through the listener.
struct Ctx {
    /// All main-loop events, handled in one place (sink, log, published state, stream edges).
    handler: Arc<EventHandler>,
    sink: Arc<Sink>,
    store: Arc<dyn IdentityStore>,
}

impl Listener for Ctx {
    fn event(&self, ev: LibEvent) {
        self.handler.event(ev);
    }

    fn load_blob(&self, key: &OsStr) -> Option<Vec<u8>> {
        self.store.load(key)
    }

    fn save_blob(&self, key: &OsStr, data: &[u8]) -> bool {
        self.store.save(key, data)
    }

    fn audio_write(&self, data: &[u8], timeout_ms: u32) -> (usize, u32, i64) {
        self.sink.write(data, timeout_ms)
    }

    fn loudness(&self, v: u16) {
        self.handler.loudness(v);
    }

    fn beat(&self) {
        self.handler.beat();
    }

    fn spectrum(&self, bins: &[u16]) {
        self.handler.spectrum(bins);
    }
}

impl SinkPort for Sink {
    fn configure(&self, rate: u32, channels: u8, bits: u8) {
        Self::configure(self, rate, channels, bits);
    }

    fn release(&self) {
        Self::release(self);
    }
}

impl Link for SsClient {
    fn run_loop(&self) -> bool {
        Self::run_loop(self)
    }

    fn is_connected(&self) -> bool {
        Self::is_connected(self)
    }

    fn server_info(&self) -> Option<(String, String)> {
        Self::server_info(self)
    }

    fn volume(&self) -> u8 {
        Self::volume(self)
    }

    fn muted(&self) -> bool {
        Self::muted(self)
    }

    fn update_volume(&self, volume: u8) {
        Self::update_volume(self, volume);
    }

    fn update_muted(&self, muted: bool) {
        Self::update_muted(self, muted);
    }

    fn confirm_pairing_window(&self) {
        Self::confirm_pairing_window(self);
    }
}

impl SendspinHandle {
    /// На этом динамике играет аудиопоток Sendspin (конец потока без «stopped» сообщается на 1 с
    /// позже; см. `state`). Значение меняет актор; сразу после команды геттер отдаёт прежнее.
    pub fn streaming(&self) -> bool {
        self.status.streaming()
    }

    /// Sendspin-сервер (например, Music Assistant) подключён: атомарный флаг, который пишет поток
    /// библиотеки; чтение не блокирует.
    pub fn connected(&self) -> bool {
        self.client.is_connected()
    }

    /// The daemon is about to be restarted by its supervisor: send `client/goodbye` with reason
    /// `restart` so Music Assistant redials this player. Without it the library's stop() sends
    /// `shutdown`, after which MA deliberately does not reconnect until its provider restarts.
    pub fn goodbye_restart(&self) {
        if self.client.is_connected() {
            info!("sendspin: goodbye restart (daemon restarts)");
            self.client.disconnect_restart();
        }
    }

    /// Software gain 0..1 applied to Sendspin audio (ramped), used for ducking.
    pub fn set_duck_gain(&self, g: f32) {
        self.sink.set_gain(g);
    }

    /// Local volume / mute change (buttons, HA): reported to the server when it differs.
    pub fn report_volume(&self, volume: f32, muted: bool) {
        let _ = self.cmd.send(Cmd::ReportVolume { volume, muted });
    }

    /// Group transport command to the server (controller role), for the whole Sendspin group.
    /// false when no server is connected or it does not offer the command now (e.g. NEXT with
    /// an empty queue).
    pub fn transport(&self, t: Transport) -> bool {
        if !self.client.is_connected() {
            return false;
        }
        let (c, name) = transport_command(t);
        let ok = self.client.send_command(c);
        info!(
            "sendspin: transport {name} {}",
            if ok {
                "sent"
            } else {
                "not offered by the server"
            }
        );
        ok
    }

    /// Another local source took over the speaker: leave the Sendspin group (stops our
    /// playback, the rest of the group keeps playing). `streaming` changes only with the
    /// stream end that follows.
    pub fn yield_to_local(&self) {
        if !self.status.streaming() {
            return;
        }
        info!("sendspin: local playback took over, leaving the group");
        self.client.leave();
    }

    /// The operator performed the pairing-window gesture.
    pub fn confirm_pairing_window(&self) {
        let _ = self.cmd.send(Cmd::ConfirmPairingWindow);
    }
}

/// Starts the client (WebSocket server on `cfg.port`) with the current local volume / mute.
/// Must run inside the controller's `LocalSet`.
pub fn spawn(
    cfg: &SendspinConfig,
    volume: f32,
    muted: bool,
    master_gain: f32,
) -> Result<(SendspinHandle, UnboundedReceiver<SendspinEvent>), crate::SendspinInitError> {
    let store: Arc<dyn IdentityStore> = Arc::new(FileStore::new(cfg.state_dir.clone()));
    ffi::set_log_level_info();
    if !store.ensure() {
        warn!("sendspin: identity will not persist");
    }
    let unpaired = store.unpaired_access();
    let (tx, rx) = unbounded_channel();
    let status = Status::new();
    let sink = Arc::new(Sink::new(cfg.playback));
    let initial_gain = if muted { 0.0 } else { master_gain };
    sink.set_gain(initial_gain);
    let handler = Arc::new(EventHandler::new(
        Arc::clone(&status),
        Arc::clone(&sink) as Arc<dyn SinkPort>,
        tx,
    ));
    let ctx = Arc::new(Ctx {
        handler: Arc::clone(&handler),
        sink: Arc::clone(&sink),
        store,
    });
    let config = ffi::Config {
        name: &cfg.name,
        product_name: &cfg.product_name,
        manufacturer: &cfg.manufacturer,
        software_version: &cfg.software_version,
        mac: &cfg.mac,
        port: cfg.port,
        formats: &FORMATS,
        // 8 log-spaced bands over the range the speaker-reference analyzer covers; ~30 frames/s
        // matches the LED frame period (33 ms). Only ~1/7 of buffer_capacity holds wire data and
        // the server sends frames as far ahead as audio (seconds): 64 KB keeps its flow control
        // from starving them.
        visualizer: Visualizer {
            buffer_capacity: 65536,
            types: &VIS_TYPES,
            rate_max: 30,
            bins: 8,
            scale: Scale::Log,
            f_min: 40,
            f_max: 7600,
            display_offset_ms: 0,
        },
        // Dynamic pairing code: six digits shown on the 4-digit clock (paged as 3-3 groups).
        pairing_code_display_digits: true,
        // The operator pairs with the six-digit code; the provisioning PSK never enters a log.
        pairing_psk_location: "operator",
        unpaired_access: unpaired,
    };
    let created = Client::new(&config, ctx);
    let started = created.filter(|c| {
        c.start().unwrap_or_else(|| {
            error!("sendspin: client start threw an exception");
            false
        })
    });
    let Some(client) = started else {
        error!("sendspin: client start failed");
        return Err(crate::SendspinInitError::Start);
    };
    client.set_output_delay_adjustable(true);
    client.update_volume(volume_percent(volume));
    client.update_muted(muted);
    info!(
        "sendspin: listening on ws://0.0.0.0:{}/sendspin as '{}', client_id {}, unpaired access {}",
        cfg.port,
        cfg.name,
        client.client_id(),
        if unpaired { "on" } else { "off" }
    );
    if client.pairing_token().is_some() {
        info!("sendspin: pairing token retained (hidden; pairing uses the six-digit code)");
    }

    let client = Rc::new(client);
    let (cmd, cmds) = unbounded_channel();
    let actor = Actor::new(
        Rc::clone(&client),
        Arc::clone(&handler),
        Arc::clone(&status),
    );
    pulsar_core::supervisor().spawn_local("sendspin", actor.run(cmds));
    Ok((
        SendspinHandle {
            client,
            status,
            cmd,
            sink,
        },
        rx,
    ))
}
