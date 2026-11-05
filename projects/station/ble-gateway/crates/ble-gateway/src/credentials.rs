//! SSID/PSK/token buffers written by the GATT characteristics.
//!
//! Both the GATT interface and the commit worker share one [`Credentials`], so
//! a commit always reads a consistent frame without reaching into zbus objects.
//! Long writes append at the requested byte.

use std::fmt;
use std::sync::Mutex;

use tracing::warn;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialField {
    Ssid = 0,
    Psk = 1,
    Token = 2,
}

#[derive(Default)]
pub struct Credentials([Mutex<Vec<u8>>; 3]);

/// Byte-for-byte copy of all three buffers.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct CredentialSnapshot {
    pub ssid: Vec<u8>,
    pub psk: Vec<u8>,
    pub token: Vec<u8>,
}

impl fmt::Debug for Credentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Credentials(***)")
    }
}

impl fmt::Debug for CredentialSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CredentialSnapshot(***)")
    }
}

impl Credentials {
    pub fn get(&self, field: CredentialField) -> Vec<u8> {
        match self.0[field as usize].lock() {
            Ok(value) => value.clone(),
            Err(error) => {
                warn!(%error, "credential buffer lock poisoned");
                Vec::new()
            }
        }
    }

    pub fn set(&self, field: CredentialField, value: &[u8]) {
        match self.0[field as usize].lock() {
            Ok(mut buffer) => {
                buffer.clear();
                buffer.extend_from_slice(value);
            }
            Err(error) => warn!(%error, "credential buffer lock poisoned"),
        }
    }

    /// Apply a GATT write at `offset`. `false` means an offset past the value
    /// or beyond the maximum attribute length BlueZ transmits.
    pub fn write(&self, field: CredentialField, value: &[u8], offset: usize) -> bool {
        if offset > 512 {
            return false;
        }
        let mut buffer = match self.0[field as usize].lock() {
            Ok(buffer) => buffer,
            Err(error) => {
                warn!(%error, "credential buffer lock poisoned");
                return false;
            }
        };
        if offset > buffer.len() {
            return false;
        }
        buffer.truncate(offset);
        buffer.extend_from_slice(value);
        true
    }

    pub fn snapshot(&self) -> CredentialSnapshot {
        CredentialSnapshot {
            ssid: self.get(CredentialField::Ssid),
            psk: self.get(CredentialField::Psk),
            token: self.get(CredentialField::Token),
        }
    }

    /// Seed all buffers at once (tests and provisioning tools).
    pub fn set_all(&self, ssid: &[u8], psk: &[u8], token: &[u8]) {
        self.set(CredentialField::Ssid, ssid);
        self.set(CredentialField::Psk, psk);
        self.set(CredentialField::Token, token);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_write_replaces_and_later_writes_append() {
        let credentials = Credentials::default();
        assert!(credentials.write(CredentialField::Psk, b"password", 0));
        assert!(credentials.write(CredentialField::Psk, b"123", 8));
        assert_eq!(credentials.get(CredentialField::Psk), b"password123");
        assert!(credentials.write(CredentialField::Psk, b"!", 11));
        assert_eq!(credentials.get(CredentialField::Psk), b"password123!");
    }

    #[test]
    fn offsets_past_the_value_or_the_attribute_limit_fail() {
        let credentials = Credentials::default();
        credentials.set(CredentialField::Ssid, b"Home");
        assert!(!credentials.write(CredentialField::Ssid, b"x", 5));
        assert!(!credentials.write(CredentialField::Ssid, b"x", 513));
        assert_eq!(credentials.get(CredentialField::Ssid), b"Home");
    }

    #[test]
    fn snapshot_is_independent_of_later_writes() {
        let credentials = Credentials::default();
        credentials.set_all(b"Home", b"password123", b"123456");
        let snapshot = credentials.snapshot();
        credentials.set(CredentialField::Ssid, b"Other");
        assert_eq!(snapshot.ssid, b"Home");
        assert_eq!(snapshot.psk, b"password123");
        assert_eq!(snapshot.token, b"123456");
    }
}
