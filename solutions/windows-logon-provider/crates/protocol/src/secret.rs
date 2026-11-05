//! Plaintext payload of `secret.bin` after DPAPI machine-scope decryption:
//! `{"version":1,"password":"<utf-8>","key":"<base64 of 32 bytes>"}`.
//! Password and key are wiped on drop and redacted in `Debug`.

use core::fmt;

use ricardo_secret::Secret;
use serde::Deserialize;
use zeroize::Zeroizing;

use crate::wire::{self, present};
use crate::{Error, b64_decode};

const KEY_BYTES: usize = 32;
const MAX_PASSWORD_BYTES: usize = 512;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SecretWire {
    #[serde(default, deserialize_with = "present")]
    version: Option<i64>,
    password: String,
    key: String,
}

/// Decrypted machine secret. Sharing [`Secret`] keeps the password zeroizing
/// and redacted; the key has no string form, only a zeroizing byte buffer.
pub struct MachineSecret {
    password: Secret,
    key: Zeroizing<Vec<u8>>,
}

impl MachineSecret {
    pub fn password(&self) -> &str {
        self.password.expose()
    }

    pub fn key(&self) -> &[u8] {
        &self.key
    }
}

impl fmt::Debug for MachineSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MachineSecret")
            .field("password", &"<redacted>")
            .field("key", &"<redacted>")
            .finish()
    }
}

pub fn parse_secret_json(body: &str) -> Result<MachineSecret, Error> {
    let mut wire: SecretWire = wire::parse(body)?;

    match wire.version {
        None | Some(1) => {}
        Some(_) => return Err(Error::new("secret version must be 1")),
    }

    if wire.password.is_empty() || wire.password.len() > MAX_PASSWORD_BYTES {
        return Err(Error::new("secret password must be 1..512 bytes"));
    }
    // A password may contain any byte except NUL; JSON escapes keep line
    // breaks representable.
    if wire.password.bytes().any(|c| c == 0) {
        return Err(Error::new("secret password contains a NUL"));
    }

    let key = Zeroizing::new(
        b64_decode(&wire.key).map_err(|_| Error::new("secret key is not 32 base64 bytes"))?,
    );
    if key.len() != KEY_BYTES {
        return Err(Error::new("secret key is not 32 base64 bytes"));
    }

    Ok(MachineSecret {
        password: Secret::new(std::mem::take(&mut wire.password)),
        key,
    })
}
