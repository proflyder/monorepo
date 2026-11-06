//! Файл ключа Noise `api_key` (base64 из 32 байт, как `api: encryption: key` ESPHome):
//! пишется авто-провижинингом HA (NoiseEncryptionSetKeyRequest) или `ctl.sh api-key`.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use esphome_proto::api_key::trim;
use esphome_proto::frame::Psk;
use tracing::{error, info, warn};

use crate::error::KeyError;

pub use home_linux::io_error_text as os_error_text;

/// Действующий ключ. Без ключа Noise-сессии идут на нулевом ключе: так ESPHome даёт HA
/// провижинить ключ, не отправляя его открытым текстом (защита только от прослушивания).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Key {
    pub psk: Psk,
    pub has_key: bool,
    /// Fail-closed без случайного ключа (getrandom не сработал): каждый Noise handshake
    /// отвергается как неверный ключ, plaintext — как «requires encryption» (`has_key`).
    pub reject_all: bool,
}

/// Читает файл ключа. Нет файла / пусто / нулевой ключ — ключа нет. Нечитаемый файл или не
/// base64 из 32 байт — fail-closed: кто-то хотел включить шифрование, отката в plaintext нет;
/// случайный ключ не пускает никого, пока файл не исправят (или не удалят).
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
        // Фиксированный резервный ключ был бы известным ключом: лучше отказывать всем.
        error!("api: getrandom failed; rejecting every client");
    }
    Key {
        psk: Psk::new(raw),
        has_key: true,
        reject_all,
    }
}

/// Итог NoiseEncryptionSetKeyRequest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetKey {
    /// Отвечено success = false.
    Rejected,
    /// Действующий ключ отправлен снова: success, ничего не меняется.
    Unchanged,
    /// Файл изменился: перечитать его и отключить всех клиентов.
    Changed,
}

/// Пишет (или при пустом `b64` удаляет) файл ключа. `b64` уже обрезан.
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

/// Атомарная замена: отключение питания во время ротации ключа HA должно оставить либо старый,
/// либо новый ключ, но не порванный файл.
fn write_file_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    // Оставшийся временный файл (питание пропало посреди записи) не должен одалживать свой
    // режим или владельца: ключ пишется в созданный сейчас файл 0600.
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
        // close(2) не проверяется (drop File игнорирует результат): fsync выше уже сообщил
        // ошибку записи, а на локальных ФС /data close сообщать нечего.
        drop(f);
        fs::rename(&tmp, path)
    };
    let r = write();
    if r.is_err() {
        let _ = fs::remove_file(&tmp);
        return r;
    }
    // rename переживает отключение питания только после sync каталога. Это не ошибка запроса:
    // новый ключ уже на месте и действует.
    let dir = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    if let Err(e) = File::open(dir).and_then(|d| d.sync_all()) {
        warn!("api: cannot sync {}: {}", dir.display(), os_error_text(&e));
    }
    Ok(())
}
