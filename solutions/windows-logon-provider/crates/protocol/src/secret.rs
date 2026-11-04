//! Plaintext payload of `C:\ProgramData\Ricardo\KompLogon\secret.bin` after
//! DPAPI machine-scope decryption:
//!
//! ```json
//! {"version":1,"password":"<utf-8>","key":"<base64 of 32 bytes>"}
//! ```
//!
//! The password and the key are wiped on drop and never appear in `Debug`.

use core::fmt;

use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::json::{self, JsonValue, get_string};
use crate::{Error, b64_decode};

const KEY_BYTES: usize = 32;
const MAX_PASSWORD_BYTES: usize = 512;

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct MachineSecret {
    password: String,
    key: Vec<u8>,
}

impl MachineSecret {
    pub fn password(&self) -> &str {
        &self.password
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
    let object = json::parse(body)?;

    if let Some(value) = json::find_value(&object, "version") {
        match value {
            JsonValue::Number(1) => {}
            _ => return Err(Error::new("secret version must be 1")),
        }
    }

    let mut password = Zeroizing::new(
        get_string(&object, "password")
            .ok_or(Error::new("secret without password"))?
            .to_owned(),
    );
    if password.is_empty() || password.len() > MAX_PASSWORD_BYTES {
        return Err(Error::new("secret password must be 1..512 bytes"));
    }
    // A password may contain any byte except NUL; JSON escapes keep line
    // breaks representable.
    if password.bytes().any(|c| c == 0) {
        return Err(Error::new("secret password contains a NUL"));
    }

    let encoded_key = get_string(&object, "key").ok_or(Error::new("secret without key"))?;
    let mut key = Zeroizing::new(
        b64_decode(encoded_key).map_err(|_| Error::new("secret key is not 32 base64 bytes"))?,
    );
    if key.len() != KEY_BYTES {
        return Err(Error::new("secret key is not 32 base64 bytes"));
    }

    Ok(MachineSecret {
        password: std::mem::take(&mut *password),
        key: std::mem::take(&mut *key),
    })
}
