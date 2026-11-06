//! The [`home_worker::Handler`] that executes one scheduled event.
//!
//! Lease → plan → ledger duplicate check → HA call → receipt → ack. A terminal
//! `FAILED` is only for events a retry cannot fix; a transient HA failure
//! leaves the event unacknowledged so lease expiry re-delivers it, and a
//! `DELIVERED` ack is never written without a 2xx from Home Assistant.

use std::future::Future;
use std::sync::Arc;

use home_ha::ErrorKind;
use home_worker::{Action, Event, Handler, Outcome, Source};
use serde_json::{Map, Value};

use crate::config::Config;
use crate::domain::{self, PlanError, PlannedAction, Scripts};
use crate::ledger::{ActiveAlarm, Ledger, Receipt};

/// The Home Assistant surface the bridge needs.
pub trait HaClient: Send + Sync {
    fn call_script(
        &self,
        script: &str,
        variables: Map<String, Value>,
    ) -> impl Future<Output = Result<(), home_ha::Error>> + Send;

    fn ping(&self) -> impl Future<Output = Result<(), home_ha::Error>> + Send;
}

impl HaClient for home_ha::Client {
    fn call_script(
        &self,
        script: &str,
        variables: Map<String, Value>,
    ) -> impl Future<Output = Result<(), home_ha::Error>> + Send {
        self.call_script(script, variables)
    }

    fn ping(&self) -> impl Future<Output = Result<(), home_ha::Error>> + Send {
        self.ping()
    }
}

impl<T: HaClient + ?Sized> HaClient for Arc<T> {
    fn call_script(
        &self,
        script: &str,
        variables: Map<String, Value>,
    ) -> impl Future<Output = Result<(), home_ha::Error>> + Send {
        (**self).call_script(script, variables)
    }

    fn ping(&self) -> impl Future<Output = Result<(), home_ha::Error>> + Send {
        (**self).ping()
    }
}

/// The executor. One instance owns the configured queue and the receipt ledger.
pub struct Bridge<H: HaClient> {
    config: Arc<Config>,
    ha: Arc<H>,
    ledger: Arc<Ledger>,
}

impl<H: HaClient> std::fmt::Debug for Bridge<H> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Bridge")
            .field("config", &self.config)
            .field("ledger", &self.ledger)
            .finish_non_exhaustive()
    }
}

impl<H: HaClient> Bridge<H> {
    /// Build the executor.
    #[must_use]
    pub fn new(config: Arc<Config>, ha: Arc<H>, ledger: Arc<Ledger>) -> Self {
        Self { config, ha, ledger }
    }

    /// `"execute"` or `"dry-run"`, matching the Go log line.
    #[must_use]
    pub fn mode(&self) -> &'static str {
        if self.config.execute {
            "execute"
        } else {
            "dry-run"
        }
    }

    /// Effective script names.
    #[must_use]
    pub fn scripts(&self) -> Scripts {
        Scripts {
            alarm: self.config.alarm_script.clone(),
            announce: self.config.announce_script.clone(),
            stop: self.config.stop_script.clone(),
        }
    }

    fn plan(&self, event: &Event) -> Result<PlannedAction, PlanError> {
        domain::plan(event, &self.config.queue, &self.scripts())
    }

    fn record_receipt(&self, event: &Event, script: &str) {
        let completed = self.ledger.now_unix();
        let receipt = Receipt {
            event_id: event.id.clone(),
            source_id: event.source_id.clone(),
            action: domain::action_label(event.action),
            script: script.to_owned(),
            completed_unix: completed,
        };
        match (event.action, event.source) {
            (Action::Fire, Source::Alarm) => {
                let active = ActiveAlarm {
                    source_id: event.source_id.clone(),
                    event_id: event.id.clone(),
                    occurrence_unix: event.occurrence_unix,
                    fire_due_unix: event.due_unix,
                    since_unix: completed,
                };
                if let Err(error) = self.ledger.record_start(receipt, active) {
                    tracing::error!(
                        %error,
                        "active alarm marker write failed; a later STOP may be skipped after restart"
                    );
                }
            }
            (Action::Stop, Source::Alarm) => {
                match self.ledger.record_stop(receipt, &event.source_id) {
                    Ok(true) => {}
                    Ok(false) => tracing::warn!(
                        "active alarm marker changed before STOP was recorded; a newer alarm was left untouched"
                    ),
                    Err(error) => tracing::error!(
                        %error,
                        "receipt ledger write failed; a re-delivery may duplicate the Home Assistant side effect"
                    ),
                }
            }
            _ => {
                if let Err(error) = self.ledger.record(receipt) {
                    tracing::error!(
                        %error,
                        "receipt ledger write failed; a re-delivery may duplicate the Home Assistant side effect"
                    );
                }
            }
        }
    }
}

impl<H: HaClient> Handler for Bridge<H> {
    fn order(&self, events: &mut Vec<Event>) {
        domain::order_events(events);
    }

    async fn handle(&self, event: &Event) -> Outcome {
        if event.id.is_empty() {
            tracing::error!("scheduler returned an event without id; ignoring it");
            return Outcome::Retry;
        }

        let mut plan = match self.plan(event) {
            Ok(plan) => plan,
            Err(error) => {
                tracing::warn!(
                    reason = %error,
                    "event cannot be executed by the HA executor"
                );
                return Outcome::Failed(error.to_string());
            }
        };

        if self.ledger.seen(&event.id) {
            tracing::info!("duplicate delivery suppressed by receipt ledger");
            return Outcome::Delivered(
                "duplicate delivery; Home Assistant side effect already applied".to_owned(),
            );
        }

        if let Some(reason) = plan.skip_reason.as_deref() {
            tracing::info!(reason, "event acknowledged without an HA call");
            return Outcome::Skipped(reason.to_owned());
        }

        // A global alarm_stop may only silence the alarm this bridge
        // started. A stale STOP for an older occurrence, or a STOP that
        // races a newer alarm, must never touch the ringing alarm.
        if event.action == Action::Stop {
            let active = self.ledger.active_alarm();
            let matches = active
                .as_ref()
                .is_some_and(|active| domain::stop_matches_active(event, active));
            let Some(active) = active.filter(|_| matches) else {
                let reason = "stop ignored: source or occurrence does not match the active alarm started by this bridge";
                tracing::info!(reason, "event acknowledged without an HA call");
                return Outcome::Skipped(reason.to_owned());
            };
            // Home Assistant writes this identity synchronously inside
            // alarm_start before returning 2xx, and one consumer handles
            // one batch at a time, so a newer FIRE always owns a different
            // stored id. The script fails closed when the id is unknown
            // while the alarm is still ringing.
            plan.variables.insert(
                "expected_source_id".to_owned(),
                Value::String(active.source_id),
            );
            plan.variables.insert(
                "expected_event_id".to_owned(),
                Value::String(active.event_id),
            );
        }

        let script = plan.script.clone();
        if let Err(error) = self.ha.call_script(&script, plan.variables).await {
            let status = error_status(&error);
            let permanent = error.kind() == ErrorKind::Permanent;
            let detail = ha_error_detail(&error);
            if permanent && !matches!(status, Some(401 | 403)) {
                tracing::error!(%error, "home assistant rejected the call; failing the event");
                return Outcome::Failed(format!(
                    "home assistant rejected the call: {}",
                    sanitize_detail(&detail)
                ));
            }
            if event.attempts >= self.config.max_attempts {
                tracing::error!(
                    %error,
                    attempts = event.attempts,
                    max_attempts = self.config.max_attempts,
                    "home assistant call kept failing; failing the event after the attempt limit"
                );
                return Outcome::Failed(format!(
                    "home assistant call failed repeatedly: {}",
                    sanitize_detail(&detail)
                ));
            }
            // Transient or auth/configuration error: leave the event
            // unacknowledged so lease expiry re-delivers it.
            tracing::warn!(
                %error,
                attempts = event.attempts,
                "home assistant call failed; leaving event unacknowledged for retry"
            );
            return Outcome::Retry;
        }

        self.record_receipt(event, &script);
        tracing::info!(script, "home assistant accepted the call");
        Outcome::Delivered(format!("home assistant accepted {script}"))
    }
}

fn error_status(error: &home_ha::Error) -> Option<u16> {
    match error {
        home_ha::Error::Status { status, .. } => Some(*status),
        _ => None,
    }
}

/// Render an HA failure the way `packages/go/httpapi.Error` does. Only the
/// operation and a status are included: a response body may carry deployment
/// detail and is never logged.
fn ha_error_detail(error: &home_ha::Error) -> String {
    match error {
        home_ha::Error::Status { op, status, .. } => format!("{op}: status={status}"),
        home_ha::Error::Transport { op, .. } => format!("{op}: request failed"),
        home_ha::Error::Decode { op } => format!("{op}: invalid response"),
        home_ha::Error::Invalid { op, detail } => format!("{op}: {detail}"),
        home_ha::Error::Certificate { op } => format!("{op}: unusable CA"),
    }
}

/// Collapse whitespace and bound the ack detail. Truncation stays on a UTF-8
/// boundary, unlike the Go byte slice which could split a Cyrillic character.
fn sanitize_detail(text: &str) -> String {
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate_bytes(&joined, 300)
}

fn truncate_bytes(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_owned();
    }
    let mut end = limit;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use home_ha::Error;
    use home_worker::{Action, Event, Handler, Outcome, Source};
    use serde_json::{Map, Value, json};

    use super::{Bridge, HaClient, sanitize_detail};
    use crate::config::Config;
    use crate::ledger::Ledger;

    #[derive(Default)]
    struct FakeHa {
        calls: Mutex<Vec<(String, Map<String, Value>)>>,
        status: Option<u16>,
    }

    impl HaClient for FakeHa {
        async fn call_script(
            &self,
            script: &str,
            variables: Map<String, Value>,
        ) -> Result<(), Error> {
            self.calls
                .lock()
                .expect("lock")
                .push((script.to_owned(), variables));
            self.status.map_or(Ok(()), |status| {
                Err(Error::Status {
                    op: "call script",
                    status,
                    message: String::new(),
                })
            })
        }

        async fn ping(&self) -> Result<(), Error> {
            Ok(())
        }
    }

    fn config() -> Config {
        Config::load_from(
            &|key| match key {
                "HA_URL" => Some("http://ha.example:8123".to_owned()),
                "SCHEDULER_ENDPOINT" => Some("http://scheduler.example:8098".to_owned()),
                "SCHEDULER_TOKEN" => Some("scheduler-secret".to_owned()),
                "HA_TOKEN" => Some("ha-secret".to_owned()),
                "BRIDGE_CONSUMER_ID" => Some("ha-bridge-home".to_owned()),
                "BRIDGE_EXECUTE" => Some("true".to_owned()),
                _ => None,
            },
            &|_| Ok(Vec::new()),
        )
        .expect("config")
    }

    fn alarm(id: &str, action: Action, attempts: i32) -> Event {
        Event {
            id: id.to_owned(),
            source_id: "alarm-1".to_owned(),
            task_type: "alarm.fire".to_owned(),
            queue: "home".to_owned(),
            label: "Wake up".to_owned(),
            payload: String::new(),
            action,
            source: Source::Alarm,
            attempts,
            due_unix: 1700000060,
            occurrence_unix: 1700000000,
            lease_until_unix: 0,
        }
    }

    fn bridge(ha: Arc<FakeHa>) -> Bridge<FakeHa> {
        Bridge::new(
            Arc::new(config()),
            ha,
            Arc::new(Ledger::memory_with_clock(|| 1_700_000_000)),
        )
    }

    #[tokio::test]
    async fn fire_calls_the_alarm_script_and_records_the_marker() {
        let ha = Arc::new(FakeHa::default());
        let bridge = bridge(Arc::clone(&ha));
        let outcome = bridge.handle(&alarm("evt-fire", Action::Fire, 0)).await;
        assert_eq!(
            outcome,
            Outcome::Delivered("home assistant accepted alarm_start".to_owned())
        );
        let calls = ha.calls.lock().expect("lock");
        assert_eq!(calls[0].0, "alarm_start");
        assert_eq!(calls[0].1["label"], json!("Wake up"));
        assert!(bridge.ledger.active_alarm().is_some());
    }

    #[tokio::test]
    async fn a_stale_stop_is_skipped_without_calling_ha() {
        let ha = Arc::new(FakeHa::default());
        let bridge = bridge(Arc::clone(&ha));
        let outcome = bridge.handle(&alarm("evt-stop", Action::Stop, 0)).await;
        assert!(matches!(outcome, Outcome::Skipped(_)));
        assert!(ha.calls.lock().expect("lock").is_empty());
    }

    #[tokio::test]
    async fn a_permanent_rejection_fails_immediately() {
        let ha = Arc::new(FakeHa {
            status: Some(404),
            ..FakeHa::default()
        });
        let bridge = bridge(Arc::clone(&ha));
        let outcome = bridge.handle(&alarm("evt-fire", Action::Fire, 1)).await;
        assert_eq!(
            outcome,
            Outcome::Failed("home assistant rejected the call: call script: status=404".to_owned())
        );
    }

    #[tokio::test]
    async fn bridge_max_attempts_keeps_the_go_behaviour() {
        // The scheduler already increments `attempts` when leasing, so the
        // limit is reached when the delivered value equals the configured one.
        let ha = Arc::new(FakeHa {
            status: Some(401),
            ..FakeHa::default()
        });
        let bridge = bridge(Arc::clone(&ha));
        assert_eq!(
            bridge.handle(&alarm("evt-fire", Action::Fire, 9)).await,
            Outcome::Retry
        );
        assert_eq!(
            bridge.handle(&alarm("evt-fire", Action::Fire, 10)).await,
            Outcome::Failed(
                "home assistant call failed repeatedly: call script: status=401".to_owned()
            )
        );
    }

    #[test]
    fn detail_is_collapsed_and_safely_truncated() {
        assert_eq!(sanitize_detail("  a\n b  "), "a b");
        let text = "ж".repeat(200);
        let truncated = sanitize_detail(&text);
        assert!(truncated.len() <= 300);
        assert!(truncated.chars().all(|character| character == 'ж'));
    }
}
