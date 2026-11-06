//! Announces turns the owner did not start by voice: child completions and
//! dispatcher messages.
//!
//! Two modes exist:
//!
//! * with the durable queue every non-owner turn is enqueued for the work
//!   profile, regardless of the current speaker profile; the queue consumer
//!   waits for the work profile and an idle station. A restart rescans a recent
//!   window and the idempotency key drops duplicates;
//! * without the queue the cursor advances, the profile is re-read and the batch
//!   is spoken only in the work profile as one utterance.
//!
//! Errors back off 2 s → 60 s and never speak or reach Hermes.

use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use voice_contract::UpdateItem;

use super::queue::Producer;
use super::results::sleep;
use super::{Gateway, HaAnnouncer, ProfileReader};
use crate::domain::clock::Clock;
use crate::domain::profile::Profile;
use crate::domain::sanitize::{announce_text, trim_space};

/// Tuning of the dispatcher-updates poller.
#[derive(Debug, Clone)]
pub struct UpdatesOptions {
    pub rescan: i64,
    pub wait: u64,
    pub max_chars: usize,
    pub ttl: i64,
    pub retry_min: Duration,
    pub retry_max: Duration,
    pub idle_pause: Duration,
}

impl Default for UpdatesOptions {
    fn default() -> Self {
        Self {
            rescan: 300,
            wait: 25,
            max_chars: 200,
            ttl: 10_800,
            retry_min: Duration::from_secs(2),
            retry_max: Duration::from_secs(60),
            idle_pause: Duration::from_millis(100),
        }
    }
}

/// Long-polls dispatcher updates and announces or enqueues them.
pub struct Updates {
    gateway: Arc<Gateway>,
    profile: Arc<ProfileReader>,
    announcer: Arc<HaAnnouncer>,
    producer: Option<Arc<Producer>>,
    clock: Arc<dyn Clock>,
    options: UpdatesOptions,
}

impl std::fmt::Debug for Updates {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Updates")
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

impl Updates {
    #[must_use]
    pub fn new(
        gateway: Arc<Gateway>,
        profile: Arc<ProfileReader>,
        announcer: Arc<HaAnnouncer>,
        producer: Option<Arc<Producer>>,
        clock: Arc<dyn Clock>,
        options: UpdatesOptions,
    ) -> Self {
        Self {
            gateway,
            profile,
            announcer,
            producer,
            clock,
            options,
        }
    }

    fn queue_enabled(&self) -> bool {
        self.producer
            .as_ref()
            .is_some_and(|producer| producer.enabled())
    }

    /// Polls until cancelled. It never returns an error: the poller must never
    /// die.
    pub async fn run(&self, cancel: CancellationToken) {
        let queued = self.queue_enabled();
        let mut cursor: Option<i64> = None;
        let mut retry = self.options.retry_min;
        while !cancel.is_cancelled() {
            if cursor.is_none() {
                match self.gateway.updates(None, 1).await {
                    Err(_) => {
                        retry = backoff(&cancel, retry, self.options.retry_max).await;
                        continue;
                    }
                    Ok(response) => {
                        let Some(next) = response.cursor.filter(|cursor| *cursor >= 0) else {
                            retry = backoff(&cancel, retry, self.options.retry_max).await;
                            continue;
                        };
                        if queued {
                            cursor = Some((next - self.options.rescan).max(0));
                            tracing::info!(
                                event = "updates_bootstrap",
                                cursor = next,
                                rescan_from = cursor.unwrap_or_default(),
                                "updates poller warmed up"
                            );
                        } else {
                            cursor = Some(next);
                            tracing::info!(
                                event = "updates_bootstrap",
                                cursor = next,
                                "updates poller warmed up"
                            );
                        }
                        retry = self.options.retry_min;
                        if !sleep(&cancel, self.options.idle_pause).await {
                            return;
                        }
                        continue;
                    }
                }
            }

            let Ok(response) = self.gateway.updates(cursor, self.options.wait).await else {
                retry = backoff(&cancel, retry, self.options.retry_max).await;
                continue;
            };
            let Some(next) = response.cursor.filter(|cursor| *cursor >= 0) else {
                retry = backoff(&cancel, retry, self.options.retry_max).await;
                continue;
            };
            cursor = Some(next);
            retry = self.options.retry_min;

            if !response.items.is_empty() {
                if queued {
                    let (enqueued, stale) = self.enqueue_batch(&response.items).await;
                    tracing::info!(
                        event = "updates",
                        items = response.items.len(),
                        queued = enqueued,
                        stale,
                        cursor = next,
                        "updates enqueued"
                    );
                } else {
                    let (announced, listening, profile) =
                        self.announce_batch(&response.items).await;
                    tracing::info!(
                        event = "updates",
                        items = response.items.len(),
                        announced,
                        listen = listening,
                        profile,
                        cursor = next,
                        "updates announced"
                    );
                }
            }
            if !sleep(&cancel, self.options.idle_pause).await {
                return;
            }
        }
    }

    /// Enqueues each non-owner update and returns `(queued, skipped_stale)`.
    async fn enqueue_batch(&self, items: &[UpdateItem]) -> (usize, usize) {
        let mut queued = 0;
        let mut stale = 0;
        let Some(producer) = &self.producer else {
            return (queued, stale);
        };
        let now = self.clock.now().unix_timestamp();
        for item in items {
            let speech = trim_space(&item.spoken);
            if speech.is_empty() {
                continue;
            }
            let created = completed_unix(Some(item.completed_at.unwrap_or_default()));
            if let Some(created) = created
                && now - created > self.options.ttl
            {
                stale += 1;
                continue;
            }
            let text = announce_text(speech, self.options.max_chars);
            if text.is_empty() {
                continue;
            }
            let seq = item.seq.to_string();
            if producer
                .enqueue(&text, Profile::Work, "dispatcher", &seq, created)
                .await
            {
                queued += 1;
            }
        }
        (queued, stale)
    }

    async fn announce_batch(&self, items: &[UpdateItem]) -> (bool, bool, &'static str) {
        let spoken: Vec<&str> = items
            .iter()
            .map(|item| trim_space(&item.spoken))
            .filter(|spoken| !spoken.is_empty())
            .collect();
        // Work profile only: read fresh from HA, no cache.
        let profile = self.profile.fetch().await;
        let Some(Profile::Work) = profile else {
            return (false, false, profile.map_or("unknown", Profile::as_str));
        };
        if spoken.is_empty() {
            return (false, false, Profile::Work.as_str());
        }
        let text = announce_text(&spoken.join(" "), self.options.max_chars);
        if text.is_empty() {
            return (false, false, Profile::Work.as_str());
        }
        let (announced, listening) = self.announcer.announce(&text).await;
        (announced, listening, Profile::Work.as_str())
    }
}

/// The completion time of one updates item in epoch seconds. Millisecond values
/// are converted; `<= 0` is unknown.
#[must_use]
pub fn completed_unix(completed_at: Option<i64>) -> Option<i64> {
    let value = completed_at?;
    if value <= 0 {
        return None;
    }
    if value > 1_000_000_000_000 {
        return Some(value / 1000);
    }
    Some(value)
}

async fn backoff(cancel: &CancellationToken, current: Duration, max: Duration) -> Duration {
    let next = (current * 2).min(max);
    let _ = sleep(cancel, current).await;
    next
}

#[cfg(test)]
mod tests {
    use super::{UpdatesOptions, completed_unix};

    #[test]
    fn completed_at_accepts_seconds_and_milliseconds() {
        assert_eq!(completed_unix(Some(1_700_000_000)), Some(1_700_000_000));
        assert_eq!(completed_unix(Some(1_700_000_000_123)), Some(1_700_000_000));
        assert_eq!(completed_unix(Some(0)), None);
        assert_eq!(completed_unix(Some(-5)), None);
        assert_eq!(completed_unix(None), None);
    }

    #[test]
    fn defaults_match_the_documented_contract() {
        let options = UpdatesOptions::default();
        assert_eq!(options.rescan, 300);
        assert_eq!(options.wait, 25);
        assert_eq!(options.max_chars, 200);
        assert_eq!(options.ttl, 10_800);
        assert_eq!(options.retry_min.as_secs(), 2);
        assert_eq!(options.retry_max.as_secs(), 60);
    }
}
