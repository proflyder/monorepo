use std::collections::BTreeSet;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::{Duration, Instant};

use http::HeaderValue;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use thiserror::Error;

const BEARER_PREFIX: &str = "bearer ";

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

    #[must_use]
    pub fn all_but_admin() -> Self {
        MODULES
            .iter()
            .copied()
            .filter(|module| *module != Module::Admin)
            .collect()
    }

    pub fn parse(values: &[String]) -> Result<Self, Error> {
        let (modules, unknown) = Self::parse_lenient(values);
        unknown
            .into_iter()
            .next()
            .map_or(Ok(modules), |value| Err(Error::UnknownModule(value)))
    }

    #[must_use]
    pub fn parse_lenient(values: &[String]) -> (Self, Vec<String>) {
        let mut granted = BTreeSet::new();
        let mut unknown = Vec::new();
        for value in values {
            match Module::from_id(value) {
                Some(module) => {
                    granted.insert(module);
                }
                None => unknown.push(value.clone()),
            }
        }
        (Self { granted }, unknown)
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
    pub fn is_empty(&self) -> bool {
        self.granted.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.granted.len()
    }

    #[must_use]
    pub fn ids(&self) -> Vec<&'static str> {
        self.with_base().iter().map(|module| module.id()).collect()
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
            let (_, unknown) = Modules::parse_lenient(&record.modules);
            if !unknown.is_empty() {
                tracing::warn!(
                    token = record.id.as_str(),
                    ?unknown,
                    "access snapshot grants unknown modules; they are ignored"
                );
            }
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
                found = Some(Principal {
                    id: record.id.clone(),
                    name: record.name.clone(),
                    owner: record.owner,
                    modules: Modules::parse_lenient(&record.modules).0,
                });
            }
        }
        found
    }

    #[must_use]
    pub fn token_ids_with(&self, module: Module) -> Vec<String> {
        self.tokens
            .iter()
            .filter(|token| {
                token.owner || Modules::parse_lenient(&token.modules).0.contains(module)
            })
            .map(|token| token.id.clone())
            .collect()
    }
}

pub const REFRESH_INTERVAL: Duration = Duration::from_secs(30);
pub const MAX_SNAPSHOT_AGE: Duration = Duration::from_secs(3600);
const ON_DEMAND_REFRESH_INTERVAL: Duration = Duration::from_secs(1);

struct CacheState {
    snapshot: AccessSnapshot,
    fetched_at: Option<Instant>,
}

#[derive(Clone)]
pub struct AccessCache {
    state: Arc<RwLock<CacheState>>,
    max_age: Option<Duration>,
}

impl Default for AccessCache {
    fn default() -> Self {
        Self::remote(MAX_SNAPSHOT_AGE)
    }
}

impl fmt::Debug for AccessCache {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AccessCache")
            .field("max_age", &self.max_age)
            .finish_non_exhaustive()
    }
}

impl AccessCache {
    #[must_use]
    pub fn new(snapshot: AccessSnapshot) -> Self {
        Self {
            state: Arc::new(RwLock::new(CacheState {
                snapshot,
                fetched_at: Some(Instant::now()),
            })),
            max_age: None,
        }
    }

    #[must_use]
    pub fn remote(max_age: Duration) -> Self {
        Self {
            state: Arc::new(RwLock::new(CacheState {
                snapshot: AccessSnapshot::default(),
                fetched_at: None,
            })),
            max_age: Some(max_age),
        }
    }

    pub fn replace(&self, snapshot: AccessSnapshot) {
        let mut state = self.write();
        state.snapshot = snapshot;
        state.fetched_at = Some(Instant::now());
    }

    #[must_use]
    pub fn revision(&self) -> i64 {
        self.read().snapshot.revision
    }

    #[must_use]
    pub fn is_fresh(&self) -> bool {
        let state = self.read();
        match (self.max_age, state.fetched_at) {
            (_, None) => false,
            (None, Some(_)) => true,
            (Some(max_age), Some(at)) => at.elapsed() <= max_age,
        }
    }

    pub fn authenticate(&self, header: Option<&HeaderValue>) -> Option<Principal> {
        if !self.is_fresh() {
            return None;
        }
        self.read().snapshot.authenticate(header)
    }

    pub fn authenticate_token(&self, token: &str) -> Option<Principal> {
        if !self.is_fresh() {
            return None;
        }
        self.read().snapshot.authenticate_token(token)
    }

    #[must_use]
    pub fn token_ids_with(&self, module: Module) -> Vec<String> {
        if !self.is_fresh() {
            return Vec::new();
        }
        self.read().snapshot.token_ids_with(module)
    }

    fn read(&self) -> RwLockReadGuard<'_, CacheState> {
        self.state.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn write(&self) -> RwLockWriteGuard<'_, CacheState> {
        self.state.write().unwrap_or_else(PoisonError::into_inner)
    }
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum Error {
    #[error("access snapshot: {0}")]
    Json(String),
    #[error("unknown module: {0}")]
    UnknownModule(String),
    #[error("access client: {0}")]
    Client(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Forbidden {
    Unauthenticated,
    ServiceOnly,
    Module(Module),
}

impl Forbidden {
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Unauthenticated => "unauthenticated",
            Self::ServiceOnly => "service_only",
            Self::Module(_) => "module_forbidden",
        }
    }

    #[must_use]
    pub fn body(self) -> String {
        match self {
            Self::Module(module) => format!(
                r#"{{"error":"{}","module":"{}"}}"#,
                self.code(),
                module.id()
            ),
            _ => format!(r#"{{"error":"{}"}}"#, self.code()),
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

const ACCESS_BODY_LIMIT: usize = 1 << 20;
const ACCESS_TIMEOUT: Duration = Duration::from_secs(5);

type FetchFuture<'a> = Pin<Box<dyn Future<Output = Result<AccessSnapshot, Error>> + Send + 'a>>;

trait Fetch: Send + Sync {
    fn fetch(&self) -> FetchFuture<'_>;
}

struct HttpFetch {
    client: reqwest::Client,
    url: String,
    token: String,
}

impl Fetch for HttpFetch {
    fn fetch(&self) -> FetchFuture<'_> {
        Box::pin(async move {
            let response = self
                .client
                .get(&self.url)
                .bearer_auth(&self.token)
                .send()
                .await
                .map_err(|_| Error::Client("access request failed".into()))?;
            if !response.status().is_success() {
                return Err(Error::Client(format!(
                    "access returned HTTP {}",
                    response.status().as_u16()
                )));
            }
            if response
                .content_length()
                .is_some_and(|length| length > ACCESS_BODY_LIMIT as u64)
            {
                return Err(Error::Client("access response is too large".into()));
            }
            let body = response
                .bytes()
                .await
                .map_err(|_| Error::Client("access body".into()))?;
            if body.len() > ACCESS_BODY_LIMIT {
                return Err(Error::Client("access response is too large".into()));
            }
            AccessSnapshot::parse(&body)
        })
    }
}

struct NoFetch;

impl Fetch for NoFetch {
    fn fetch(&self) -> FetchFuture<'_> {
        Box::pin(async { Err(Error::Client("access source is fixed".into())) })
    }
}

struct Flight {
    generation: u64,
    last_on_demand: Option<Instant>,
}

pub struct AccessClient {
    cache: AccessCache,
    source: Box<dyn Fetch>,
    url: String,
    generation: AtomicU64,
    flight: tokio::sync::Mutex<Flight>,
}

impl fmt::Debug for AccessClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AccessClient")
            .field("url", &self.url)
            .field("revision", &self.cache.revision())
            .finish_non_exhaustive()
    }
}

impl AccessClient {
    pub fn new(url: impl Into<String>, token: impl Into<String>) -> Result<Arc<Self>, Error> {
        let url = url.into();
        let token = token.into();
        if url.trim().is_empty() || token.trim().is_empty() {
            return Err(Error::Client(
                "access URL and service token are required".into(),
            ));
        }
        let _already_installed = rustls::crypto::ring::default_provider().install_default();
        let client = reqwest::Client::builder()
            .timeout(ACCESS_TIMEOUT)
            .build()
            .map_err(|error| Error::Client(error.to_string()))?;
        Ok(Arc::new(Self::with_source(
            url.clone(),
            Box::new(HttpFetch { client, url, token }),
        )))
    }

    #[must_use]
    pub fn fixed(snapshot: AccessSnapshot) -> Arc<Self> {
        let client = Self::with_source(String::new(), Box::new(NoFetch));
        client.cache.replace(snapshot);
        Arc::new(client)
    }

    fn with_source(url: String, source: Box<dyn Fetch>) -> Self {
        Self {
            cache: AccessCache::default(),
            source,
            url,
            generation: AtomicU64::new(0),
            flight: tokio::sync::Mutex::new(Flight {
                generation: 0,
                last_on_demand: None,
            }),
        }
    }

    #[must_use]
    pub fn cache(&self) -> &AccessCache {
        &self.cache
    }

    pub async fn refresh(&self) -> Result<(), Error> {
        let seen = self.generation.load(Ordering::Acquire);
        let mut flight = self.flight.lock().await;
        if flight.generation != seen {
            return Ok(());
        }
        let result = self.refresh_locked(&mut flight).await;
        drop(flight);
        result
    }

    async fn refresh_locked(&self, flight: &mut Flight) -> Result<(), Error> {
        let result = self.source.fetch().await;
        flight.generation = flight.generation.wrapping_add(1);
        self.generation.store(flight.generation, Ordering::Release);
        let snapshot = result?;
        self.cache.replace(snapshot);
        Ok(())
    }

    pub async fn authenticate(&self, header: Option<&HeaderValue>) -> Option<Principal> {
        let header = header?;
        if let Some(principal) = self.cache.authenticate(Some(header)) {
            return Some(principal);
        }
        let seen = self.generation.load(Ordering::Acquire);
        let mut flight = match self.flight.try_lock() {
            Ok(flight) => flight,
            Err(_) if self.cache.is_fresh() => self.flight.lock().await,
            Err(_) => return None,
        };
        if flight.generation == seen {
            let now = Instant::now();
            let allowed = flight
                .last_on_demand
                .is_none_or(|previous| now.duration_since(previous) >= ON_DEMAND_REFRESH_INTERVAL);
            if allowed {
                flight.last_on_demand = Some(now);
                if let Err(error) = self.refresh_locked(&mut flight).await {
                    tracing::warn!(%error, "access refresh failed");
                }
            }
        }
        drop(flight);
        self.cache.authenticate(Some(header))
    }

    pub async fn run(self: Arc<Self>, cancel: tokio_util::sync::CancellationToken) {
        let mut tick = tokio::time::interval(REFRESH_INTERVAL);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                () = cancel.cancelled() => break,
                _ = tick.tick() => {
                    if let Err(error) = self.refresh().await {
                        tracing::warn!(%error, "access refresh failed");
                    }
                }
            }
        }
    }
}

#[must_use]
pub fn snapshot_url(scheduler_url: &str) -> String {
    let base = scheduler_url.trim().trim_end_matches('/');
    let base = base
        .strip_suffix(SCHEDULER_SERVICE_PATH)
        .unwrap_or(base)
        .trim_end_matches('/');
    format!("{base}/v1/access/snapshot")
}

const SCHEDULER_SERVICE_PATH: &str = "/home.v1.SchedulerService";

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
    fn snapshots_ignore_unknown_modules_but_the_admin_parser_rejects_them() {
        let parsed = AccessSnapshot::parse(
            br#"{"revision":1,"tokens":[{"id":"a","name":"a","sha256":"x","modules":["nope","home"]}]}"#,
        )
        .expect("unknown modules are ignored in snapshots");
        let (modules, unknown) = Modules::parse_lenient(&parsed.tokens[0].modules);
        assert!(modules.contains(Module::Home));
        assert_eq!(unknown, vec!["nope".to_owned()]);
        assert!(AccessSnapshot::parse(b"{}").is_ok());
        assert!(AccessSnapshot::parse(b"not-json").is_err());
        assert_eq!(
            Modules::parse(&["home".to_owned(), "nope".to_owned()]),
            Err(Error::UnknownModule("nope".to_owned()))
        );
    }

    struct CountingFetch {
        calls: Arc<std::sync::atomic::AtomicUsize>,
        result: Result<AccessSnapshot, Error>,
        delay: Duration,
    }

    impl Fetch for CountingFetch {
        fn fetch(&self) -> FetchFuture<'_> {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(self.delay).await;
                self.result.clone()
            })
        }
    }

    fn counting(
        result: Result<AccessSnapshot, Error>,
        delay: Duration,
    ) -> (Arc<AccessClient>, Arc<std::sync::atomic::AtomicUsize>) {
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let client = AccessClient::with_source(
            "test".to_owned(),
            Box::new(CountingFetch {
                calls: Arc::clone(&calls),
                result,
                delay,
            }),
        );
        (Arc::new(client), calls)
    }

    #[tokio::test]
    async fn a_missing_header_never_refreshes() {
        let (client, calls) = counting(Ok(snapshot()), Duration::ZERO);
        assert!(client.authenticate(None).await.is_none());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn concurrent_requests_with_a_new_token_share_one_refresh() {
        let (client, calls) = counting(Ok(snapshot()), Duration::from_millis(50));
        let mut before = snapshot();
        before.tokens.retain(|token| token.owner);
        client.cache.replace(before);
        let mama = header("Bearer mama-token");
        let tasks: Vec<_> = (0..8)
            .map(|_| {
                let client = Arc::clone(&client);
                let mama = mama.clone();
                tokio::spawn(async move { client.authenticate(Some(&mama)).await })
            })
            .collect();
        for task in tasks {
            let principal = task
                .await
                .expect("join")
                .expect("every waiter is authenticated");
            assert_eq!(principal.name, "mama");
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn unknown_tokens_are_rate_limited_while_the_cache_is_fresh() {
        let (client, calls) = counting(Ok(snapshot()), Duration::ZERO);
        client.refresh().await.expect("refresh");
        let wrong = header("Bearer wrong");
        assert!(client.authenticate(Some(&wrong)).await.is_none());
        assert!(client.authenticate(Some(&wrong)).await.is_none());
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_stale_remote_cache_fails_closed() {
        let cache = AccessCache::remote(Duration::ZERO);
        assert!(!cache.is_fresh());
        cache.replace(snapshot());
        std::thread::sleep(Duration::from_millis(5));
        assert!(!cache.is_fresh());
        assert!(cache.authenticate_token("owner-token").is_none());
        assert!(cache.token_ids_with(Module::Alerts).is_empty());
        let fresh = AccessCache::remote(MAX_SNAPSHOT_AGE);
        fresh.replace(snapshot());
        assert!(fresh.authenticate_token("owner-token").is_some());
    }

    #[tokio::test]
    async fn a_failed_refresh_keeps_the_last_snapshot_until_it_is_too_old() {
        let (client, _) = counting(Err(Error::Client("down".into())), Duration::ZERO);
        client.cache.replace(snapshot());
        assert!(client.refresh().await.is_err());
        assert!(
            client
                .authenticate(Some(&header("Bearer mama-token")))
                .await
                .is_some()
        );
    }

    #[test]
    fn the_snapshot_survives_an_hour_long_scheduler_outage() {
        assert_eq!(MAX_SNAPSHOT_AGE, Duration::from_secs(3600));
        assert!(MAX_SNAPSHOT_AGE >= REFRESH_INTERVAL * 100);
    }

    #[tokio::test]
    async fn a_stale_cache_does_not_queue_requests_behind_a_slow_refresh() {
        let (client, calls) = counting(
            Err(Error::Client("down".into())),
            Duration::from_millis(300),
        );
        let wrong = header("Bearer mama-token");
        let started = Instant::now();
        let tasks: Vec<_> = (0..8)
            .map(|_| {
                let client = Arc::clone(&client);
                let wrong = wrong.clone();
                tokio::spawn(async move { client.authenticate(Some(&wrong)).await })
            })
            .collect();
        for task in tasks {
            assert!(task.await.expect("join").is_none());
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(started.elapsed() < Duration::from_millis(600));
        let again = Instant::now();
        assert!(client.authenticate(Some(&wrong)).await.is_none());
        assert!(
            again.elapsed() < Duration::from_millis(100),
            "rate limit holds while stale"
        );
    }

    #[tokio::test]
    async fn a_fixed_client_never_touches_the_network() {
        let client = AccessClient::fixed(snapshot());
        assert!(
            client
                .authenticate(Some(&header("Bearer wrong")))
                .await
                .is_none()
        );
        assert!(
            client
                .authenticate(Some(&header("Bearer owner-token")))
                .await
                .is_some()
        );
    }

    #[test]
    fn the_snapshot_url_ignores_the_rpc_service_suffix() {
        for base in [
            "http://scheduler:8098",
            "http://scheduler:8098/",
            "http://scheduler:8098/home.v1.SchedulerService",
            "http://scheduler:8098/home.v1.SchedulerService/",
        ] {
            assert_eq!(
                snapshot_url(base),
                "http://scheduler:8098/v1/access/snapshot"
            );
        }
    }

    #[test]
    fn module_queries_return_only_tokens_with_the_module() {
        let cache = AccessCache::new(snapshot());
        let alerts = cache.token_ids_with(Module::Alerts);
        assert!(alerts.contains(&"owner-id".to_owned()));
        assert!(!alerts.contains(&"mama-id".to_owned()));
        assert_eq!(alerts.len(), 1);
    }

    #[test]
    fn the_cache_follows_the_source_even_when_the_revision_goes_back() {
        let cache = AccessCache::new(snapshot());
        let mut older = snapshot();
        older.revision = 2;
        older.tokens.pop();
        cache.replace(older);
        assert_eq!(cache.revision(), 2);
        assert!(cache.authenticate_token("mama-token").is_none());
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
