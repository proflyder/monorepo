use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use esphome_proto::api_key::trim;
use esphome_proto::frame::Psk;
use tracing::{error, info, warn};

use crate::error::KeyError;

pub use home_linux::io_error_text as os_error_text;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Key {
    pub psk: Psk,
    pub has_key: bool,
    pub reject_all: bool,
}

pub fn load_key(path: Option<&Path>) -> Key {
    let Some(path) = path else {
        return Key::default();
    };
    let mut buf = Vec::with_capacity(256);
    match File::open(path).and_then(|f| f.take(256).read_to_end(&mut buf)) {
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => {
            error!(
                "api: cannot read {}: {}; no client can connect until it is fixed",
                path.display(),
                os_error_text(&e)
            );
            return fail_closed();
        }
    }
    let s = trim(&buf);
    if s.is_empty() {
        info!("api: no encryption key ({}), plaintext API", path.display());
        return Key::default();
    }
    let Some(psk) = esphome_proto::api_key::decode_psk(s) else {
        error!(
            "api: {} is not base64 of 32 bytes; no client can connect until it is fixed",
            path.display()
        );
        return fail_closed();
    };
    if psk.is_zero() {
        return Key::default();
    }
    info!("api: Noise encryption enabled");
    Key {
        psk,
        has_key: true,
        reject_all: false,
    }
}

fn fail_closed() -> Key {
    let mut raw = [0u8; 32];
    let reject_all = getrandom::fill(&mut raw).is_err();
    if reject_all {
        error!("api: getrandom failed; rejecting every client");
    }
    Key {
        psk: Psk::new(raw),
        has_key: true,
        reject_all,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetKey {
    Rejected,
    Unchanged,
    Changed,
}

pub fn set_key(path: Option<&Path>, b64: &[u8], current: &Key) -> Result<SetKey, KeyError> {
    let Some(path) = path else {
        return Ok(SetKey::Rejected);
    };
    if b64.is_empty() {
        if let Err(e) = fs::remove_file(path)
            && e.kind() != io::ErrorKind::NotFound
        {
            return Err(KeyError::Remove {
                path: path.to_owned(),
                source: e,
            });
        }
        info!("api: encryption key cleared by client");
        return Ok(SetKey::Changed);
    }
    let Some(psk) = esphome_proto::api_key::decode_psk(b64) else {
        return Err(KeyError::Invalid {
            path: path.to_owned(),
        });
    };
    if psk.is_zero() {
        return Err(KeyError::Invalid {
            path: path.to_owned(),
        });
    }
    if current.has_key && psk.as_bytes() == current.psk.as_bytes() {
        return Ok(SetKey::Unchanged);
    }
    let mut data = b64.to_vec();
    data.push(b'\n');
    write_file_atomic(path, &data).map_err(|source| KeyError::Write {
        path: path.to_owned(),
        source,
    })?;
    info!("api: encryption key set by client");
    Ok(SetKey::Changed)
}

fn write_file_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    if let Err(e) = fs::remove_file(&tmp)
        && e.kind() != io::ErrorKind::NotFound
    {
        return Err(e);
    }
    let write = || {
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, path)
    };
    let r = write();
    if r.is_err() {
        let _ = fs::remove_file(&tmp);
        return r;
    }
    let dir = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    if let Err(e) = File::open(dir).and_then(|d| d.sync_all()) {
        warn!("api: cannot sync {}: {}", dir.display(), os_error_text(&e));
    }
    Ok(())
}
