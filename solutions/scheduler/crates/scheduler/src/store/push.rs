use rusqlite::{OptionalExtension, Row, Transaction, params};

use super::{Store, StoreError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushDevice {
    pub device_id: String,
    pub token: String,
    pub platform: String,
    pub app_version: String,
    pub device_name: String,
    pub token_id: String,
    pub created_unix: i64,
    pub updated_unix: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PushDeviceInput {
    pub device_id: String,
    pub token: String,
    pub platform: String,
    pub app_version: String,
    pub device_name: String,
}

pub const MAX_PUSH_DEVICES: usize = 16;
pub const MAX_TOKEN_BYTES: usize = 4096;
pub const MIN_TOKEN_BYTES: usize = 32;
pub const MAX_META_BYTES: usize = 64;
pub const MIN_DEVICE_ID_BYTES: usize = 8;
pub const MAX_DEVICE_ID_BYTES: usize = 128;

const COLS: &str =
    "device_id, token, platform, app_version, device_name, token_id, created_unix, updated_unix";

fn scan_device(row: &Row<'_>) -> rusqlite::Result<PushDevice> {
    Ok(PushDevice {
        device_id: row.get(0)?,
        token: row.get(1)?,
        platform: row.get(2)?,
        app_version: row.get(3)?,
        device_name: row.get(4)?,
        token_id: row.get(5)?,
        created_unix: row.get(6)?,
        updated_unix: row.get(7)?,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceScope {
    All,
    Token(String),
}

fn device_where(
    transaction: &Transaction<'_>,
    column: &str,
    value: &str,
) -> Result<Option<PushDevice>, StoreError> {
    let mut statement = transaction.prepare(&format!(
        "SELECT {COLS} FROM push_devices WHERE {column} = ?1"
    ))?;
    Ok(statement
        .query_row(params![value], scan_device)
        .optional()?)
}

fn may_take(current: &PushDevice, token_id: &str, owner: bool, fcm_token: &str) -> bool {
    owner
        || current.token_id.is_empty()
        || current.token_id == token_id
        || current.token == fcm_token
}

fn register_in(
    transaction: &Transaction<'_>,
    mut device: PushDeviceInput,
    token_id: &str,
    owner: bool,
    now_unix: i64,
) -> Result<PushDevice, StoreError> {
    let existing = device_where(transaction, "device_id", &device.device_id)?;
    if let Some(current) = existing.as_ref()
        && !may_take(current, token_id, owner, &device.token)
    {
        return Err(StoreError::precondition(
            "push device belongs to another application token",
        ));
    }
    if let Some(previous) = device_where(transaction, "token", &device.token)?
        && previous.device_id != device.device_id
    {
        if device.device_name.is_empty() {
            device.device_name.clone_from(&previous.device_name);
        }
        transaction.execute(
            "DELETE FROM notifications WHERE device_id = ?1",
            params![previous.device_id],
        )?;
        transaction.execute(
            "DELETE FROM push_devices WHERE device_id = ?1",
            params![previous.device_id],
        )?;
    }
    if existing.is_none() {
        let count: i64 =
            transaction.query_row("SELECT COUNT(*) FROM push_devices", [], |row| row.get(0))?;
        if usize::try_from(count).unwrap_or(usize::MAX) >= MAX_PUSH_DEVICES {
            return Err(StoreError::limit("too many push devices"));
        }
    }
    transaction.execute(
        "INSERT INTO push_devices (device_id, token, platform, app_version, device_name, token_id, created_unix, updated_unix)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
         ON CONFLICT(device_id) DO UPDATE SET
            token = excluded.token,
            platform = excluded.platform,
            app_version = excluded.app_version,
            device_name = CASE WHEN excluded.device_name = ''
                THEN push_devices.device_name ELSE excluded.device_name END,
            token_id = CASE WHEN excluded.token_id = ''
                THEN push_devices.token_id ELSE excluded.token_id END,
            updated_unix = excluded.updated_unix",
        params![
            device.device_id,
            device.token,
            device.platform,
            device.app_version,
            device.device_name,
            token_id,
            now_unix
        ],
    )?;
    device_where(transaction, "device_id", &device.device_id)?.ok_or(StoreError::NotFound)
}

impl Store {
    #[cfg(test)]
    pub fn register_push_device(
        &self,
        input: &PushDeviceInput,
        now_unix: i64,
    ) -> Result<PushDevice, StoreError> {
        self.register_push_device_for(input, "", true, now_unix)
    }

    pub fn register_push_device_for(
        &self,
        input: &PushDeviceInput,
        token_id: &str,
        owner: bool,
        now_unix: i64,
    ) -> Result<PushDevice, StoreError> {
        let device = validate_device(input)?;
        let token_id = token_id.to_owned();
        self.with_tx(move |transaction| {
            register_in(transaction, device, &token_id, owner, now_unix)
        })
    }

    #[cfg(test)]
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

    pub fn backfill_push_device_owners(&self, legacy_id: &str) -> Result<usize, StoreError> {
        let legacy_id = legacy_id.to_owned();
        self.call(move |connection| {
            let changed = connection.execute(
                "UPDATE push_devices SET token_id = ?1 WHERE token_id = ''",
                params![legacy_id],
            )?;
            Ok(changed)
        })
    }

    pub fn list_push_devices_for(&self, scope: DeviceScope) -> Result<Vec<PushDevice>, StoreError> {
        let token_id = match scope {
            DeviceScope::All => None,
            DeviceScope::Token(id) => Some(id),
        };
        self.call(move |connection| {
            let mut devices = Vec::new();
            if let Some(id) = token_id.as_deref() {
                let mut statement = connection.prepare(&format!(
                    "SELECT {COLS} FROM push_devices WHERE token_id = ?1 ORDER BY updated_unix DESC, device_id ASC"
                ))?;
                let rows = statement.query_map(params![id], scan_device)?;
                for row in rows {
                    devices.push(row?);
                }
            } else {
                let mut statement = connection.prepare(&format!(
                    "SELECT {COLS} FROM push_devices ORDER BY updated_unix DESC, device_id ASC"
                ))?;
                let rows = statement.query_map([], scan_device)?;
                for row in rows {
                    devices.push(row?);
                }
            }
            Ok(devices)
        })
    }

    pub fn rename_push_device_for(
        &self,
        device_id: &str,
        device_name: &str,
        scope: DeviceScope,
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
        let token_id = match scope {
            DeviceScope::All => None,
            DeviceScope::Token(id) => Some(id),
        };
        let changed = self.call(move |connection| {
            token_id.as_deref().map_or_else(
                || {
                    connection.execute(
                        "UPDATE push_devices SET device_name = ?1, updated_unix = ?2 WHERE device_id = ?3",
                        params![name, now_unix, key],
                    )
                },
                |token| {
                    connection.execute(
                        "UPDATE push_devices SET device_name = ?1, updated_unix = ?2 WHERE device_id = ?3 AND token_id = ?4",
                        params![name, now_unix, key, token],
                    )
                },
            )
        })?;
        if changed == 0 {
            return Err(StoreError::NotFound);
        }
        self.push_device(device_id.trim())
            .and_then(|device| device.ok_or(StoreError::NotFound))
    }

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

    #[cfg(test)]
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

    #[cfg(test)]
    pub fn remove_push_device(&self, token: &str) -> Result<bool, StoreError> {
        self.remove_push_device_for(token, DeviceScope::All)
    }

    pub fn remove_push_device_for(
        &self,
        token: &str,
        scope: DeviceScope,
    ) -> Result<bool, StoreError> {
        let token = token.to_owned();
        let token_id = match scope {
            DeviceScope::All => None,
            DeviceScope::Token(id) => Some(id),
        };
        self.call(move |connection| {
            let transaction = connection.transaction()?;
            let deleted = if let Some(id) = token_id.as_deref() {
                transaction.execute(
                    "DELETE FROM notifications WHERE device_id IN (
                        SELECT device_id FROM push_devices WHERE token = ?1 AND token_id = ?2
                    )",
                    params![token, id],
                )?;
                transaction.execute(
                    "DELETE FROM push_devices WHERE token = ?1 AND token_id = ?2",
                    params![token, id],
                )?
            } else {
                transaction.execute(
                    "DELETE FROM notifications WHERE device_id IN (
                        SELECT device_id FROM push_devices WHERE token = ?1
                    )",
                    params![token],
                )?;
                transaction.execute("DELETE FROM push_devices WHERE token = ?1", params![token])?
            };
            transaction.commit()?;
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
        assert!(store.register_push_device(&input(TOKEN_A), 2).is_ok());
    }

    #[tokio::test]
    async fn a_v3_token_keyed_row_survives_and_collapses_on_the_first_device_registration() {
        let (_directory, path) = temp_database("collapse");
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
    #[test]
    fn devices_are_scoped_to_the_registering_token() {
        let store = store();
        let mut owner_input = input(TOKEN_A);
        owner_input.device_name = "Стас".to_owned();
        let owner = store
            .register_push_device_for(&owner_input, "owner", false, 100)
            .expect("owner device");
        let mut mama_input = input(TOKEN_B);
        mama_input.device_id = "device-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned();
        mama_input.device_name = "Мама".to_owned();
        let mama = store
            .register_push_device_for(&mama_input, "mama", false, 101)
            .expect("member device");
        assert_eq!(owner.token_id, "owner");
        assert_eq!(mama.token_id, "mama");

        let own = store
            .list_push_devices_for(DeviceScope::Token("mama".to_owned()))
            .expect("own list");
        assert_eq!(own.len(), 1);
        assert_eq!(own[0].device_id, mama.device_id);
        assert_eq!(
            store
                .list_push_devices_for(DeviceScope::All)
                .expect("all")
                .len(),
            2
        );
        assert!(
            store
                .rename_push_device_for(
                    &owner.device_id,
                    "Чужой",
                    DeviceScope::Token("mama".to_owned()),
                    102,
                )
                .is_err()
        );
        assert!(
            !store
                .remove_push_device_for(TOKEN_A, DeviceScope::Token("mama".to_owned()))
                .expect("remove scope")
        );
        let mut foreign = input(TOKEN_B);
        foreign.device_id.clone_from(&owner.device_id);
        assert!(
            store
                .register_push_device_for(&foreign, "mama", false, 103)
                .is_err()
        );
    }

    #[tokio::test]
    async fn a_v8_database_migrates_with_devices_and_empty_ownership() {
        let (directory, path) = temp_database("v9");
        {
            let db = home_sqlite::Db::open(&path, &MIGRATIONS[..8]).expect("v8 database");
            db.call(|connection| {
                connection.execute(
                    "INSERT INTO push_devices (device_id, token, platform, app_version, device_name, created_unix, updated_unix)
                     VALUES ('device-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', ?1, 'android', '0.1.0', 'Стас', 100, 100)",
                    params![TOKEN_A],
                )?;
                Ok(())
            })
            .await
            .expect("insert v8 row");
            db.shutdown().expect("close");
        }
        let store = Store::open(&path, Config::default()).expect("migrates to v9");
        let devices = store.list_push_devices().expect("list");
        assert_eq!(devices[0].token_id, "");
        assert_eq!(
            store.backfill_push_device_owners("mama").expect("backfill"),
            1
        );
        assert_eq!(store.backfill_push_device_owners("mama").expect("again"), 0);
        assert_eq!(store.list_push_devices().expect("list")[0].token_id, "mama");
        let refreshed = store
            .register_push_device_for(&input(TOKEN_A), "mama", false, 200)
            .expect("the legacy phone re-registers without a conflict");
        assert_eq!(refreshed.token_id, "mama");
        std::fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn the_same_app_install_can_move_to_a_new_token_but_others_cannot_take_it() {
        let store = store();
        store
            .register_push_device_for(&input(TOKEN_A), "mama", false, 100)
            .expect("mama");
        let moved = store
            .register_push_device_for(&input(TOKEN_A), "papa", false, 101)
            .expect("same FCM token proves the same install");
        assert_eq!(moved.token_id, "papa");
        let error = store
            .register_push_device_for(&input(TOKEN_B), "guest", false, 102)
            .expect_err("a foreign install cannot take the device id");
        assert_eq!(error.kind(), Some(crate::domain::ErrorKind::Precondition));
        store
            .register_push_device_for(&input(TOKEN_B), "owner", true, 103)
            .expect("the owner may claim");
    }

    #[test]
    fn a_reinstall_with_a_new_device_id_replaces_the_old_row() {
        let store = store();
        store
            .register_push_device_for(&input(TOKEN_A), "mama", false, 100)
            .expect("first");
        let mut other = input(TOKEN_A);
        other.device_id = "device-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned();
        other.device_name = String::new();
        let device = store
            .register_push_device_for(&other, "guest", false, 101)
            .expect("same FCM token, new device id");
        assert_eq!(device.device_name, "Phone");
        assert_eq!(store.list_push_devices().expect("list").len(), 1);
    }

    #[test]
    fn a_failed_registration_leaves_no_partial_changes() {
        let store = store();
        store
            .register_push_device_for(&input(TOKEN_A), "owner", true, 100)
            .expect("owner");
        let mut stolen = input(TOKEN_A);
        stolen.device_id = "device-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned();
        store
            .register_push_device_for(&stolen, "mama", false, 101)
            .expect("same install");
        let mut conflicting = input(TOKEN_B);
        conflicting.device_id = "device-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned();
        assert!(
            store
                .register_push_device_for(&conflicting, "guest", false, 102)
                .is_err()
        );
        let devices = store.list_push_devices().expect("list");
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].token, TOKEN_A);
        assert_eq!(devices[0].token_id, "mama");
    }
}
