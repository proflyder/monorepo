//! The durable announcement queue: a producer that stores a spoken answer as
//! a `voice.announce` task and a consumer that speaks it only when the station
//! is idle.
//!
//! The payload keys, task type, queue names, idempotency keys and ack outcomes
//! are a wire contract; see [`crate::domain::queue_payload`].

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use home_proto::SchedulerClient;
use home_proto::v1::{AckOutcome as ProtoOutcome, EnqueueTaskRequest, ListDueEventsRequest};
use home_worker::{Ack, AckOutcome, Event, Handler, LeaseRequest, Outcome, Queue, QueueError};

use super::{HaAnnouncer, ProfileReader, TokenProvider};
use crate::domain::announce::with_stale_prefix;
use crate::domain::clock::Clock;
use crate::domain::profile::Profile;
use crate::domain::queue_payload::{self, AnnouncePayload, TASK_TYPE};
use crate::domain::sanitize::trim_space;

/// Default lease duration of one announcement.
pub const LEASE_SECONDS: i32 = 120;
/// An answer that waited longer than this gets the "Пока вас не было: " prefix.
pub const STALE_AFTER: Duration = Duration::from_secs(120);
/// How long the station may take to report busy after an announcement.
pub const BUSY_GRACE: Duration = Duration::from_secs(10);
/// How long to wait for the station to become idle again.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(180);
/// Ack detail when the station never settled.
pub const DETAIL_IDLE: &str = "idle_timeout";

/// Scheduler URL plus a token that is re-read on every call.
pub struct SchedulerAccess {
    url: String,
    timeout: Duration,
    token: TokenProvider,
    cache: Mutex<Option<(String, SchedulerClient)>>,
}

impl std::fmt::Debug for SchedulerAccess {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SchedulerAccess")
            .field("url", &self.url)
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

impl SchedulerAccess {
    #[must_use]
    pub fn new(url: &str, timeout: Duration, token: TokenProvider) -> Self {
        Self {
            url: url.trim().trim_end_matches('/').to_owned(),
            timeout,
            token,
            cache: Mutex::new(None),
        }
    }

    fn client(&self) -> Result<SchedulerClient, home_proto::SchedulerError> {
        let token = (self.token)();
        let cached_client = {
            let cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
            cache
                .as_ref()
                .filter(|(cached, _)| *cached == token)
                .map(|(_, client)| client.clone())
        };
        if let Some(client) = cached_client {
            return Ok(client);
        }
        let client = SchedulerClient::new(
            &self.url,
            home_secret::Token::new(token.clone()),
            self.timeout,
        )?;
        let mut cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        *cache = Some((token, client.clone()));
        drop(cache);
        Ok(client)
    }
}

/// Enqueues durable announcements.
pub struct Producer {
    access: Arc<SchedulerAccess>,
    enabled: bool,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for Producer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Producer")
            .field("enabled", &self.enabled)
            .finish_non_exhaustive()
    }
}

impl Producer {
    #[must_use]
    pub fn new(access: Arc<SchedulerAccess>, enabled: bool, clock: Arc<dyn Clock>) -> Self {
        Self {
            access,
            enabled,
            clock,
        }
    }

    /// Whether the queue is configured and switched on.
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Stores one spoken answer and returns `false` when the queue is off, the
    /// text or profile is unusable, or the scheduler is unreachable. The caller
    /// then falls back to a direct announcement.
    pub async fn enqueue(
        &self,
        speech: &str,
        profile: Profile,
        thread_id: &str,
        seq: &str,
        created_unix: Option<i64>,
    ) -> bool {
        if !self.enabled || speech.is_empty() {
            return false;
        }
        let now = self.clock.now().unix_timestamp();
        let created = created_unix.unwrap_or(now);
        let payload = AnnouncePayload::new(speech, profile.as_str(), thread_id, seq, created);
        let Ok(payload_json) = payload.encode() else {
            return false;
        };
        let Ok(client) = self.access.client() else {
            return false;
        };
        let request = EnqueueTaskRequest {
            task_type: TASK_TYPE.to_owned(),
            payload_json,
            run_at_unix: now,
            queue: queue_payload::queue_name(profile.as_str()),
            idempotency_key: queue_payload::idempotency_key(thread_id, seq),
            ..Default::default()
        };
        client.enqueue_task(request).await.is_ok()
    }
}

/// [`Queue`] adapter over [`SchedulerAccess`].
#[derive(Debug, Clone)]
pub struct SchedulerQueue {
    access: Arc<SchedulerAccess>,
}

impl SchedulerQueue {
    #[must_use]
    pub fn new(access: Arc<SchedulerAccess>) -> Self {
        Self { access }
    }
}

impl Queue for SchedulerQueue {
    fn lease(
        &self,
        request: LeaseRequest,
    ) -> impl Future<Output = Result<Vec<Event>, QueueError>> + Send {
        let access = Arc::clone(&self.access);
        async move {
            let client = access.client()?;
            let events = client
                .list_due_events(ListDueEventsRequest {
                    consumer_id: request.consumer_id,
                    lease_seconds: request.lease_seconds,
                    limit: request.limit,
                    queue: request.queue,
                    ..Default::default()
                })
                .await?;
            Ok(events.into_iter().map(Event::from).collect())
        }
    }

    fn ack(&self, ack: Ack) -> impl Future<Output = Result<(), QueueError>> + Send {
        let access = Arc::clone(&self.access);
        async move {
            let client = access.client()?;
            let outcome = match ack.outcome {
                AckOutcome::Delivered => ProtoOutcome::ACK_OUTCOME_DELIVERED,
                AckOutcome::Failed => ProtoOutcome::ACK_OUTCOME_FAILED,
                AckOutcome::Skipped => ProtoOutcome::ACK_OUTCOME_SKIPPED,
            };
            client
                .ack_event(&ack.event_id, &ack.consumer_id, outcome, &ack.detail)
                .await?;
            Ok(())
        }
    }
}

/// Gate: the station is idle *and* the current profile owns this queue.
pub struct StationGate {
    profile: Arc<ProfileReader>,
    satellite: String,
    media_player: String,
    expected: Profile,
}

impl StationGate {
    #[must_use]
    pub fn new(
        profile: Arc<ProfileReader>,
        satellite: impl Into<String>,
        media_player: impl Into<String>,
        expected: Profile,
    ) -> Self {
        Self {
            profile,
            satellite: satellite.into(),
            media_player: media_player.into(),
            expected,
        }
    }
}

impl home_worker::Gate for StationGate {
    fn ready(&self) -> Pin<Box<dyn Future<Output = bool> + Send + '_>> {
        Box::pin(async move {
            let current = self.profile.fetch().await;
            if current != Some(self.expected) {
                return false;
            }
            let satellite = self.profile.state(&self.satellite).await;
            let media = self.profile.state(&self.media_player).await;
            station_idle(satellite.as_deref(), media.as_deref())
        })
    }
}

/// Speaks one leased event.
pub struct AnnouncementHandler {
    announcer: Arc<HaAnnouncer>,
    profile: Arc<ProfileReader>,
    satellite: String,
    media_player: String,
    ttl_seconds: i64,
    clock: Arc<dyn Clock>,
    poll: Duration,
}

impl AnnouncementHandler {
    #[must_use]
    pub fn new(
        announcer: Arc<HaAnnouncer>,
        profile: Arc<ProfileReader>,
        satellite: impl Into<String>,
        media_player: impl Into<String>,
        ttl_seconds: i64,
        clock: Arc<dyn Clock>,
        poll: Duration,
    ) -> Self {
        Self {
            announcer,
            profile,
            satellite: satellite.into(),
            media_player: media_player.into(),
            ttl_seconds,
            clock,
            poll,
        }
    }

    /// Waits for an issued announcement to finish; `true` when the station is
    /// idle again. The station first gets a grace period to report busy (HA
    /// service calls are asynchronous), then the overall idle budget.
    async fn wait_until_idle(&self) -> bool {
        let start = Instant::now();
        let mut busy_seen = false;
        while start.elapsed() < IDLE_TIMEOUT {
            let satellite = self.profile.state(&self.satellite).await;
            let media = self.profile.state(&self.media_player).await;
            let idle = station_idle(satellite.as_deref(), media.as_deref());
            if !idle {
                busy_seen = true;
            } else if busy_seen || start.elapsed() >= BUSY_GRACE {
                return true;
            }
            tokio::time::sleep(self.poll).await;
        }
        false
    }
}

impl Handler for AnnouncementHandler {
    async fn handle(&self, event: &Event) -> Outcome {
        {
            let payload = AnnouncePayload::decode(&event.payload)
                .unwrap_or_else(|| AnnouncePayload::new("", "", "", "", 0));
            let speech = payload.text;
            if trim_space(&speech).is_empty() {
                return Outcome::Skipped("empty_text".to_owned());
            }
            let now = self.clock.now().unix_timestamp();
            let created = if payload.created_unix > 0 {
                Some(payload.created_unix)
            } else {
                None
            };
            let age = created.map_or(0, |created| (now - created).max(0));
            if created.is_some() && age > self.ttl_seconds {
                return Outcome::Skipped("stale".to_owned());
            }
            let speech = with_stale_prefix(&speech, age, STALE_AFTER.as_secs() as i64);
            let (announced, _listening) = self.announcer.announce(&speech).await;
            if !announced {
                // Leave the event leased: expiry returns it to PENDING.
                return Outcome::Retry;
            }
            if self.wait_until_idle().await {
                Outcome::Delivered(String::new())
            } else {
                Outcome::Delivered(DETAIL_IDLE.to_owned())
            }
        }
    }
}

/// Reports whether the speaker is free for a queued announcement. Unknown or
/// unavailable state is busy: speaking into an unknown station would interrupt
/// a dialog or another announcement. Music `playing` is fine; `announcing` is
/// the HA announcement channel.
#[must_use]
pub fn station_idle(satellite: Option<&str>, media: Option<&str>) -> bool {
    let Some(satellite) = satellite else {
        return false;
    };
    if satellite.is_empty() || satellite == "unknown" || satellite == "unavailable" {
        return false;
    }
    if satellite != "idle" {
        return false;
    }
    let Some(media) = media else {
        return false;
    };
    if media.is_empty() || media == "unknown" || media == "unavailable" {
        return false;
    }
    media != "announcing"
}

/// Builds a `home_worker::Worker` lease for one profile.
#[must_use]
pub fn lease_request(consumer_id: &str, profile: Profile) -> LeaseRequest {
    LeaseRequest {
        consumer_id: consumer_id.to_owned(),
        queue: queue_payload::queue_name(profile.as_str()),
        lease_seconds: LEASE_SECONDS,
        limit: 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{STALE_AFTER, station_idle};
    use crate::domain::announce::with_stale_prefix;

    #[test]
    fn station_is_idle_only_when_both_entities_are_known_and_free() {
        assert!(station_idle(Some("idle"), Some("idle")));
        assert!(station_idle(Some("idle"), Some("playing")));
        assert!(!station_idle(Some("idle"), Some("announcing")));
        assert!(!station_idle(Some("listening"), Some("idle")));
        assert!(!station_idle(Some("unknown"), Some("idle")));
        assert!(!station_idle(Some("unavailable"), Some("idle")));
        assert!(!station_idle(None, Some("idle")));
        assert!(!station_idle(Some("idle"), None));
        assert!(!station_idle(Some(""), Some("idle")));
    }

    #[test]
    fn stale_prefix_uses_the_documented_threshold() {
        assert_eq!(
            with_stale_prefix("Готово.", 121, STALE_AFTER.as_secs() as i64),
            "Пока вас не было: Готово."
        );
    }
}
