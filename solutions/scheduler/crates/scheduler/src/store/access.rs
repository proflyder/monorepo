use home_app_auth::{AccessSnapshot, Module, Modules, TokenRecord};
use rusqlite::{Connection, OptionalExtension, Row, params};

use super::{Store, StoreError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessToken {
    pub id: String,
    pub name: String,
    pub modules: Modules,
    pub owner: bool,
    pub created_unix: i64,
    pub updated_unix: i64,
}

const COLS: &str =
    "id, name, sha256, modules_json, is_owner, created_unix, updated_unix, revoked_unix";

fn scan_token(row: &Row<'_>) -> rusqlite::Result<AccessToken> {
    let modules_json: String = row.get(3)?;
    let modules = serde_json::from_str::<Vec<String>>(&modules_json)
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })
        .and_then(|values| {
            Modules::parse(&values).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    3,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })
        })?;
    Ok(AccessToken {
        id: row.get(0)?,
        name: row.get(1)?,
        modules,
        owner: row.get::<_, i64>(4)? != 0,
        created_unix: row.get(5)?,
        updated_unix: row.get(6)?,
    })
}

fn token(connection: &Connection, id: &str) -> rusqlite::Result<Option<AccessToken>> {
    let mut statement = connection.prepare(&format!(
        "SELECT {COLS} FROM app_tokens WHERE id = ? AND revoked_unix = 0"
    ))?;
    statement.query_row(params![id], scan_token).optional()
}

fn revision(connection: &Connection) -> rusqlite::Result<i64> {
    connection.query_row("SELECT revision FROM access_meta WHERE id = 1", [], |row| {
        row.get(0)
    })
}

fn bump(connection: &Connection) -> rusqlite::Result<i64> {
    connection.execute(
        "UPDATE access_meta SET revision = revision + 1 WHERE id = 1",
        [],
    )?;
    revision(connection)
}

fn clean_name(name: &str) -> Result<String, StoreError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(StoreError::invalid("token name is required"));
    }
    if name.chars().count() > 64 {
        return Err(StoreError::invalid("token name is too long"));
    }
    if name.chars().any(char::is_control) {
        return Err(StoreError::invalid("token name has control characters"));
    }
    Ok(name.to_owned())
}

fn clean_modules(modules: &Modules, owner: bool) -> Result<Vec<String>, StoreError> {
    let mut modules = modules.clone();
    modules.insert(Module::Base);
    if owner {
        modules.insert(Module::Admin);
    }
    Ok(modules.ids().into_iter().map(str::to_owned).collect())
}

pub fn snapshot(connection: &Connection) -> Result<AccessSnapshot, StoreError> {
    let mut statement = connection.prepare(&format!(
        "SELECT {COLS} FROM app_tokens WHERE revoked_unix = 0 ORDER BY created_unix, id"
    ))?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)? != 0,
        ))
    })?;
    let mut tokens = Vec::new();
    for row in rows {
        let (id, name, sha256, modules_json, owner) = row?;
        let modules: Vec<String> = serde_json::from_str(&modules_json)
            .map_err(|error| StoreError::invalid(format!("token {name} modules: {error}")))?;
        tokens.push(TokenRecord {
            id,
            name,
            sha256,
            owner,
            modules,
        });
    }
    Ok(AccessSnapshot {
        revision: revision(connection)?,
        tokens,
    })
}

impl Store {
    pub fn access_snapshot(&self) -> Result<AccessSnapshot, StoreError> {
        self.read(snapshot)
    }

    pub fn access_legacy_imported(&self) -> Result<bool, StoreError> {
        self.read(|connection| {
            let value: i64 = connection.query_row(
                "SELECT legacy_imported FROM access_meta WHERE id = 1",
                [],
                |row| row.get(0),
            )?;
            Ok(value != 0)
        })
    }

    pub fn access_import_legacy(&self, digest: &str, now_unix: i64) -> Result<bool, StoreError> {
        let digest = digest.to_owned();
        self.with_tx(move |transaction| {
            let imported: i64 = transaction.query_row(
                "SELECT legacy_imported FROM access_meta WHERE id = 1",
                [],
                |row| row.get(0),
            )?;
            if imported != 0 {
                return Ok(false);
            }
            let exists: i64 = transaction.query_row(
                "SELECT COUNT(*) FROM app_tokens WHERE id = 'mama'",
                [],
                |row| row.get(0),
            )?;
            if exists == 0 {
                let modules = Modules::all_but_admin();
                transaction.execute(
                    "INSERT INTO app_tokens (id, name, sha256, modules_json, is_owner, created_unix, updated_unix, revoked_unix)
                     VALUES ('mama', 'Мама', ?, ?, 0, ?, ?, 0)",
                    params![
                        digest,
                        serde_json::to_string(&clean_modules(&modules, false)?)
                            .map_err(|error| StoreError::invalid(error.to_string()))?,
                        now_unix,
                        now_unix
                    ],
                )?;
                bump(transaction)?;
            }
            transaction.execute("UPDATE access_meta SET legacy_imported = 1 WHERE id = 1", [])?;
            Ok(true)
        })
    }

    pub fn access_import_seed(
        &self,
        snapshot: &AccessSnapshot,
        now_unix: i64,
    ) -> Result<bool, StoreError> {
        if snapshot.tokens.is_empty() {
            return Ok(false);
        }
        let snapshot = snapshot.clone();
        self.with_tx(move |transaction| {
            let mut changed = false;
            for record in &snapshot.tokens {
                let existing = token(transaction, &record.id)?;
                if let Some(existing) = existing {
                    if record.owner && existing.owner {
                        let modules = serde_json::to_string(&record.modules)
                            .map_err(|error| StoreError::invalid(error.to_string()))?;
                        let digest: String = transaction.query_row(
                            "SELECT sha256 FROM app_tokens WHERE id = ?1",
                            params![record.id],
                            |row| row.get(0),
                        )?;
                        if digest != record.sha256 {
                            transaction.execute(
                                "UPDATE app_tokens SET sha256 = ?, modules_json = ?, name = ?, updated_unix = ? WHERE id = ?",
                                params![record.sha256, modules, record.name, now_unix, record.id],
                            )?;
                            changed = true;
                        }
                    }
                    continue;
                }
                let name_taken: i64 = transaction.query_row(
                    "SELECT COUNT(*) FROM app_tokens WHERE name = ?",
                    params![record.name],
                    |row| row.get(0),
                )?;
                if name_taken != 0 {
                    continue;
                }
                transaction.execute(
                    "INSERT INTO app_tokens (id, name, sha256, modules_json, is_owner, created_unix, updated_unix, revoked_unix)
                     VALUES (?, ?, ?, ?, ?, ?, ?, 0)",
                    params![
                        record.id,
                        record.name,
                        record.sha256,
                        serde_json::to_string(&record.modules)
                            .map_err(|error| StoreError::invalid(error.to_string()))?,
                        i64::from(record.owner),
                        now_unix,
                        now_unix
                    ],
                )?;
                changed = true;
            }
            if changed {
                bump(transaction)?;
            }
            Ok(changed)
        })
    }

    pub fn access_list(&self) -> Result<Vec<AccessToken>, StoreError> {
        self.read(|connection| {
            let mut statement = connection.prepare(&format!(
                "SELECT {COLS} FROM app_tokens WHERE revoked_unix = 0 ORDER BY created_unix, id"
            ))?;
            let rows = statement.query_map([], scan_token)?;
            let mut tokens = Vec::new();
            for row in rows {
                tokens.push(row?);
            }
            Ok(tokens)
        })
    }

    pub fn access_token(&self, id: &str) -> Result<AccessToken, StoreError> {
        let id = id.to_owned();
        self.read(move |connection| token(connection, &id)?.ok_or(StoreError::NotFound))
    }

    pub fn access_create(
        &self,
        id: &str,
        name: &str,
        digest: &str,
        modules: &Modules,
        now_unix: i64,
    ) -> Result<AccessToken, StoreError> {
        let name = clean_name(name)?;
        if modules.contains(Module::Admin) {
            return Err(StoreError::precondition(
                "the admin module belongs to the owner token",
            ));
        }
        let modules_json = serde_json::to_string(&clean_modules(modules, false)?)
            .map_err(|error| StoreError::invalid(error.to_string()))?;
        let id = id.to_owned();
        let digest = digest.to_owned();
        self.with_tx(move |transaction| {
            let duplicate: i64 = transaction.query_row(
                "SELECT COUNT(*) FROM app_tokens WHERE lower(name) = lower(?) AND revoked_unix = 0",
                params![name],
                |row| row.get(0),
            )?;
            if duplicate != 0 {
                return Err(StoreError::invalid("token name is already used"));
            }
            transaction.execute(
                "INSERT INTO app_tokens (id, name, sha256, modules_json, is_owner, created_unix, updated_unix, revoked_unix)
                 VALUES (?, ?, ?, ?, 0, ?, ?, 0)",
                params![id, name, digest, modules_json, now_unix, now_unix],
            )?;
            bump(transaction)?;
            token(transaction, &id)?.ok_or(StoreError::NotFound)
        })
    }

    pub fn access_rename(
        &self,
        id: &str,
        name: &str,
        now_unix: i64,
    ) -> Result<AccessToken, StoreError> {
        let name = clean_name(name)?;
        let id = id.to_owned();
        self.with_tx(move |transaction| {
            token(transaction, &id)?.ok_or(StoreError::NotFound)?;
            let duplicate: i64 = transaction.query_row(
                "SELECT COUNT(*) FROM app_tokens WHERE lower(name) = lower(?) AND id != ? AND revoked_unix = 0",
                params![name, id],
                |row| row.get(0),
            )?;
            if duplicate != 0 {
                return Err(StoreError::invalid("token name is already used"));
            }
            transaction.execute(
                "UPDATE app_tokens SET name = ?, updated_unix = ? WHERE id = ?",
                params![name, now_unix, id],
            )?;
            bump(transaction)?;
            token(transaction, &id)?.ok_or(StoreError::NotFound)
        })
    }

    pub fn access_set_modules(
        &self,
        id: &str,
        modules: &Modules,
        now_unix: i64,
    ) -> Result<AccessToken, StoreError> {
        let id = id.to_owned();
        let requested = modules.clone();
        self.with_tx(move |transaction| {
            let current = token(transaction, &id)?.ok_or(StoreError::NotFound)?;
            if current.owner && !requested.contains(Module::Admin) {
                return Err(StoreError::precondition(
                    "owner token must keep the admin module",
                ));
            }
            if !current.owner && requested.contains(Module::Admin) {
                return Err(StoreError::precondition(
                    "the admin module belongs to the owner token",
                ));
            }
            let modules_json = serde_json::to_string(&clean_modules(&requested, current.owner)?)
                .map_err(|error| StoreError::invalid(error.to_string()))?;
            transaction.execute(
                "UPDATE app_tokens SET modules_json = ?, updated_unix = ? WHERE id = ?",
                params![modules_json, now_unix, id],
            )?;
            bump(transaction)?;
            token(transaction, &id)?.ok_or(StoreError::NotFound)
        })
    }

    pub fn access_revoke(&self, id: &str, now_unix: i64) -> Result<(), StoreError> {
        let id = id.to_owned();
        self.with_tx(move |transaction| {
            let current = token(transaction, &id)?.ok_or(StoreError::NotFound)?;
            if current.owner {
                return Err(StoreError::precondition("owner token cannot be revoked"));
            }
            transaction.execute(
                "DELETE FROM notifications WHERE device_id IN (
                    SELECT device_id FROM push_devices WHERE token_id = ?1
                )",
                params![id],
            )?;
            transaction.execute("DELETE FROM push_devices WHERE token_id = ?1", params![id])?;
            transaction.execute(
                "UPDATE app_tokens SET revoked_unix = ?, updated_unix = ? WHERE id = ?",
                params![now_unix, now_unix, id],
            )?;
            bump(transaction)?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use home_app_auth::{Modules, sha256_hex};

    use super::*;
    use crate::store::Config;

    fn store() -> Store {
        Store::open_in_memory(Config::default()).expect("store")
    }

    #[test]
    fn the_legacy_token_is_imported_once_as_mama() {
        let store = store();
        assert!(
            store
                .access_import_legacy(&sha256_hex("legacy"), 100)
                .expect("import")
        );
        assert!(
            !store
                .access_import_legacy(&sha256_hex("legacy"), 101)
                .expect("idempotent")
        );
        let tokens = store.access_list().expect("list");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].name, "Мама");
        assert_eq!(tokens[0].modules.ids(), Modules::all_but_admin().ids());
    }

    #[test]
    fn a_seed_adds_the_owner_only_until_an_owner_exists() {
        let store = store();
        let snapshot = AccessSnapshot {
            revision: 1,
            tokens: vec![TokenRecord {
                id: "owner".to_owned(),
                name: "owner".to_owned(),
                sha256: sha256_hex("owner-token"),
                owner: true,
                modules: Modules::owner()
                    .ids()
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            }],
        };
        assert!(store.access_import_seed(&snapshot, 10).expect("import"));
        assert!(!store.access_import_seed(&snapshot, 11).expect("idempotent"));
        let owner = store.access_token("owner").expect("owner");
        assert!(owner.owner);
        assert!(owner.modules.contains(Module::Admin));
    }

    #[test]
    fn the_owner_cannot_lose_admin_or_be_revoked() {
        let store = store();
        let snapshot = AccessSnapshot {
            revision: 1,
            tokens: vec![TokenRecord {
                id: "owner".to_owned(),
                name: "owner".to_owned(),
                sha256: sha256_hex("owner-token"),
                owner: true,
                modules: Modules::owner()
                    .ids()
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            }],
        };
        store.access_import_seed(&snapshot, 10).expect("import");
        assert!(
            store
                .access_set_modules("owner", &Modules::member(), 11)
                .is_err()
        );
        assert!(store.access_revoke("owner", 11).is_err());
    }

    #[test]
    fn create_rename_update_and_revoke_change_the_snapshot() {
        let store = store();
        let created = store
            .access_create(
                "phone",
                "Phone",
                &sha256_hex("phone-token"),
                &Modules::member(),
                10,
            )
            .expect("create");
        assert!(created.modules.contains(Module::Notifications));
        let renamed = store.access_rename("phone", "Kitchen", 11).expect("rename");
        assert_eq!(renamed.name, "Kitchen");
        let updated = store
            .access_set_modules("phone", &Modules::member(), 12)
            .expect("modules");
        assert_eq!(updated.modules.ids(), Modules::member().ids());
        let revision = store.access_snapshot().expect("snapshot").revision;
        store.access_revoke("phone", 13).expect("revoke");
        let snapshot = store.access_snapshot().expect("snapshot");
        assert!(snapshot.tokens.is_empty());
        assert!(snapshot.revision > revision);
        assert!(
            store
                .access_create(
                    "dup",
                    "Kitchen",
                    &sha256_hex("other"),
                    &Modules::member(),
                    14
                )
                .is_err()
        );
    }

    #[test]
    fn search_is_by_sha256_and_never_plaintext() {
        let store = store();
        store
            .access_create(
                "phone",
                "Phone",
                &sha256_hex("phone-token"),
                &Modules::member(),
                10,
            )
            .expect("create");
        let snapshot = store.access_snapshot().expect("snapshot");
        assert!(
            snapshot
                .authenticate_token("phone-token")
                .expect("token")
                .allows(Module::Home)
        );
        assert!(snapshot.authenticate_token("wrong").is_none());
        assert!(!format!("{snapshot:?}").contains("phone-token"));
    }
    #[test]
    fn revoking_a_token_removes_its_devices_and_history() {
        let store = store();
        let created = store
            .access_create(
                "phone",
                "Phone",
                &sha256_hex("phone-token"),
                &Modules::member(),
                10,
            )
            .expect("token");
        store
            .register_push_device_for(
                &crate::store::PushDeviceInput {
                    device_id: "device-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
                    token: "token-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
                    platform: "android".to_owned(),
                    app_version: "0.1.0".to_owned(),
                    device_name: "Phone".to_owned(),
                },
                &created.id,
                true,
                11,
            )
            .expect("device");
        store
            .record_notification(
                "device-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                &crate::store::NotificationInput {
                    id: "n-1".to_owned(),
                    kind: "notify".to_owned(),
                    title: "T".to_owned(),
                    body: "B".to_owned(),
                    deep_link: None,
                },
                12,
            )
            .expect("history");
        store.access_revoke(&created.id, 13).expect("revoke");
        assert!(store.list_push_devices().expect("devices").is_empty());
        assert!(
            store
                .list_notifications(
                    "device-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    None,
                    crate::store::DEFAULT_HISTORY_LIMIT,
                )
                .expect("history")
                .entries
                .is_empty()
        );
    }

    #[test]
    fn a_seed_rotates_the_owner_hash_by_id() {
        let store = store();
        let first = AccessSnapshot {
            revision: 1,
            tokens: vec![TokenRecord {
                id: "owner".to_owned(),
                name: "owner".to_owned(),
                sha256: sha256_hex("first"),
                owner: true,
                modules: Modules::owner()
                    .ids()
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            }],
        };
        store.access_import_seed(&first, 10).expect("first");
        let second = AccessSnapshot {
            revision: 2,
            tokens: vec![TokenRecord {
                id: "owner".to_owned(),
                name: "owner".to_owned(),
                sha256: sha256_hex("second"),
                owner: true,
                modules: Modules::owner()
                    .ids()
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            }],
        };
        store.access_import_seed(&second, 11).expect("rotate");
        let snapshot = store.access_snapshot().expect("snapshot");
        assert!(snapshot.authenticate_token("second").is_some());
        assert!(snapshot.authenticate_token("first").is_none());
    }
}
