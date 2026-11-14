//! Push delivery devices (FCM registration tokens).
//!
//! The scheduler only *stores* the token: it never talks to Google and never
//! decides what to deliver. The sender (`solutions/phone-push`) reads this
//! registry and sends through FCM, so the scheduling core keeps its single
//! responsibility while the phone keeps using the one endpoint and bearer it
//! already has.
//!
//! A device is identified by its own stable `device_id`, not by the FCM token:
//! Google rotates tokens (reinstall, app data restore, periodic rotation), and
//! a rotation must update the same row instead of adding a second phone. The
//! name is owner-visible metadata, so it can change without re-registering.

use rusqlite::{Row, params};

use super::{Store, StoreError};

/// One registered push device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushDevice {
    /// Stable device identity chosen by the app, never the FCM token.
    pub device_id: String,
    /// FCM registration token. Never logged.
    pub token: String,
    /// `android` today; the column stays free-form for future clients.
    pub platform: String,
    /// App version reported at registration.
    pub app_version: String,
    /// Owner-visible device name set in the app.
    pub device_name: String,
    /// First registration (unix seconds).
    pub created_unix: i64,
    /// Last registration or refresh (unix seconds).
    pub updated_unix: i64,
}

/// Validated registration input (server-derived fields stay out).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PushDeviceInput {
    pub device_id: String,
    pub token: String,
    pub platform: String,
    pub app_version: String,
    pub device_name: String,
}

/// Upper bound on stored devices: a family, not a fleet.
pub const MAX_PUSH_DEVICES: usize = 16;
/// FCM tokens are long opaque strings; keep the bound tight but generous.
pub const MAX_TOKEN_BYTES: usize = 4096;
/// Shortest plausible FCM registration token.
pub const MIN_TOKEN_BYTES: usize = 32;
/// Bound for the free-form metadata fields.
pub const MAX_META_BYTES: usize = 64;
/// Bound for the stable device identity (a SHA-256 hex digest is 64 chars).
pub const MIN_DEVICE_ID_BYTES: usize = 8;
/// Bound for the stable device identity.
pub const MAX_DEVICE_ID_BYTES: usize = 128;

const COLS: &str =
    "device_id, token, platform, app_version, device_name, created_unix, updated_unix";

fn scan_device(row: &Row<'_>) -> rusqlite::Result<PushDevice> {
    Ok(PushDevice {
        device_id: row.get(0)?,
        token: row.get(1)?,
        platform: row.get(2)?,
        app_version: row.get(3)?,
        device_name: row.get(4)?,
        created_unix: row.get(5)?,
        updated_unix: row.get(6)?,
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

fn clean_device_id(input: &PushDeviceInput, token: &str) -> Result<String, StoreError> {
    let device_id = input.device_id.trim();
    // Older builds registered by token only. Keeping the token as the
    // provisional identity preserves those rows until the app upgrades and
    // sends its real `device_id`, which then collapses the legacy row.
    if device_id.is_empty() {
        return Ok(token.to_owned());
    }
    if device_id.len() < MIN_DEVICE_ID_BYTES || device_id.len() > MAX_DEVICE_ID_BYTES {
        return Err(StoreError::invalid("device_id has an invalid length"));
    }
    let printable = device_id
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || "-_.:".contains(character));
    if !printable {
        return Err(StoreError::invalid("device_id has invalid characters"));
    }
    Ok(device_id.to_owned())
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
        device_id: clean_device_id(input, token)?,
        token: token.to_owned(),
        platform,
        app_version: clean_meta(&input.app_version, "app_version")?,
        device_name: clean_meta(&input.device_name, "device_name")?,
    })
}

impl Store {
    /// Inserts or refreshes one device, keyed by `device_id`. A rotated FCM
    /// token updates that device's row; a token that was previously attached
    /// to a legacy token-keyed row or to another device identity is detached
    /// from the old row, so every token maps to exactly one device. An empty
    /// name keeps the stored one, so a token refresh never erases the name set
    /// in the app.
    pub fn register_push_device(
        &self,
        input: &PushDeviceInput,
        now_unix: i64,
    ) -> Result<PushDevice, StoreError> {
        let mut device = validate_device(input)?;
        if device.device_name.is_empty() {
            // The first registration from an upgraded app collapses a
            // token-keyed v3 row; carry that row's name so the owner does not
            // have to set it again.
            if let Some(legacy) = self.push_device_by_token(&device.token)? {
                device.device_name = legacy.device_name;
            }
        }
        let count = self.list_push_devices()?.len();
        let known = self.push_device(&device.device_id)?.is_some();
        if !known && count >= MAX_PUSH_DEVICES {
            return Err(StoreError::limit("too many push devices"));
        }
        let device_id = device.device_id.clone();
        self.call(move |connection| {
            connection.execute(
                "DELETE FROM push_devices WHERE token = ?1 AND device_id <> ?2",
                params![device.token, device.device_id],
            )?;
            connection.execute(
                "INSERT INTO push_devices (device_id, token, platform, app_version, device_name, created_unix, updated_unix)
				 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
				 ON CONFLICT(device_id) DO UPDATE SET
					token = excluded.token,
					platform = excluded.platform,
					app_version = excluded.app_version,
					device_name = CASE WHEN excluded.device_name = ''
						THEN push_devices.device_name ELSE excluded.device_name END,
					updated_unix = excluded.updated_unix",
                params![
                    device.device_id,
                    device.token,
                    device.platform,
                    device.app_version,
                    device.device_name,
                    now_unix
                ],
            )?;
            Ok(())
        })?;
        self.push_device(&device_id)?.ok_or(StoreError::NotFound)
    }

    /// Renames one registered device without re-registering its token.
    pub fn rename_push_device(
        &self,
        device_id: &str,
        device_name: &str,
        now_unix: i64,
    ) -> Result<PushDevice, StoreError> {
        let device_id = device_id.trim().to_owned();
        if device_id.is_empty() {
            return Err(StoreError::invalid("device_id is required"));
        }
        let name = clean_meta(device_name, "device_name")?;
        if name.is_empty() {
            return Err(StoreError::invalid("device_name is required"));
        }
        let key = device_id.clone();
        let changed = self.call(move |connection| {
            connection.execute(
                "UPDATE push_devices SET device_name = ?1, updated_unix = ?2 WHERE device_id = ?3",
                params![name, now_unix, key],
            )
        })?;
        if changed == 0 {
            return Err(StoreError::NotFound);
        }
        self.push_device(device_id.trim())
            .and_then(|device| device.ok_or(StoreError::NotFound))
    }

    /// Reads one device by its stable identity.
    pub fn push_device(&self, device_id: &str) -> Result<Option<PushDevice>, StoreError> {
        let device_id = device_id.to_owned();
        self.call(move |connection| {
            let mut statement = connection.prepare(&format!(
                "SELECT {COLS} FROM push_devices WHERE device_id = ?1"
            ))?;
            let mut rows = statement.query(params![device_id])?;
            match rows.next()? {
                Some(row) => Ok(Some(scan_device(row)?)),
                None => Ok(None),
            }
        })
    }

    /// Reads one device by its current FCM token.
    pub fn push_device_by_token(&self, token: &str) -> Result<Option<PushDevice>, StoreError> {
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
                "SELECT {COLS} FROM push_devices ORDER BY updated_unix DESC, device_id ASC"
            ))?;
            let rows = statement.query_map([], scan_device)?;
            let mut devices = Vec::new();
            for row in rows {
                devices.push(row?);
            }
            Ok(devices)
        })
    }

    /// Removes one device by token; `false` means it was already gone.
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
    use crate::store::{Config, MIGRATIONS};

    fn store() -> Store {
        Store::open_in_memory(Config::default()).expect("store")
    }

    /// A throwaway database path; the scheduler crate has no `tempfile`
    /// dependency, and Bazel tests run from a sandboxed working directory.
    fn temp_database(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "scheduler-push-{}-{nanos}-{name}",
            std::process::id()
        ));
        std::fs::create_dir_all(&directory).expect("temp directory");
        (directory.clone(), directory.join("scheduler.db"))
    }

    fn input(token: &str) -> PushDeviceInput {
        PushDeviceInput {
            device_id: "device-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
            token: token.to_owned(),
            platform: "android".to_owned(),
            app_version: "0.1.0".to_owned(),
            device_name: "Phone".to_owned(),
        }
    }

    const TOKEN_A: &str = "token-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const TOKEN_B: &str = "token-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn registering_the_same_device_refreshes_instead_of_duplicating() {
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
    fn a_rotated_token_updates_the_same_device() {
        let store = store();
        let first = store
            .register_push_device(&input(TOKEN_A), 100)
            .expect("register");
        let rotated = store
            .register_push_device(&input(TOKEN_B), 200)
            .expect("rotate");
        assert_eq!(rotated.device_id, first.device_id);
        assert_eq!(rotated.token, TOKEN_B);
        assert_eq!(rotated.created_unix, 100);
        assert_eq!(rotated.updated_unix, 200);
        assert_eq!(store.list_push_devices().expect("list").len(), 1);
        assert!(store.push_device_by_token(TOKEN_A).expect("old").is_none());
    }

    #[test]
    fn a_token_moved_to_another_device_identity_leaves_one_row() {
        let store = store();
        store
            .register_push_device(&input(TOKEN_A), 100)
            .expect("register");
        store
            .register_push_device(
                &PushDeviceInput {
                    device_id: "device-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned(),
                    ..input(TOKEN_A)
                },
                200,
            )
            .expect("move");
        let devices = store.list_push_devices().expect("list");
        assert_eq!(devices.len(), 1);
        assert_eq!(
            devices[0].device_id,
            "device-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        );
    }

    #[test]
    fn an_empty_name_on_refresh_keeps_the_stored_name() {
        let store = store();
        store
            .register_push_device(&input(TOKEN_A), 100)
            .expect("register");
        let refreshed = store
            .register_push_device(
                &PushDeviceInput {
                    device_name: String::new(),
                    ..input(TOKEN_B)
                },
                200,
            )
            .expect("refresh");
        assert_eq!(refreshed.device_name, "Phone");
    }

    #[test]
    fn renaming_does_not_touch_the_token_or_creation_time() {
        let store = store();
        let first = store
            .register_push_device(&input(TOKEN_A), 100)
            .expect("register");
        let renamed = store
            .rename_push_device(&first.device_id, "  Стас  ", 300)
            .expect("rename");
        assert_eq!(renamed.device_name, "Стас");
        assert_eq!(renamed.token, TOKEN_A);
        assert_eq!(renamed.created_unix, 100);
        assert_eq!(renamed.updated_unix, 300);
        assert!(matches!(
            store.rename_push_device("device-missing", "Стас", 300),
            Err(StoreError::NotFound)
        ));
        assert!(
            store
                .rename_push_device(&first.device_id, "  ", 300)
                .is_err()
        );
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
    fn an_invalid_device_id_is_rejected() {
        let store = store();
        let bad = PushDeviceInput {
            device_id: "short".to_owned(),
            ..input(TOKEN_A)
        };
        assert!(store.register_push_device(&bad, 1).is_err());
        let spaced = PushDeviceInput {
            device_id: "device with space".to_owned(),
            ..input(TOKEN_A)
        };
        assert!(store.register_push_device(&spaced, 1).is_err());
    }

    #[test]
    fn an_empty_device_id_falls_back_to_the_token() {
        let store = store();
        let legacy = PushDeviceInput {
            device_id: String::new(),
            ..input(TOKEN_A)
        };
        let device = store.register_push_device(&legacy, 1).expect("register");
        assert_eq!(device.device_id, TOKEN_A);
    }

    #[test]
    fn the_device_count_is_bounded() {
        let store = store();
        store
            .register_push_device(&input(TOKEN_A), 1)
            .expect("register");
        for index in 0..MAX_PUSH_DEVICES - 1 {
            let token = format!("token-{index:04}-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
            let device = PushDeviceInput {
                device_id: format!("device-{index:04}-aaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                ..input(&token)
            };
            store.register_push_device(&device, 1).expect("register");
        }
        let extra = PushDeviceInput {
            device_id: "device-cccccccccccccccccccccccccccccccc".to_owned(),
            ..input(TOKEN_B)
        };
        assert!(store.register_push_device(&extra, 1).is_err());
        // Registering an existing device is still allowed at the cap.
        assert!(store.register_push_device(&input(TOKEN_A), 2).is_ok());
    }

    #[tokio::test]
    async fn a_v3_token_keyed_row_survives_and_collapses_on_the_first_device_registration() {
        let (directory, path) = temp_database("collapse");
        {
            let db = home_sqlite::Db::open(&path, &MIGRATIONS[..3]).expect("v3 database");
            db.call(|connection| {
                connection.execute(
                    "INSERT INTO push_devices (token, platform, app_version, device_name, created_unix, updated_unix)
                     VALUES (?1, 'android', '0.1.0', 'Pixel', 100, 100)",
                    params![TOKEN_A],
                )?;
                Ok(())
            })
            .await
            .expect("insert v3 row");
            db.shutdown().expect("close");
        }
        let store = Store::open(&path, Config::default()).expect("migrates to v4");
        let migrated = store.list_push_devices().expect("list");
        assert_eq!(migrated.len(), 1);
        assert_eq!(migrated[0].device_id, TOKEN_A);
        assert_eq!(migrated[0].device_name, "Pixel");

        let device = store
            .register_push_device(
                &PushDeviceInput {
                    device_id: "device-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
                    device_name: String::new(),
                    ..input(TOKEN_A)
                },
                200,
            )
            .expect("collapses the legacy row");
        assert_eq!(device.device_id, "device-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        assert_eq!(device.device_name, "Pixel");
        assert_eq!(device.created_unix, 200);
        assert_eq!(store.list_push_devices().expect("list").len(), 1);
    }

    #[tokio::test]
    async fn a_v3_database_migrates_with_its_push_device_intact() {
        let (directory, path) = temp_database("collapse");
        {
            let db = home_sqlite::Db::open(&path, &MIGRATIONS[..3]).expect("v3 database");
            db.call(|connection| {
                connection.execute(
                    "INSERT INTO push_devices (token, platform, app_version, device_name, created_unix, updated_unix)
                     VALUES (?1, 'android', '0.1.0', 'Pixel', 100, 100)",
                    params![TOKEN_A],
                )?;
                Ok(())
            })
            .await
            .expect("insert v3 row");
            db.shutdown().expect("close");
        }
        let store = Store::open(&path, Config::default()).expect("migrates to v4");
        let devices = store.list_push_devices().expect("list");
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].token, TOKEN_A);
        assert_eq!(devices[0].device_id, TOKEN_A);
        assert_eq!(
            store.status(0).expect("status").schema_version,
            crate::store::SCHEMA_VERSION
        );
        std::fs::remove_dir_all(directory).expect("cleanup");
    }
}
