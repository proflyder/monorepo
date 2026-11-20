mod actor;
mod dns;
#[allow(unsafe_code)]
mod sys;

use std::io;

use home_actor::Handle;
use thiserror::Error;

pub use actor::MdnsService;

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
    pub fn add_service(&self, s: MdnsService) {
        let _ = self.cmds.send(actor::Cmd::AddService(s));
    }

    pub fn set_txt(&self, service: &str, txt: Vec<String>) {
        let _ = self.cmds.send(actor::Cmd::SetTxt(service.to_owned(), txt));
    }
}

pub fn spawn(host: &str) -> Result<MdnsHandle, MdnsError> {
    actor::start(host)
}
