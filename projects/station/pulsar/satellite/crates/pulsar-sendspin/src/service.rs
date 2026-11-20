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

#[derive(Clone)]
pub struct SendspinHandle {
    client: Rc<SsClient>,
    status: Arc<Status>,
    cmd: UnboundedSender<Cmd>,
    sink: Arc<Sink>,
}

struct Ctx {
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
    pub fn streaming(&self) -> bool {
        self.status.streaming()
    }

    pub fn connected(&self) -> bool {
        self.client.is_connected()
    }

    pub fn goodbye_restart(&self) {
        if self.client.is_connected() {
            info!("sendspin: goodbye restart (daemon restarts)");
            self.client.disconnect_restart();
        }
    }

    pub fn set_duck_gain(&self, g: f32) {
        self.sink.set_gain(g);
    }

    pub fn report_volume(&self, volume: f32, muted: bool) {
        let _ = self.cmd.send(Cmd::ReportVolume { volume, muted });
    }

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

    pub fn yield_to_local(&self) {
        if !self.status.streaming() {
            return;
        }
        info!("sendspin: local playback took over, leaving the group");
        self.client.leave();
    }

    pub fn confirm_pairing_window(&self) {
        let _ = self.cmd.send(Cmd::ConfirmPairingWindow);
    }
}

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
        pairing_code_display_digits: true,
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
