//! The routing service: read the profile, ask the gateway and turn the pair
//! into a [`Decision`] plus the background announcement.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use super::{Gateway, ProfileReader, Results};
use crate::domain::profile::Profile;
use crate::domain::routing::{Decision, GatewayAnswer, decide};

/// Resolves profile routing for the last user utterance.
pub struct VoiceRouter {
    profile: Arc<ProfileReader>,
    gateway: Arc<Gateway>,
    results: Option<Arc<Results>>,
    counts: Mutex<BTreeMap<&'static str, u64>>,
}

impl std::fmt::Debug for VoiceRouter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VoiceRouter")
            .field("results", &self.results.is_some())
            .finish_non_exhaustive()
    }
}

impl VoiceRouter {
    #[must_use]
    pub fn new(
        profile: Arc<ProfileReader>,
        gateway: Arc<Gateway>,
        results: Option<Arc<Results>>,
    ) -> Self {
        Self {
            profile,
            gateway,
            results,
            counts: Mutex::new(BTreeMap::new()),
        }
    }

    /// Resolves profile and route for one utterance. It never fails: every
    /// failure becomes an explicit decision so the fail-closed contract holds.
    pub async fn route(&self, utterance: &str) -> Decision {
        let profile = self.profile.profile().await;
        let answer = match profile {
            Some(profile) => self
                .gateway
                .utterance(utterance, profile)
                .await
                .map_or(GatewayAnswer::Failed, GatewayAnswer::Ok),
            None => GatewayAnswer::Failed,
        };
        let decision = decide(profile, answer);
        if let (Some(results), Some(announcement), Some(profile)) =
            (&self.results, &decision.announcement, decision.profile)
        {
            results.start(announcement, profile);
        }
        self.count(decision.outcome);
        decision
    }

    fn count(&self, outcome: &'static str) {
        let mut counts = self.counts.lock().unwrap_or_else(PoisonError::into_inner);
        *counts.entry(outcome).or_insert(0) += 1;
    }

    /// One outcome counter, for diagnostics.
    #[must_use]
    pub fn count_of(&self, outcome: &str) -> u64 {
        self.counts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(outcome)
            .copied()
            .unwrap_or(0)
    }
}

/// The profile as a structural log value.
#[must_use]
pub fn profile_or_unknown(profile: Option<Profile>) -> &'static str {
    profile.map_or("unknown", Profile::as_str)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use axum::routing::{get, post};
    use axum::{Json, Router};
    use serde_json::{Value, json};

    use super::VoiceRouter;
    use crate::domain::clock::SystemClock;
    use crate::domain::profile::Profile;
    use crate::services::{Gateway, HaAccess, ProfileReader};

    async fn serve(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        format!("http://{address}")
    }

    fn reader(base: &str) -> Arc<ProfileReader> {
        let access = Arc::new(HaAccess::new(
            base,
            Duration::from_secs(2),
            Arc::new(|| "token".to_owned()),
        ));
        Arc::new(ProfileReader::new(
            access,
            "input_select.voice",
            Duration::from_secs(1),
            Arc::new(SystemClock),
        ))
    }

    fn gateway(base: &str) -> Arc<Gateway> {
        Arc::new(
            Gateway::new(
                base,
                Arc::new(|| "token".to_owned()),
                Duration::from_secs(2),
            )
            .expect("gateway"),
        )
    }

    #[tokio::test]
    async fn routes_through_the_profile_and_the_gateway() {
        let ha = serve(Router::new().route(
            "/api/states/input_select.voice",
            get(|| async { Json(json!({"state": "Рабочий"})) }),
        ))
        .await;
        let gw = serve(Router::new().route(
            "/v1/voice/utterance",
            post(|Json(body): Json<Value>| async move {
                assert_eq!(body["profile"], "work");
                Json(json!({"route": "hermes"}))
            }),
        ))
        .await;
        let router = VoiceRouter::new(reader(&ha), gateway(&gw), None);
        let decision = router.route("проверь").await;
        assert!(!decision.handled);
        assert_eq!(decision.outcome, "hermes");
        assert_eq!(decision.profile, Some(Profile::Work));
        assert_eq!(router.count_of("hermes"), 1);
    }

    #[tokio::test]
    async fn an_unknown_profile_never_calls_the_gateway() {
        let ha = serve(Router::new().route(
            "/api/states/input_select.voice",
            get(|| async { Json(json!({"state": "unknown"})) }),
        ))
        .await;
        let gw = serve(Router::new().route(
            "/v1/voice/utterance",
            post(|| async { Json(json!({"route": "hermes"})) }),
        ))
        .await;
        let router = VoiceRouter::new(reader(&ha), gateway(&gw), None);
        let decision = router.route("проверь").await;
        assert!(decision.handled);
        assert_eq!(decision.outcome, "profile_unknown");
    }
}
