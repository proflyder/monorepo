//! The fetch → save → threshold → send → record pipeline.
//!
//! This is the Go `internal/service` flow with the alert pass delegated to the
//! pure [`crate::domain::alerts::plan`]. Threshold and history-save failures are
//! logged and ignored, exactly as before.

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use crate::domain::alerts;
use crate::domain::clock::Clock;
use crate::domain::message;
use crate::domain::model::{Rates, Record, SentAlert};
use crate::metrics::Metrics;
use crate::store::Store;

/// A boxed send future used by the two I/O traits below.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Fetches the current rates.
pub trait RatesSource: Send + Sync {
    fn fetch(&self) -> BoxFuture<'_, Result<Rates, String>>;
}

/// Delivers one Telegram message to a chat.
pub trait MessageSender: Send + Sync {
    fn send<'a>(&'a self, chat_id: &'a str, text: &'a str) -> BoxFuture<'a, Result<(), String>>;
}

/// A pipeline failure. `Display` is the raw cause used in the HTTP 502
/// envelope and the `/trigger` reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineError(pub String);

impl fmt::Display for PipelineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for PipelineError {}

/// The pipeline dependencies.
pub struct Pipeline {
    pub source: Arc<dyn RatesSource>,
    pub store: Arc<Store>,
    pub sender: Arc<dyn MessageSender>,
    pub metrics: Arc<Metrics>,
    pub clock: Arc<dyn Clock>,
}

impl fmt::Debug for Pipeline {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Pipeline")
            .field(
                "clock",
                &crate::domain::clock::debug_clock(self.clock.as_ref()),
            )
            .finish_non_exhaustive()
    }
}

impl Pipeline {
    /// Execute one invocation.
    ///
    /// `force_notification` is the manual-trigger contract: skip dedup reads,
    /// always send. `save_to_history` is false for a manual trigger.
    pub async fn run(
        &self,
        job: &str,
        chat_id: &str,
        force_notification: bool,
        save_to_history: bool,
    ) -> Result<(), PipelineError> {
        self.metrics.inc_job_run(job);
        let started = Instant::now();
        let fetched = self.source.fetch().await;
        self.metrics
            .observe_parse(job, started.elapsed().as_secs_f64());
        let rates = match fetched {
            Ok(rates) => rates,
            Err(cause) => {
                self.metrics.inc_job_failure(job);
                tracing::error!(job, error = %cause, "failed to fetch currency rates");
                return Err(PipelineError(cause));
            }
        };
        let now = self.clock.now();

        self.metrics.set_rates(
            rates.usd_to_kzt.buy,
            rates.usd_to_kzt.sell,
            rates.rub_to_kzt.buy,
            rates.rub_to_kzt.sell,
        );
        log_rate("USD", "KZT", rates.usd_to_kzt, now);
        log_rate("RUB", "KZT", rates.rub_to_kzt, now);

        if save_to_history {
            let record = Record {
                timestamp: now,
                rates,
            };
            if let Err(error) = self.store.save_record(&record) {
                tracing::error!(%error, "failed to save history");
            }
        } else {
            tracing::info!("skipping history save (manual trigger)");
        }

        let plan = {
            let store = Arc::clone(&self.store);
            alerts::plan(
                &rates,
                force_notification,
                move |period| {
                    // Records are stored with millisecond precision; cut the
                    // cutoff at the same resolution so the boundary matches
                    // the Go `now.Add(-duration)` comparison.
                    let cutoff_ms = now
                        .as_millisecond()
                        .saturating_sub(period.seconds.saturating_mul(1_000));
                    let cutoff = jiff::Timestamp::from_millisecond(cutoff_ms)
                        .unwrap_or(jiff::Timestamp::UNIX_EPOCH);
                    match store.record_before(cutoff) {
                        Ok(record) => Some(record),
                        Err(crate::store::StoreError::NotFound) => None,
                        Err(error) => {
                            tracing::error!(%error, period = period.name, "failed to get historical data");
                            None
                        }
                    }
                },
                |key| match store_get(&self.store, key) {
                    Ok(value) => value,
                    Err(error) => {
                        tracing::error!(%error, period = %key.period, "failed to read sent alert");
                        None
                    }
                },
            )
        };

        for key in &plan.clears {
            if let Err(error) = self.store.clear_sent_alert(key) {
                tracing::error!(%error, "failed to clear sent alert");
            }
        }

        if plan.alerts.is_empty() && !force_notification {
            tracing::info!("no thresholds exceeded, skipping telegram notification");
            return Ok(());
        }

        let text = message::format(&rates, &plan.alerts);
        tracing::debug!(text, "message content");
        if let Err(cause) = self.sender.send(chat_id, &text).await {
            self.metrics.inc_telegram_failure();
            self.metrics.inc_job_failure(job);
            tracing::error!(chat_id, error = %cause, "failed to send message to telegram");
            return Err(PipelineError(cause));
        }
        // The message is delivered; recording must not be lost. The store call
        // is synchronous and cannot be cancelled like a Go request context.
        for alert in &plan.alerts {
            let record = SentAlert {
                key: alert.key(),
                level: alert.level,
                direction: alert.direction,
                rate_at_alert: alert.new_rate,
                change_percent: alert.change_percent,
                sent_at: Some(now),
            };
            if let Err(error) = self.store.record_sent_alert(&record) {
                tracing::error!(%error, pair = alert.pair.as_str(), "failed to record sent alert");
            }
        }
        Ok(())
    }
}

fn store_get(
    store: &Store,
    key: &crate::domain::model::AlertKey,
) -> Result<Option<SentAlert>, crate::store::StoreError> {
    store.get_sent_alert(key)
}

fn log_rate(from: &str, to: &str, rate: crate::domain::model::ExchangeRate, now: jiff::Timestamp) {
    tracing::info!(
        event = "currency_rate_updated",
        currency_from = from,
        currency_to = to,
        rate_buy = rate.buy,
        rate_sell = rate.sell,
        timestamp = now.as_millisecond(),
        "currency rate updated"
    );
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::domain::clock::FixedClock;
    use crate::domain::model::ExchangeRate;

    struct Source {
        result: Result<Rates, String>,
        calls: AtomicUsize,
    }

    impl RatesSource for Source {
        fn fetch(&self) -> BoxFuture<'_, Result<Rates, String>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move { self.result.clone() })
        }
    }

    struct Sender {
        sent: std::sync::Mutex<Vec<(String, String)>>,
        fail: bool,
    }

    impl MessageSender for Sender {
        fn send<'a>(
            &'a self,
            chat_id: &'a str,
            text: &'a str,
        ) -> BoxFuture<'a, Result<(), String>> {
            Box::pin(async move {
                if self.fail {
                    return Err("telegram down".to_owned());
                }
                self.sent
                    .lock()
                    .expect("lock")
                    .push((chat_id.to_owned(), text.to_owned()));
                Ok(())
            })
        }
    }

    fn rates() -> Rates {
        Rates {
            usd_to_kzt: ExchangeRate {
                buy: 500.0,
                sell: 501.0,
            },
            rub_to_kzt: ExchangeRate {
                buy: 5.0,
                sell: 5.1,
            },
        }
    }

    fn pipeline(source: Result<Rates, String>, fail: bool) -> Pipeline {
        let now: jiff::Timestamp = "2026-10-31T12:00:00Z".parse().expect("now");
        Pipeline {
            source: Arc::new(Source {
                result: source,
                calls: AtomicUsize::new(0),
            }),
            store: Arc::new(Store::open_in_memory().expect("store")),
            sender: Arc::new(Sender {
                sent: std::sync::Mutex::new(Vec::new()),
                fail,
            }),
            metrics: Arc::new(Metrics::new()),
            clock: Arc::new(FixedClock(now)),
        }
    }

    #[tokio::test]
    async fn a_manual_trigger_always_sends_and_does_not_save_history() {
        let pipeline = pipeline(Ok(rates()), false);
        pipeline
            .run("trigger", "123", true, false)
            .await
            .expect("run");
        assert_eq!(pipeline.store.count_records().expect("records"), 0);
    }

    #[tokio::test]
    async fn a_fetch_failure_is_returned_verbatim() {
        let pipeline = pipeline(Err("parse exploded".to_owned()), false);
        let error = pipeline
            .run("trigger", "123", true, false)
            .await
            .expect_err("fetch");
        assert_eq!(error.to_string(), "parse exploded");
    }

    #[tokio::test]
    async fn a_sender_failure_is_reported_and_no_alert_is_recorded() {
        let pipeline = pipeline(Ok(rates()), true);
        let error = pipeline
            .run("trigger", "123", true, false)
            .await
            .expect_err("send");
        assert_eq!(error.to_string(), "telegram down");
        assert_eq!(pipeline.store.count_sent_alerts().expect("sent"), 0);
    }

    #[tokio::test]
    async fn a_firing_alert_is_sent_and_recorded() {
        let pipeline = pipeline(Ok(rates()), false);
        pipeline
            .store
            .save_record(&Record {
                timestamp: "2026-10-31T11:00:00Z".parse().expect("time"),
                rates: Rates {
                    usd_to_kzt: ExchangeRate {
                        buy: 400.0,
                        sell: 400.0,
                    },
                    rub_to_kzt: ExchangeRate {
                        buy: 5.0,
                        sell: 5.1,
                    },
                },
            })
            .expect("seed");
        pipeline
            .run("hourly", "123", false, true)
            .await
            .expect("run");
        let sent = pipeline.store.count_sent_alerts().expect("sent");
        assert!(sent > 0, "hourly alert should be recorded");
    }
}
