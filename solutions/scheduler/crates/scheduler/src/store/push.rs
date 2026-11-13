//! Push delivery devices (FCM registration tokens).
//!
//! The scheduler only *stores* the token: it never talks to Google and never
//! decides what to deliver. The sender (`solutions/phone-push`) reads this
//! registry and sends through FCM, so the scheduling core keeps its single
//! responsibility while the phone keeps using the one endpoint and bearer it
//! already has.

use rusqlite::{Row, params};

use super::{Store, StoreError};

/// One registered push device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushDevice {
    /// FCM registration token. Never logged.
    pub token: String,
    /// `android` today; the column stays free-form for future clients.
    pub platform: String,
    /// App version reported at registration.
    pub app_version: String,
    /// Owner-visible device name reported at registration.
    pub device_name: String,
    /// First registration (unix seconds).
    pub created_unix: i64,
    /// Last registration or refresh (unix seconds).
    pub updated_unix: i64,
}

/// Validated registration input (server-derived fields stay out).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PushDeviceInput {
    pub token: String,
    pub platform: String,
    pub app_version: String,
    pub device_name: String,
}

/// Upper bound on stored devices: one owner, a handful of phones.
pub const MAX_PUSH_DEVICES: usize = 16;
/// FCM tokens are long opaque strings; keep the bound tight but generous.
pub const MAX_TOKEN_BYTES: usize = 4096;
/// Shortest plausible FCM registration token.
pub const MIN_TOKEN_BYTES: usize = 32;
/// Bound for the free-form metadata fields.
pub const MAX_META_BYTES: usize = 64;

const COLS: &str = "token, platform, app_version, device_name, created_unix, updated_unix";

fn scan_device(row: &Row<'_>) -> rusqlite::Result<PushDevice> {
    Ok(PushDevice {
        token: row.get(0)?,
        platform: row.get(1)?,
        app_version: row.get(2)?,
        device_name: row.get(3)?,
        created_unix: row.get(4)?,
        updated_unix: row.get(5)?,
    })
}

fn clean_meta(value: &str, field: &str) -> Result<String, StoreError> {
    let value = value.trim();
    if value.len() > MAX_META_BYTES {
        return Err(StoreError::invalid(format!("{field} is too long")));
    }
    if value.chars().any(char::is_control) {
        return Err(StoreError::invalid(format!(
            "{field} has control characters"
        )));
    }
    Ok(value.to_owned())
}

/// Validates a registration request, never trusting server-derived fields.
pub fn validate_device(input: &PushDeviceInput) -> Result<PushDeviceInput, StoreError> {
    let token = input.token.trim();
    if token.len() < MIN_TOKEN_BYTES || token.len() > MAX_TOKEN_BYTES {
        return Err(StoreError::invalid("token has an invalid length"));
    }
    if token
        .chars()
        .any(|character| character.is_whitespace() || character.is_control())
    {
        return Err(StoreError::invalid("token has invalid characters"));
    }
    let platform = clean_meta(&input.platform, "platform")?;
    if platform.is_empty() {
        return Err(StoreError::invalid("platform is required"));
    }
    Ok(PushDeviceInput {
        token: token.to_owned(),
        platform,
        app_version: clean_meta(&input.app_version, "app_version")?,
        device_name: clean_meta(&input.device_name, "device_name")?,
    })
}

impl Store {
    /// Inserts or refreshes one device. Re-registering the same token keeps its
    /// original `created_unix`, so an app restart is not a new device.
    pub fn register_push_device(
        &self,
        input: &PushDeviceInput,
        now_unix: i64,
    ) -> Result<PushDevice, StoreError> {
        let device = validate_device(input)?;
        let count = self.list_push_devices()?.len();
        let known = self.push_device(&device.token)?.is_some();
        if !known && count >= MAX_PUSH_DEVICES {
            return Err(StoreError::limit("too many push devices"));
        }
        let token = device.token.clone();
        self.call(move |connection| {
            connection.execute(
                "INSERT INTO push_devices (token, platform, app_version, device_name, created_unix, updated_unix)
				 VALUES (?1, ?2, ?3, ?4, ?5, ?5)
				 ON CONFLICT(token) DO UPDATE SET
					platform = excluded.platform,
					app_version = excluded.app_version,
					device_name = excluded.device_name,
					updated_unix = excluded.updated_unix",
                params![
                    device.token,
                    device.platform,
                    device.app_version,
                    device.device_name,
                    now_unix
                ],
            )?;
            Ok(())
        })?;
        self.push_device(&token)?.ok_or(StoreError::NotFound)
    }

    /// Reads one device by its token.
    pub fn push_device(&self, token: &str) -> Result<Option<PushDevice>, StoreError> {
        let token = token.to_owned();
        self.call(move |connection| {
            let mut statement =
                connection.prepare(&format!("SELECT {COLS} FROM push_devices WHERE token = ?1"))?;
            let mut rows = statement.query(params![token])?;
            match rows.next()? {
                Some(row) => Ok(Some(scan_device(row)?)),
                None => Ok(None),
            }
        })
    }

    /// Every registered device, newest update first.
    pub fn list_push_devices(&self) -> Result<Vec<PushDevice>, StoreError> {
        self.call(|connection| {
            let mut statement = connection.prepare(&format!(
                "SELECT {COLS} FROM push_devices ORDER BY updated_unix DESC, token ASC"
            ))?;
            let rows = statement.query_map([], scan_device)?;
            let mut devices = Vec::new();
            for row in rows {
                devices.push(row?);
            }
            Ok(devices)
        })
    }

    /// Removes one device; `false` means it was already gone.
    pub fn remove_push_device(&self, token: &str) -> Result<bool, StoreError> {
        let token = token.to_owned();
        self.call(move |connection| {
            let deleted =
                connection.execute("DELETE FROM push_devices WHERE token = ?1", params![token])?;
            Ok(deleted > 0)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Config;

    fn store() -> Store {
        Store::open_in_memory(Config::default()).expect("store")
    }

    fn input(token: &str) -> PushDeviceInput {
        PushDeviceInput {
            token: token.to_owned(),
            platform: "android".to_owned(),
            app_version: "0.1.0".to_owned(),
            device_name: "Phone".to_owned(),
        }
    }

    const TOKEN_A: &str = "token-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const TOKEN_B: &str = "token-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn registering_the_same_token_refreshes_instead_of_duplicating() {
        let store = store();
        let first = store
            .register_push_device(&input(TOKEN_A), 100)
            .expect("register");
        assert_eq!(first.created_unix, 100);
        let second = store
            .register_push_device(
                &PushDeviceInput {
                    device_name: "Pixel".to_owned(),
                    ..input(TOKEN_A)
                },
                200,
            )
            .expect("refresh");
        assert_eq!(second.created_unix, 100);
        assert_eq!(second.updated_unix, 200);
        assert_eq!(second.device_name, "Pixel");
        assert_eq!(store.list_push_devices().expect("list").len(), 1);
    }

    #[test]
    fn removing_an_unknown_token_is_not_an_error() {
        let store = store();
        store
            .register_push_device(&input(TOKEN_A), 100)
            .expect("register");
        assert!(store.remove_push_device(TOKEN_A).expect("remove"));
        assert!(!store.remove_push_device(TOKEN_A).expect("remove again"));
        assert!(store.list_push_devices().expect("list").is_empty());
    }

    #[test]
    fn short_or_whitespace_tokens_are_rejected() {
        let store = store();
        assert!(store.register_push_device(&input("short"), 1).is_err());
        let spaced = "a aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        assert!(store.register_push_device(&input(spaced), 1).is_err());
        assert!(store.list_push_devices().expect("list").is_empty());
    }

    #[test]
    fn the_device_count_is_bounded() {
        let store = store();
        store
            .register_push_device(&input(TOKEN_A), 1)
            .expect("register");
        for index in 0..MAX_PUSH_DEVICES - 1 {
            let token = format!("token-{index:04}-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
            store
                .register_push_device(&input(&token), 1)
                .expect("register");
        }
        assert!(store.register_push_device(&input(TOKEN_B), 1).is_err());
        // Registering an existing token is still allowed at the cap.
        assert!(store.register_push_device(&input(TOKEN_A), 2).is_ok());
    }
}
