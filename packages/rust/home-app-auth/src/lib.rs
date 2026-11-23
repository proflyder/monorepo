use std::collections::BTreeSet;
use std::fmt;
use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

use http::HeaderValue;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use thiserror::Error;

const BEARER_PREFIX: &str = "bearer ";
pub const LEGACY_NAME: &str = "mama";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Module {
    Base,
    Home,
    Scenarios,
    AlarmsTimers,
    IntercomCall,
    Notifications,
    Chat,
    Vpn,
    System,
    InfraReports,
    Alerts,
    Admin,
}

pub const MODULES: &[Module] = &[
    Module::Base,
    Module::Home,
    Module::Scenarios,
    Module::AlarmsTimers,
    Module::IntercomCall,
    Module::Notifications,
    Module::Chat,
    Module::Vpn,
    Module::System,
    Module::InfraReports,
    Module::Alerts,
    Module::Admin,
];

pub const MEMBER_MODULES: &[Module] = &[
    Module::Home,
    Module::Scenarios,
    Module::AlarmsTimers,
    Module::IntercomCall,
    Module::Notifications,
];

impl Module {
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Base => "base",
            Self::Home => "home",
            Self::Scenarios => "scenarios",
            Self::AlarmsTimers => "alarms_timers",
            Self::IntercomCall => "intercom_call",
            Self::Notifications => "notifications",
            Self::Chat => "chat",
            Self::Vpn => "vpn",
            Self::System => "system",
            Self::InfraReports => "infra_reports",
            Self::Alerts => "alerts",
            Self::Admin => "admin",
        }
    }

    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::Base => "Базовый доступ",
            Self::Home => "Дом",
            Self::Scenarios => "Сценарии",
            Self::AlarmsTimers => "Будильники и таймеры",
            Self::IntercomCall => "Звонок на колонку",
            Self::Notifications => "Уведомления",
            Self::Chat => "Чат",
            Self::Vpn => "VPN",
            Self::System => "Система",
            Self::InfraReports => "Отчёты инфраструктуры",
            Self::Alerts => "Алерты",
            Self::Admin => "Администрирование",
        }
    }

    #[must_use]
    pub fn from_id(value: &str) -> Option<Self> {
        MODULES.iter().copied().find(|module| module.id() == value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Modules {
    granted: BTreeSet<Module>,
}

impl Modules {
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn member() -> Self {
        MEMBER_MODULES.iter().copied().collect()
    }

    #[must_use]
    pub fn owner() -> Self {
        MODULES.iter().copied().collect()
    }

    pub fn parse(values: &[String]) -> Result<Self, Error> {
        let mut modules = BTreeSet::new();
        for value in values {
            let module =
                Module::from_id(value).ok_or_else(|| Error::UnknownModule(value.clone()))?;
            modules.insert(module);
        }
        Ok(Self { granted: modules })
    }

    pub fn insert(&mut self, module: Module) {
        self.granted.insert(module);
    }

    pub fn remove(&mut self, module: Module) {
        self.granted.remove(&module);
    }

    #[must_use]
    pub fn contains(&self, module: Module) -> bool {
        module == Module::Base || self.granted.contains(&module)
    }

    #[must_use]
    pub fn with_base(&self) -> BTreeSet<Module> {
        let mut modules = self.granted.clone();
        modules.insert(Module::Base);
        modules
    }

    #[must_use]
    pub fn ids(&self) -> Vec<&'static str> {
        self.with_base().iter().map(|module| module.id()).collect()
    }

    #[must_use]
    pub fn is_owner_set(&self) -> bool {
        self.granted.contains(&Module::Admin)
    }
}

impl FromIterator<Module> for Modules {
    fn from_iter<T: IntoIterator<Item = Module>>(iter: T) -> Self {
        Self {
            granted: iter.into_iter().collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenRecord {
    pub id: String,
    pub name: String,
    pub sha256: String,
    #[serde(default)]
    pub owner: bool,
    pub modules: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessSnapshot {
    #[serde(default)]
    pub revision: i64,
    #[serde(default)]
    pub tokens: Vec<TokenRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Principal {
    pub id: String,
    pub name: String,
    pub owner: bool,
    modules: Modules,
}

impl Principal {
    #[must_use]
    pub fn allows(&self, module: Module) -> bool {
        self.owner || self.modules.contains(module)
    }

    #[must_use]
    pub fn modules(&self) -> &Modules {
        &self.modules
    }

    #[must_use]
    pub fn module_ids(&self) -> Vec<&'static str> {
        if self.owner {
            Modules::owner().ids()
        } else {
            self.modules.ids()
        }
    }
}

impl AccessSnapshot {
    pub fn parse(json: &[u8]) -> Result<Self, Error> {
        let snapshot: Self =
            serde_json::from_slice(json).map_err(|error| Error::Json(error.to_string()))?;
        for record in &snapshot.tokens {
            Modules::parse(&record.modules)?;
        }
        Ok(snapshot)
    }

    pub fn authenticate(&self, header: Option<&HeaderValue>) -> Option<Principal> {
        let value = header?.to_str().ok()?;
        self.authenticate_bearer(value)
    }

    pub fn authenticate_bearer(&self, value: &str) -> Option<Principal> {
        let candidate = bearer(value)?;
        self.authenticate_token(candidate)
    }

    pub fn authenticate_token(&self, token: &str) -> Option<Principal> {
        if token.is_empty() {
            return None;
        }
        let digest = sha256_hex(token);
        let mut found = None;
        for record in &self.tokens {
            let equal: bool = digest.as_bytes().ct_eq(record.sha256.as_bytes()).into();
            if equal && found.is_none() {
                let modules = Modules::parse(&record.modules).ok()?;
                found = Some(Principal {
                    id: record.id.clone(),
                    name: record.name.clone(),
                    owner: record.owner,
                    modules,
                });
            }
        }
        found
    }

    pub fn find(&self, id: &str) -> Option<&TokenRecord> {
        self.tokens.iter().find(|record| record.id == id)
    }
}

#[derive(Clone, Default)]
pub struct AccessCache {
    snapshot: Arc<RwLock<AccessSnapshot>>,
}

impl fmt::Debug for AccessCache {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AccessCache")
            .finish_non_exhaustive()
    }
}

impl AccessCache {
    #[must_use]
    pub fn new(snapshot: AccessSnapshot) -> Self {
        Self {
            snapshot: Arc::new(RwLock::new(snapshot)),
        }
    }

    pub fn replace(&self, snapshot: AccessSnapshot) {
        *self.write() = snapshot;
    }

    #[must_use]
    pub fn revision(&self) -> i64 {
        self.read().revision
    }

    pub fn authenticate(&self, header: Option<&HeaderValue>) -> Option<Principal> {
        self.read().authenticate(header)
    }

    pub fn authenticate_token(&self, token: &str) -> Option<Principal> {
        self.read().authenticate_token(token)
    }

    fn read(&self) -> RwLockReadGuard<'_, AccessSnapshot> {
        self.snapshot.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn write(&self) -> RwLockWriteGuard<'_, AccessSnapshot> {
        self.snapshot
            .write()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    #[error("access snapshot: {0}")]
    Json(String),
    #[error("unknown module: {0}")]
    UnknownModule(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Forbidden {
    Unauthenticated,
    ServiceOnly,
    Module(Module),
}

impl Forbidden {
    #[must_use]
    pub fn body(self) -> String {
        match self {
            Self::Unauthenticated => r#"{"error":"unauthenticated"}"#.to_owned(),
            Self::ServiceOnly => r#"{"error":"service_only"}"#.to_owned(),
            Self::Module(module) => {
                format!(
                    r#"{{"error":"module_forbidden","module":"{}"}}"#,
                    module.id()
                )
            }
        }
    }
}

impl Principal {
    pub fn require(&self, module: Module) -> Result<(), Forbidden> {
        if self.allows(module) {
            Ok(())
        } else {
            Err(Forbidden::Module(module))
        }
    }
}

#[must_use]
pub fn bearer(value: &str) -> Option<&str> {
    let value = value.trim();
    let prefix = value.get(..BEARER_PREFIX.len())?;
    if !prefix.eq_ignore_ascii_case(BEARER_PREFIX) {
        return None;
    }
    let candidate = value[BEARER_PREFIX.len()..].trim();
    (!candidate.is_empty()).then_some(candidate)
}

#[must_use]
pub fn sha256_hex(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(value: &str) -> HeaderValue {
        HeaderValue::from_str(value).expect("header")
    }

    fn snapshot() -> AccessSnapshot {
        AccessSnapshot {
            revision: 3,
            tokens: vec![
                TokenRecord {
                    id: "owner-id".to_owned(),
                    name: "owner".to_owned(),
                    sha256: sha256_hex("owner-token"),
                    owner: true,
                    modules: Modules::owner()
                        .ids()
                        .iter()
                        .map(|id| (*id).to_owned())
                        .collect(),
                },
                TokenRecord {
                    id: "mama-id".to_owned(),
                    name: "mama".to_owned(),
                    sha256: sha256_hex("mama-token"),
                    owner: false,
                    modules: Modules::member()
                        .ids()
                        .iter()
                        .map(|id| (*id).to_owned())
                        .collect(),
                },
            ],
        }
    }

    #[test]
    fn base_is_implicit_and_modules_are_matched_by_hash() {
        let snapshot = snapshot();
        let owner = snapshot
            .authenticate(Some(&header("Bearer owner-token")))
            .expect("owner");
        assert!(owner.allows(Module::Admin));
        assert!(owner.allows(Module::Base));
        let mama = snapshot
            .authenticate(Some(&header("bearer mama-token")))
            .expect("mama");
        assert!(mama.allows(Module::Home));
        assert!(mama.allows(Module::Base));
        assert!(!mama.allows(Module::Chat));
        assert_eq!(
            mama.require(Module::Chat),
            Err(Forbidden::Module(Module::Chat))
        );
        assert_eq!(
            Forbidden::Module(Module::Chat).body(),
            r#"{"error":"module_forbidden","module":"chat"}"#
        );
    }

    #[test]
    fn unknown_and_malformed_snapshots_are_rejected() {
        assert_eq!(
            AccessSnapshot::parse(
                br#"{"revision":1,"tokens":[{"id":"a","name":"a","sha256":"x","modules":["nope"]}]}"#
            ),
            Err(Error::UnknownModule("nope".to_owned()))
        );
        assert!(AccessSnapshot::parse(b"{}").is_ok());
        assert!(AccessSnapshot::parse(b"not-json").is_err());
        assert_eq!(
            Modules::parse(&["nope".to_owned()]),
            Err(Error::UnknownModule("nope".to_owned()))
        );
    }

    #[test]
    fn owner_module_set_contains_every_module() {
        let modules = Modules::owner();
        for module in MODULES {
            assert!(modules.contains(*module));
        }
        assert!(Modules::member().contains(Module::Home));
        assert!(!Modules::member().contains(Module::Admin));
    }
}
