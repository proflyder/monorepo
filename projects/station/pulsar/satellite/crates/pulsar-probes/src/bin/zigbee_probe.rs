#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand};
use pulsar_deconz::{ApsFrame, Hooks, Ncp, Server};
use pulsar_zbhci::bytes::{get_be16, get_be32};
use pulsar_zbhci::link::{Link, LinkEvent, SysfsLines};
use pulsar_zigbee::RadioPaths;
use pulsar_zigbee::coordinator::Coordinator;
use pulsar_zigbee::flasher::{radio_flash, ybl_load};
use tracing::{debug, error, info};

const APP_INFO_REQ: u16 = 0x0900;
const APP_INFO_RSP: u16 = 0x8900;
const ANSWER_TIMEOUT: Duration = Duration::from_secs(10);

fn main() -> ExitCode {
    pulsar_core::init_logger();
    pulsar_core::add_own_target(module_path!());
    let radio = pulsar_model::detect_profile().and_then(|p| p.zigbee.as_ref());
    let Some(radio) = radio else {
        error!("no Zigbee radio on this model");
        return ExitCode::FAILURE;
    };
    let lines = RadioPaths {
        tty: radio.tty.into(),
        reset: radio.reset.into(),
        boot: radio.boot.into(),
    };
    let cli = Cli::parse();
    match cli.cmd {
        Some(Cmd::Info) => info_mode(&lines),
        Some(Cmd::Flash { image }) => flash_mode(&lines, &image),
        None => server_mode(&lines, cli.port.unwrap_or(6639)),
    }
}

#[derive(Parser)]
#[command(about = "Zigbee radio without the daemon")]
struct Cli {
    port: Option<u16>,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    Info,
    Flash { image: PathBuf },
}

fn open_link(lines: &RadioPaths) -> Result<Link, pulsar_zbhci::RadioError> {
    Link::open(
        &lines.tty,
        Box::new(SysfsLines::new(lines.reset.clone(), lines.boot.clone())),
    )
}

struct ProbeHooks<'a> {
    server: &'a mut Server,
}

impl Hooks for ProbeHooks<'_> {
    fn received(&mut self, coord: &mut dyn Ncp, f: &ApsFrame) {
        info!(
            "rx 0x{:04x}:{} -> {}0x{:04x}:{} profile 0x{:04x} cluster 0x{:04x} {} bytes lqi {} rssi {}",
            f.src,
            f.src_ep,
            if f.group { "group " } else { "" },
            f.dst,
            f.dst_ep,
            f.profile,
            f.cluster,
            f.data.len(),
            f.lqi,
            f.rssi
        );
        self.server.received(coord, f);
    }

    fn sent(&mut self, coord: &mut dyn Ncp, handle: u32, status: u8) {
        info!("sent {handle:08x}: status 0x{status:02x}");
        self.server.sent(coord, handle, status);
    }

    fn changed(&mut self, coord: &mut dyn Ncp) {
        let n = coord.network();
        info!(
            "ready {} formed {} PAN 0x{:04x} channel {} IEEE {:016x}",
            u8::from(coord.ready()),
            u8::from(n.formed),
            n.pan,
            n.channel,
            n.ieee
        );
        self.server.changed(coord);
    }

    fn formed(&mut self, coord: &mut dyn Ncp, ok: bool) {
        self.server.formed(coord, ok);
    }
}

fn server_mode(lines: &RadioPaths, port: u16) -> ExitCode {
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            error!("tokio runtime: {e}");
            return ExitCode::FAILURE;
        }
    };
    rt.block_on(async {
        let mut server = Server::new(port);
        if !server.start() {
            return ExitCode::FAILURE;
        }
        let link = match open_link(lines) {
            Ok(link) => link,
            Err(e) => {
                error!("zigbee: {e}");
                return ExitCode::FAILURE;
            }
        };
        let mut coord = Coordinator::new(link);
        server.attach(&mut coord);
        coord.start(&mut ProbeHooks {
            server: &mut server,
        });
        loop {
            tokio::select! {
                w = coord.next() => coord.handle(w, &mut ProbeHooks { server: &mut server }),
                w = server.next() => server.handle(w, &mut coord),
            }
        }
    })
}

fn info_mode(lines: &RadioPaths) -> ExitCode {
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            error!("tokio runtime: {e}");
            return ExitCode::FAILURE;
        }
    };
    rt.block_on(async {
        let mut link = match open_link(lines) {
            Ok(link) => link,
            Err(e) => {
                error!("zigbee: {e}");
                return ExitCode::FAILURE;
            }
        };
        let timeout = tokio::time::sleep(ANSWER_TIMEOUT);
        tokio::pin!(timeout);
        loop {
            tokio::select! {
                ev = link.next() => match ev {
                    LinkEvent::Boot => link.send(APP_INFO_REQ, &[], None),
                    LinkEvent::Frame { ty: APP_INFO_RSP, payload } if payload.len() >= 12 => {
                        let version = get_be32(&payload);
                        let line = format!(
                            "version=0x{version:08x} family=0x{:02x} manufacturer=0x{:04x} image_type=0x{:04x} size={}",
                            version >> 24,
                            get_be16(&payload[4..]),
                            get_be16(&payload[6..]),
                            get_be32(&payload[8..])
                        );
                        let _ = writeln!(std::io::stdout(), "{line}");
                        return ExitCode::SUCCESS;
                    }
                    _ => {}
                },
                () = &mut timeout => {
                    error!("the radio does not identify its firmware");
                    return ExitCode::FAILURE;
                }
            }
        }
    })
}

fn flash_mode(lines: &RadioPaths, path: &Path) -> ExitCode {
    let (image, info) = match ybl_load(path) {
        Ok(v) => v,
        Err(e) => {
            error!("{e}");
            return ExitCode::FAILURE;
        }
    };
    info!(
        "writing {} (version 0x{:08x})",
        path.display(),
        info.version
    );
    match radio_flash(
        &lines.tty,
        &mut SysfsLines::new(lines.reset.clone(), lines.boot.clone()),
        &image,
        &mut |p| debug!("{} %", (p * 100.0) as i32),
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            error!("{e}");
            ExitCode::FAILURE
        }
    }
}
