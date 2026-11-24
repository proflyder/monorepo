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
        .map(|values| Modules::parse_lenient(&values).0)?;
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
    if !owner && modules.contains(Module::Secrets) {
        return Err(StoreError::precondition(
            "the secrets module belongs to the owner token",
        ));
    }
    modules.insert(Module::Base);
    if owner {
        modules.insert(Module::Admin);
        modules.insert(Module::Secrets);
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

fn name_taken(
    connection: &Connection,
    name: &str,
    except_id: Option<&str>,
) -> Result<bool, StoreError> {
    let wanted = name.trim().to_lowercase();
    let mut statement =
        connection.prepare("SELECT id, name FROM app_tokens WHERE revoked_unix = 0")?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (id, existing) = row?;
        if Some(id.as_str()) != except_id && existing.trim().to_lowercase() == wanted {
            return Ok(true);
        }
    }
    Ok(false)
}

fn owner_id(connection: &Connection) -> Result<Option<String>, StoreError> {
    Ok(connection
        .query_row(
            "SELECT id FROM app_tokens WHERE is_owner = 1 AND revoked_unix = 0",
            [],
            |row| row.get(0),
        )
        .optional()?)
}

fn import_seed_record(
    transaction: &Connection,
    record: &TokenRecord,
    now_unix: i64,
) -> Result<bool, StoreError> {
    let (mut modules, unknown) = Modules::parse_lenient(&record.modules);
    if !unknown.is_empty() {
        tracing::warn!(
            id = record.id.as_str(),
            ?unknown,
            "seed token grants unknown modules; ignored"
        );
    }
    if !record.owner {
        modules.remove(Module::Admin);
        modules.remove(Module::Secrets);
    }
    if record.owner && record.id != OWNER_TOKEN_ID {
        tracing::warn!(
            id = record.id.as_str(),
            "seed owner token must use the owner id; skipped"
        );
        return Ok(false);
    }
    if !record.owner && (record.id == OWNER_TOKEN_ID || record.id == LEGACY_TOKEN_ID) {
        tracing::warn!(
            id = record.id.as_str(),
            "seed token uses a reserved id; skipped"
        );
        return Ok(false);
    }
    let name = match clean_name(&record.name) {
        Ok(name) => name,
        Err(error) => {
            tracing::warn!(id = record.id.as_str(), %error, "seed token name is invalid; skipped");
            return Ok(false);
        }
    };
    let modules_json = serde_json::to_string(&clean_modules(&modules, record.owner)?)
        .map_err(|error| StoreError::invalid(error.to_string()))?;
    let stored: Option<(String, String, i64)> = transaction
        .query_row(
            "SELECT sha256, name, revoked_unix FROM app_tokens WHERE id = ?1",
            params![record.id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let name_free = !name_taken(transaction, &name, Some(&record.id))?;
    if !name_free {
        tracing::warn!(
            id = record.id.as_str(),
            name = name.as_str(),
            "seed token name is already used by another token; keeping the stored name"
        );
    }
    match stored {
        Some((_, _, revoked)) if revoked != 0 => {
            tracing::warn!(id = record.id.as_str(), "seed token was revoked; skipped");
            Ok(false)
        }
        Some((digest, stored_name, _)) => {
            if !record.owner {
                return Ok(false);
            }
            let target_name = if name_free { name } else { stored_name.clone() };
            if digest == record.sha256 && target_name == stored_name {
                return Ok(false);
            }
            transaction.execute(
                "UPDATE app_tokens SET sha256 = ?1, modules_json = ?2, name = ?3, is_owner = 1, updated_unix = ?4 WHERE id = ?5",
                params![record.sha256, modules_json, target_name, now_unix, record.id],
            )?;
            Ok(true)
        }
        None => {
            if record.owner && owner_id(transaction)?.is_some() {
                tracing::warn!("another owner token already exists; seed owner skipped");
                return Ok(false);
            }
            if !name_free {
                return Ok(false);
            }
            transaction.execute(
                "INSERT INTO app_tokens (id, name, sha256, modules_json, is_owner, created_unix, updated_unix, revoked_unix)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, 0)",
                params![
                    record.id,
                    name,
                    record.sha256,
                    modules_json,
                    i64::from(record.owner),
                    now_unix
                ],
            )?;
            Ok(true)
        }
    }
}

pub const MAX_APP_TOKENS: i64 = 64;
pub const LEGACY_TOKEN_ID: &str = "mama";
pub const OWNER_TOKEN_ID: &str = "owner";

impl Store {
    pub fn access_snapshot(&self) -> Result<AccessSnapshot, StoreError> {
        self.read(snapshot)
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
                changed |= import_seed_record(transaction, record, now_unix)?;
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
        if modules.contains(Module::Secrets) {
            return Err(StoreError::precondition(
                "the secrets module belongs to the owner token",
            ));
        }
        let modules_json = serde_json::to_string(&clean_modules(modules, false)?)
            .map_err(|error| StoreError::invalid(error.to_string()))?;
        let id = id.to_owned();
        let digest = digest.to_owned();
        self.with_tx(move |transaction| {
            let count: i64 = transaction.query_row(
                "SELECT COUNT(*) FROM app_tokens WHERE revoked_unix = 0",
                [],
                |row| row.get(0),
            )?;
            if count >= MAX_APP_TOKENS {
                return Err(StoreError::limit("too many application tokens"));
            }
            if name_taken(transaction, &name, None)? {
                return Err(StoreError::precondition("token name is already used"));
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

    pub fn access_update(
        &self,
        id: &str,
        name: Option<&str>,
        modules: Option<&Modules>,
        now_unix: i64,
    ) -> Result<AccessToken, StoreError> {
        let id = id.to_owned();
        let name = name.map(clean_name).transpose()?;
        let requested = modules.cloned();
        self.with_tx(move |transaction| {
            let current = token(transaction, &id)?.ok_or(StoreError::NotFound)?;
            if let Some(name) = name.as_deref()
                && name_taken(transaction, name, Some(&id))?
            {
                return Err(StoreError::precondition("token name is already used"));
            }
            let modules_json = match requested.as_ref() {
                Some(requested) => {
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
                    if !current.owner && requested.contains(Module::Secrets) {
                        return Err(StoreError::precondition(
                            "the secrets module belongs to the owner token",
                        ));
                    }
                    Some(
                        serde_json::to_string(&clean_modules(requested, current.owner)?)
                            .map_err(|error| StoreError::invalid(error.to_string()))?,
                    )
                }
                None => None,
            };
            if let Some(name) = name.as_deref() {
                transaction.execute(
                    "UPDATE app_tokens SET name = ?, updated_unix = ? WHERE id = ?",
                    params![name, now_unix, id],
                )?;
            }
            if let Some(modules_json) = modules_json {
                transaction.execute(
                    "UPDATE app_tokens SET modules_json = ?, updated_unix = ? WHERE id = ?",
                    params![modules_json, now_unix, id],
                )?;
            }
            if name.is_some() || requested.is_some() {
                bump(transaction)?;
            }
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
            transaction.execute("DELETE FROM vpn_peers WHERE token_id = ?1", params![id])?;
            transaction.execute(
                "UPDATE app_tokens SET name = name || '#' || id, revoked_unix = ?, updated_unix = ? WHERE id = ?",
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
        assert!(owner.modules.contains(Module::Secrets));
    }

    #[test]
    fn the_secrets_module_cannot_be_granted_to_a_non_owner() {
        let store = store();
        let mut secrets = Modules::member();
        secrets.insert(Module::Secrets);
        let error = store
            .access_create("guest", "Guest", &sha256_hex("guest"), &secrets, 10)
            .expect_err("create with secrets");
        assert_eq!(error.kind(), Some(crate::domain::ErrorKind::Precondition));
        let guest = store
            .access_create(
                "phone",
                "Phone",
                &sha256_hex("phone"),
                &Modules::member(),
                11,
            )
            .expect("plain member");
        let error = store
            .access_update("phone", None, Some(&secrets), 12)
            .expect_err("update with secrets");
        assert_eq!(error.kind(), Some(crate::domain::ErrorKind::Precondition));
        assert!(
            !store
                .access_token(&guest.id)
                .expect("guest")
                .modules
                .contains(Module::Secrets)
        );
        let snapshot = store.access_snapshot().expect("snapshot");
        assert!(
            snapshot
                .authenticate_token("phone")
                .expect("token")
                .require(Module::Secrets)
                .is_err()
        );
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
                .access_update("owner", None, Some(&Modules::member()), 11)
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
        let renamed = store
            .access_update("phone", Some("Kitchen"), None, 11)
            .expect("rename");
        assert_eq!(renamed.name, "Kitchen");
        let updated = store
            .access_update("phone", None, Some(&Modules::member()), 12)
            .expect("modules");
        assert_eq!(updated.modules.ids(), Modules::member().ids());
        let revision = store.access_snapshot().expect("snapshot").revision;
        store.access_revoke("phone", 13).expect("revoke");
        let snapshot = store.access_snapshot().expect("snapshot");
        assert!(snapshot.tokens.is_empty());
        assert!(snapshot.revision > revision);
        store
            .access_create(
                "dup",
                "Kitchen",
                &sha256_hex("other"),
                &Modules::member(),
                14,
            )
            .expect("revoked name can be reused");
    }

    #[test]
    fn names_are_unique_case_insensitively_including_cyrillic() {
        let store = store();
        store
            .access_create("a", "Гость", &sha256_hex("a"), &Modules::member(), 10)
            .expect("first");
        let error = store
            .access_create("b", "гость", &sha256_hex("b"), &Modules::member(), 11)
            .expect_err("duplicate");
        assert_eq!(error.kind(), Some(crate::domain::ErrorKind::Precondition));
        store
            .access_create("c", "Кухня", &sha256_hex("c"), &Modules::member(), 12)
            .expect("other");
        let error = store
            .access_update("c", Some("ГОСТЬ"), None, 13)
            .expect_err("rename to a used name");
        assert_eq!(error.kind(), Some(crate::domain::ErrorKind::Precondition));
    }

    #[test]
    fn a_failed_update_changes_nothing() {
        let store = store();
        store
            .access_create("a", "Phone", &sha256_hex("a"), &Modules::member(), 10)
            .expect("token");
        let revision = store.access_snapshot().expect("snapshot").revision;
        let mut admin = Modules::member();
        admin.insert(Module::Admin);
        assert!(
            store
                .access_update("a", Some("Kitchen"), Some(&admin), 11)
                .is_err()
        );
        assert_eq!(store.access_token("a").expect("token").name, "Phone");
        assert_eq!(
            store.access_snapshot().expect("snapshot").revision,
            revision
        );
    }

    #[test]
    fn the_token_count_is_bounded() {
        let store = store();
        let existing = i64::try_from(store.access_list().expect("list").len()).expect("count");
        for index in existing..MAX_APP_TOKENS {
            store
                .access_create(
                    &format!("t{index}"),
                    &format!("T{index}"),
                    &sha256_hex(&format!("t{index}")),
                    &Modules::member(),
                    10,
                )
                .expect("within the limit");
        }
        let error = store
            .access_create("x", "X", &sha256_hex("x"), &Modules::member(), 11)
            .expect_err("limit");
        assert_eq!(error.kind(), Some(crate::domain::ErrorKind::LimitExceeded));
    }

    fn owner_seed(id: &str, name: &str, token: &str) -> AccessSnapshot {
        AccessSnapshot {
            revision: 0,
            tokens: vec![TokenRecord {
                id: id.to_owned(),
                name: name.to_owned(),
                sha256: sha256_hex(token),
                owner: true,
                modules: vec!["admin".to_owned()],
            }],
        }
    }

    #[test]
    fn a_seed_never_creates_a_second_owner_or_fails_on_a_taken_name() {
        let store = store();
        assert!(
            !store
                .access_import_seed(&owner_seed("guest", "guest", "x"), 10)
                .expect("owner with a foreign id is skipped")
        );
        store
            .access_create("kitchen", "Стас", &sha256_hex("k"), &Modules::member(), 11)
            .expect("token");
        assert!(
            !store
                .access_import_seed(&owner_seed(OWNER_TOKEN_ID, "Стас", "owner-1"), 12)
                .expect("a taken name skips the new owner instead of failing")
        );
        assert!(
            store
                .access_import_seed(&owner_seed(OWNER_TOKEN_ID, "owner", "owner-1"), 13)
                .expect("owner")
        );
        assert!(
            store
                .access_import_seed(&owner_seed(OWNER_TOKEN_ID, "Стас", "owner-2"), 14)
                .expect("rotation with a taken name keeps the stored name")
        );
        let owner = store.access_token(OWNER_TOKEN_ID).expect("owner");
        assert_eq!(owner.name, "owner");
        let snapshot = store.access_snapshot().expect("snapshot");
        assert!(snapshot.authenticate_token("owner-2").is_some());
        assert_eq!(
            snapshot.tokens.iter().filter(|token| token.owner).count(),
            1
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
    #[test]
    fn a_non_owner_seed_cannot_gain_admin() {
        let store = store();
        let snapshot = AccessSnapshot {
            revision: 1,
            tokens: vec![TokenRecord {
                id: "guest".to_owned(),
                name: "guest".to_owned(),
                sha256: sha256_hex("guest"),
                owner: false,
                modules: vec!["admin".to_owned(), "home".to_owned()],
            }],
        };
        store.access_import_seed(&snapshot, 10).expect("seed");
        let guest = store.access_token("guest").expect("guest");
        assert!(!guest.modules.contains(Module::Admin));
        assert!(guest.modules.contains(Module::Home));
    }
}
