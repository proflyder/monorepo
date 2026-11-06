//! mDNS (RFC 6762/6763) responder: advertises DNS-SD service instances over IPv4 under one host
//! name. Answers PTR/SRV/TXT/A/ANY queries for each service, its instance and "<host>.local",
//! lists every service type in "_services._dns-sd._udp.local", announces on start, on
//! `add_service`/`set_txt` and when the address or link changes (rtnetlink, with a 30 s poll as
//! fallback), and sends goodbyes when the last handle is dropped.
//!
//! The crate is a single actor on the daemon's `current_thread` runtime: no own thread and no I/O
//! outside the actor task. The pure DNS core lives in `dns`, the UDP/rtnetlink adapter in `actor`.

mod actor;
mod dns;
#[allow(unsafe_code)]
mod sys;

use std::io;

use home_actor::Handle;
use thiserror::Error;

pub use actor::MdnsService;

/// A socket could not be opened or bound; nothing is advertised.
#[derive(Debug, Error)]
pub enum MdnsError {
    #[error("mdns: socket: {}", home_linux::io_error_text(.0))]
    Socket(#[source] io::Error),
    #[error("mdns: bind: {}", home_linux::io_error_text(.0))]
    Bind(#[source] io::Error),
}

pub(crate) use home_linux::io_error_text as errstr;

#[derive(Clone)]
pub struct MdnsHandle {
    pub(crate) cmds: Handle<actor::Cmd>,
}

impl MdnsHandle {
    /// Advertises another service on the same host (A record). Services added right after
    /// `spawn`, before the actor first runs, go out with the start announcement (C++
    /// `add_service` before `start()`); later ones are announced at once. At most 7 services.
    pub fn add_service(&self, s: MdnsService) {
        let _ = self.cmds.send(actor::Cmd::AddService(s));
    }

    /// Replaces the TXT strings of a service added earlier (e.g. "_esphomelib._tcp") and
    /// announces it.
    pub fn set_txt(&self, service: &str, txt: Vec<String>) {
        let _ = self.cmds.send(actor::Cmd::SetTxt(service.to_owned(), txt));
    }
}

/// Opens the responder for "<host>.local". When another responder holds 5353 without
/// SO_REUSEADDR/SO_REUSEPORT, it falls back to an ephemeral port: announcements still go out,
/// queries are not heard.
pub fn spawn(host: &str) -> Result<MdnsHandle, MdnsError> {
    actor::start(host)
}
