//! Home Assistant access: profile reads, entity states and announcements.
//!
//! Secrets are resolved through a [`TokenProvider`] on every call, so a rotated
//! mounted token is picked up without a restart.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use home_ha::Client;
use serde_json::{Map, Value, json};

use super::TokenProvider;
use crate::domain::clock::Clock;
use crate::domain::profile::{Profile, map_state};
use crate::domain::sanitize::ends_with_question;

/// A Home Assistant REST endpoint plus its rotatable bearer.
pub struct HaAccess {
    base_url: String,
    timeout: Duration,
    token: TokenProvider,
    cache: Mutex<Option<(String, Client)>>,
}

impl std::fmt::Debug for HaAccess {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HaAccess")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

impl HaAccess {
    #[must_use]
    pub fn new(base_url: &str, timeout: Duration, token: TokenProvider) -> Self {
        Self {
            base_url: base_url.trim().trim_end_matches('/').to_owned(),
            timeout,
            token,
            cache: Mutex::new(None),
        }
    }

    /// Builds or reuses the client for the current token value.
    fn client(&self) -> Option<Client> {
        if self.base_url.is_empty() {
            return None;
        }
        let token = (self.token)();
        let cached_client = {
            let cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
            cache
                .as_ref()
                .filter(|(cached, _)| *cached == token)
                .map(|(_, client)| client.clone())
        };
        if let Some(client) = cached_client {
            return Some(client);
        }
        let client = Client::new(
            self.base_url.clone(),
            home_secret::Token::new(token.clone()),
            self.timeout,
        )
        .ok()?;
        let mut cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        *cache = Some((token, client.clone()));
        drop(cache);
        Some(client)
    }

    /// Reads one entity state; anything unreadable is `None`.
    pub async fn state(&self, entity: &str) -> Option<String> {
        if entity.is_empty() {
            return None;
        }
        self.client()?.state(entity).await.ok()
    }

    /// Reads the routing profile without any cache.
    pub async fn profile(&self, entity: &str) -> Option<Profile> {
        map_state(&self.state(entity).await?)
    }

    /// Calls one Home Assistant service with the satellite entity as
    /// `entity_id`.
    pub async fn call_service(
        &self,
        domain: &str,
        service: &str,
        data: Map<String, Value>,
    ) -> bool {
        let Some(client) = self.client() else {
            return false;
        };
        client.call_service(domain, service, data).await.is_ok()
    }

    /// Runs one script with variables.
    pub async fn call_script(&self, script: &str, variables: Map<String, Value>) -> bool {
        let Some(client) = self.client() else {
            return false;
        };
        client.call_script(script, variables).await.is_ok()
    }
}

/// Reads the profile entity, caching only a fresh *successful* read. A failed
/// read is unknown and clears the cache, so a stale profile is never used.
pub struct ProfileReader {
    access: Arc<HaAccess>,
    entity: String,
    cache: Duration,
    clock: Arc<dyn Clock>,
    cached: Mutex<Option<(Profile, time::OffsetDateTime)>>,
}

impl std::fmt::Debug for ProfileReader {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProfileReader")
            .field("entity", &self.entity)
            .field("cache", &self.cache)
            .finish_non_exhaustive()
    }
}

impl ProfileReader {
    #[must_use]
    pub fn new(
        access: Arc<HaAccess>,
        entity: impl Into<String>,
        cache: Duration,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            access,
            entity: entity.into(),
            cache,
            clock,
            cached: Mutex::new(None),
        }
    }

    /// The cached profile when it is still fresh.
    pub async fn profile(&self) -> Option<Profile> {
        {
            let cached = self.cached.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some((profile, expires_at)) = cached.as_ref()
                && *expires_at > self.clock.now()
            {
                return Some(*profile);
            }
        }
        // A stale cache is never used: a read error makes the profile unknown.
        {
            let mut cached = self.cached.lock().unwrap_or_else(PoisonError::into_inner);
            *cached = None;
        }
        let profile = self.access.profile(&self.entity).await?;
        let mut cached = self.cached.lock().unwrap_or_else(PoisonError::into_inner);
        *cached = Some((profile, self.clock.now() + self.cache));
        drop(cached);
        Some(profile)
    }

    /// Reads the profile fresh, without touching the cache.
    pub async fn fetch(&self) -> Option<Profile> {
        self.access.profile(&self.entity).await
    }

    /// Reads one entity state fresh.
    pub async fn state(&self, entity: &str) -> Option<String> {
        self.access.state(entity).await
    }
}

/// Speaks late answers through Home Assistant.
///
/// * a statement runs the announcement script and leaves the satellite alone;
/// * a question uses `assist_satellite.start_conversation`, which speaks and
///   then keeps listening;
/// * when that call fails the script is still tried, so the answer is never
///   lost silently.
#[derive(Debug, Clone)]
pub struct HaAnnouncer {
    access: Arc<HaAccess>,
    script: String,
    satellite: String,
}

impl HaAnnouncer {
    #[must_use]
    pub fn new(
        access: Arc<HaAccess>,
        script: impl Into<String>,
        satellite: impl Into<String>,
    ) -> Self {
        Self {
            access,
            script: script.into(),
            satellite: satellite.into(),
        }
    }

    /// Speaks `text` and reports `(announced, listening)`.
    pub async fn announce(&self, text: &str) -> (bool, bool) {
        if text.is_empty() {
            return (false, false);
        }
        if ends_with_question(text) && self.start_conversation(text).await {
            return (true, true);
        }
        (self.via_script(text).await, false)
    }

    /// Runs the announcement script with `{message, chime:false}`.
    pub async fn via_script(&self, text: &str) -> bool {
        if self.script.is_empty() {
            return false;
        }
        let mut variables = Map::new();
        variables.insert("message".to_owned(), Value::String(text.to_owned()));
        variables.insert("chime".to_owned(), Value::Bool(false));
        self.access.call_script(&self.script, variables).await
    }

    /// Speaks a question and leaves the satellite microphone open.
    pub async fn start_conversation(&self, text: &str) -> bool {
        if self.satellite.is_empty() {
            return false;
        }
        let data = json!({
            "entity_id": self.satellite,
            "start_message": text,
            "preannounce": false,
        });
        let Some(object) = data.as_object().cloned() else {
            return false;
        };
        self.access
            .call_service("assist_satellite", "start_conversation", object)
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use axum::extract::State;
    use axum::http::StatusCode;
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use serde_json::{Value, json};

    use super::{HaAccess, HaAnnouncer, ProfileReader};
    use crate::domain::clock::SystemClock;
    use crate::domain::profile::Profile;

    #[derive(Clone, Default)]
    struct Calls(Arc<std::sync::Mutex<Vec<(String, Value)>>>);

    async fn serve(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        format!("http://{address}")
    }

    fn access(base: &str) -> Arc<HaAccess> {
        Arc::new(HaAccess::new(
            base,
            Duration::from_secs(2),
            Arc::new(|| "token".to_owned()),
        ))
    }

    async fn state() -> Json<Value> {
        Json(json!({"entity_id": "input_select.voice", "state": "Рабочий"}))
    }

    async fn service(
        State(calls): State<Calls>,
        uri: axum::http::Uri,
        Json(body): Json<Value>,
    ) -> StatusCode {
        calls
            .0
            .lock()
            .expect("lock")
            .push((uri.path().to_owned(), body));
        StatusCode::OK
    }

    #[tokio::test]
    async fn profile_reads_and_maps_the_entity_state() {
        let base = serve(Router::new().route("/api/states/input_select.voice", get(state))).await;
        let reader = ProfileReader::new(
            access(&base),
            "input_select.voice",
            Duration::from_secs(1),
            Arc::new(SystemClock),
        );
        assert_eq!(reader.profile().await, Some(Profile::Work));
        assert_eq!(reader.fetch().await, Some(Profile::Work));
    }

    #[tokio::test]
    async fn an_unreadable_profile_is_unknown_not_stale() {
        let base = serve(Router::new()).await;
        let reader = ProfileReader::new(
            access(&base),
            "input_select.voice",
            Duration::from_secs(60),
            Arc::new(SystemClock),
        );
        assert_eq!(reader.profile().await, None);
    }

    #[tokio::test]
    async fn statements_use_the_script_and_questions_start_a_conversation() {
        let calls = Calls::default();
        let app = Router::new()
            .route("/api/services/script/turn_on", post(service))
            .route(
                "/api/services/assist_satellite/start_conversation",
                post(service),
            )
            .with_state(calls.clone());
        let base = serve(app).await;
        let announcer = HaAnnouncer::new(
            access(&base),
            "script.schedule_announce",
            "assist.satellite",
        );

        assert_eq!(announcer.announce("Готово.").await, (true, false));
        assert_eq!(announcer.announce("Продолжить?").await, (true, true));

        let calls = calls.0.lock().expect("lock");
        assert_eq!(calls[0].0, "/api/services/script/turn_on");
        assert_eq!(calls[0].1["variables"]["message"], "Готово.");
        assert_eq!(calls[0].1["variables"]["chime"], false);
        assert_eq!(
            calls[1].0,
            "/api/services/assist_satellite/start_conversation"
        );
        assert_eq!(calls[1].1["entity_id"], "assist.satellite");
        assert_eq!(calls[1].1["start_message"], "Продолжить?");
        assert_eq!(calls[1].1["preannounce"], false);
    }

    #[tokio::test]
    async fn a_failing_start_conversation_falls_back_to_the_script() {
        let calls = Calls::default();
        let attempts = Arc::new(AtomicUsize::new(0));
        let failing = attempts.clone();
        let app = Router::new()
            .route("/api/services/script/turn_on", post(service))
            .route(
                "/api/services/assist_satellite/start_conversation",
                post(move || {
                    let failing = failing.clone();
                    async move {
                        failing.fetch_add(1, Ordering::SeqCst);
                        StatusCode::SERVICE_UNAVAILABLE
                    }
                }),
            )
            .with_state(calls.clone());
        let base = serve(app).await;
        let announcer = HaAnnouncer::new(access(&base), "script.announce", "assist.satellite");
        assert_eq!(announcer.announce("Продолжить?").await, (true, false));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        assert_eq!(calls.0.lock().expect("lock").len(), 1);
    }
}
