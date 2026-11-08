//! The `control` queue executor.
//!
//! The phone enqueues a generic task (`task_type` = `home.control`) on the
//! dedicated `control` queue; this handler leases that queue, resolves the
//! task's entry id and action through the catalog discovered from Home
//! Assistant right now, calls Home Assistant and acknowledges.
//!
//! The payload is data only: it names a catalog entry, and an id that is not in
//! the current catalog is rejected, so it can never reach an entity the
//! discovery rules do not expose.

use std::future::Future;
use std::sync::Arc;

use home_ha::ErrorKind;
use home_worker::{Event, Handler, Outcome, Source};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::catalog::{self, Catalog, Params, ServiceCall};
use crate::control_config::ControlConfig;
use crate::discovery::CatalogSource;

/// The Home Assistant side-effect surface.
pub trait ControlHa: Send + Sync {
    fn call_service(
        &self,
        domain: &str,
        service: &str,
        entity: &str,
        data: Map<String, Value>,
    ) -> impl Future<Output = Result<(), home_ha::Error>> + Send;
}

impl ControlHa for home_ha::Client {
    fn call_service(
        &self,
        domain: &str,
        service: &str,
        entity: &str,
        data: Map<String, Value>,
    ) -> impl Future<Output = Result<(), home_ha::Error>> + Send {
        self.call_service_on(domain, service, entity, Value::Object(data))
    }
}

impl<T: ControlHa + ?Sized> ControlHa for Arc<T> {
    fn call_service(
        &self,
        domain: &str,
        service: &str,
        entity: &str,
        data: Map<String, Value>,
    ) -> impl Future<Output = Result<(), home_ha::Error>> + Send {
        (**self).call_service(domain, service, entity, data)
    }
}

/// The executor. One instance owns the configured queue.
pub struct Control<H: ControlHa> {
    config: Arc<ControlConfig>,
    catalog: Arc<dyn CatalogSource>,
    ha: Arc<H>,
}

impl<H: ControlHa> std::fmt::Debug for Control<H> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Control")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl<H: ControlHa> Control<H> {
    /// Build the executor.
    #[must_use]
    pub fn new(config: Arc<ControlConfig>, catalog: Arc<dyn CatalogSource>, ha: Arc<H>) -> Self {
        Self {
            config,
            catalog,
            ha,
        }
    }

    /// Resolve one event against the current catalog.
    fn plan(&self, event: &Event, catalog: &Catalog) -> Result<(ServiceCall, String), PlanError> {
        if !event.queue.is_empty() && event.queue != self.config.queue {
            return Err(PlanError::Queue {
                actual: event.queue.clone(),
                expected: self.config.queue.clone(),
            });
        }
        if event.source != Source::Task || event.task_type != catalog::TASK_TYPE {
            return Err(PlanError::Unsupported {
                kind: source_label(event.source).to_owned(),
                task_type: event.task_type.clone(),
            });
        }
        let command: Command =
            serde_json::from_str(&event.payload).map_err(|_| PlanError::Payload)?;
        let entry = catalog
            .lookup(&command.entry)
            .ok_or_else(|| PlanError::Entry {
                entry: detail(&command.entry),
            })?;
        let call = entry
            .resolve(&command.action, &command.params)
            .map_err(PlanError::Resolve)?;
        Ok((call, entry.id.clone()))
    }

    async fn run_event(&self, event: &Event) -> Outcome {
        let snapshot = match self.catalog.get().await {
            Ok(snapshot) => snapshot,
            Err(error) => {
                // Home Assistant is unreachable, so the command cannot run
                // either: leave it unacknowledged and let the lease expiry
                // retry it, unless the attempt limit is already reached.
                tracing::warn!(
                    attempts = event.attempts,
                    %error,
                    "catalog unavailable; leaving event for retry"
                );
                if event.attempts >= self.config.max_attempts {
                    return Outcome::Failed("home assistant unreachable repeatedly".to_owned());
                }
                return Outcome::Retry;
            }
        };
        let (call, entry_id) = match self.plan(event, &snapshot.catalog) {
            Ok(plan) => plan,
            Err(error) => {
                tracing::warn!(reason = %error, "control command rejected");
                return Outcome::Failed(error.to_string());
            }
        };

        if let Err(error) = self
            .ha
            .call_service(&call.domain, &call.service, &call.entity, call.data)
            .await
        {
            let status = error_status(&error);
            let permanent = error.kind() == ErrorKind::Permanent;
            let detail = ha_error_detail(&error);
            if permanent && !matches!(status, Some(401 | 403)) {
                tracing::error!(%error, entry = %entry_id, "home assistant rejected the call");
                return Outcome::Failed(format!(
                    "home assistant rejected the call: {}",
                    sanitize_detail(&detail)
                ));
            }
            if event.attempts >= self.config.max_attempts {
                tracing::error!(
                    %error,
                    entry = %entry_id,
                    attempts = event.attempts,
                    "home assistant call kept failing; failing the event"
                );
                return Outcome::Failed(format!(
                    "home assistant call failed repeatedly: {}",
                    sanitize_detail(&detail)
                ));
            }
            tracing::warn!(
                %error,
                entry = %entry_id,
                attempts = event.attempts,
                "home assistant call failed; leaving event for retry"
            );
            return Outcome::Retry;
        }
        tracing::info!(
            entry = %entry_id,
            service = %format!("{}.{}", call.domain, call.service),
            "home assistant accepted the command"
        );
        Outcome::Delivered(format!("home assistant accepted {entry_id}"))
    }
}

impl<H: ControlHa> Handler for Control<H> {
    async fn handle(&self, event: &Event) -> Outcome {
        if event.id.is_empty() {
            tracing::error!("scheduler returned an event without id; ignoring it");
            return Outcome::Retry;
        }
        self.run_event(event).await
    }
}

/// The task payload. `entry` and `action` are data; `brightness`, `kelvin`,
/// `hue` and `saturation` are the light adjustments, `text` is the speak
/// action's user content. The text is validated by `resolve` and never
/// logged, echoed in an ack detail or stored.
#[derive(Debug, Deserialize)]
struct Command {
    #[serde(default)]
    entry: String,
    #[serde(default)]
    action: String,
    #[serde(flatten)]
    params: Params,
}

/// A rejected control command.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PlanError {
    #[error("event queue {actual:?} does not match {expected:?}")]
    Queue { actual: String, expected: String },
    #[error("unsupported event kind={kind:?} task_type={task_type:?}")]
    Unsupported { kind: String, task_type: String },
    #[error("invalid command payload")]
    Payload,
    #[error("entry {entry:?} is not in the catalog")]
    Entry { entry: String },
    #[error("{0}")]
    Resolve(#[from] catalog::ResolveError),
}

fn source_label(source: Source) -> &'static str {
    match source {
        Source::Alarm => "alarm",
        Source::Reminder => "reminder",
        Source::Timer => "timer",
        Source::Scenario => "scenario",
        Source::Task => "task",
        Source::Unspecified => "",
        Source::Other(_) => "unknown",
    }
}

fn error_status(error: &home_ha::Error) -> Option<u16> {
    match error {
        home_ha::Error::Status { status, .. } => Some(*status),
        _ => None,
    }
}

/// Render an HA failure like `packages/go/httpapi.Error`: operation and status
/// only, never a response body.
fn ha_error_detail(error: &home_ha::Error) -> String {
    match error {
        home_ha::Error::Status { op, status, .. } => format!("{op}: status={status}"),
        home_ha::Error::Transport { op, .. } => format!("{op}: request failed"),
        home_ha::Error::Decode { op } => format!("{op}: invalid response"),
        home_ha::Error::Invalid { op, detail } => format!("{op}: {detail}"),
        home_ha::Error::Certificate { op } => format!("{op}: unusable CA"),
        home_ha::Error::WebSocket { op } => format!("{op}: websocket failed"),
    }
}

/// Collapse whitespace and bound the text by characters, never bytes.
fn detail(text: &str) -> String {
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate_chars(&joined, 200)
}

/// The ack detail: whitespace-collapsed and bounded like Go's 200-byte
/// `detail`, but on character boundaries. 200 characters stay below the
/// scheduler's 1024-byte ack-detail limit even when every character is
/// multi-byte.
fn sanitize_detail(text: &str) -> String {
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate_chars(&joined, 200)
}

fn truncate_chars(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let end = text
        .char_indices()
        .nth(limit)
        .map_or(text.len(), |(index, _)| index);
    text[..end].to_owned()
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use home_ha::Error;
    use home_worker::{Action, Event, Handler, Outcome, Source as EventSource};
    use serde_json::{Map, Value};

    use super::{Control, ControlHa, detail};
    use crate::catalog::{Catalog, Light, LightInput, Raw, Rules, build};
    use crate::control_config::ControlConfig;
    use crate::discovery::{CatalogSource, DiscoveryError, Snapshot};

    use std::sync::Arc;

    type Calls = Mutex<Vec<(String, String, String, Map<String, Value>)>>;

    #[derive(Default)]
    struct FakeHa {
        calls: Calls,
        status: Option<u16>,
    }

    impl ControlHa for FakeHa {
        async fn call_service(
            &self,
            domain: &str,
            service: &str,
            entity: &str,
            data: Map<String, Value>,
        ) -> Result<(), Error> {
            self.calls.lock().expect("lock").push((
                domain.to_owned(),
                service.to_owned(),
                entity.to_owned(),
                data,
            ));
            self.status.map_or(Ok(()), |status| {
                Err(Error::Status {
                    op: "call service",
                    status,
                    message: String::new(),
                })
            })
        }
    }

    struct FakeCatalog {
        snapshot: Result<Snapshot, DiscoveryError>,
    }

    impl CatalogSource for FakeCatalog {
        fn get(
            &self,
        ) -> std::pin::Pin<Box<dyn Future<Output = Result<Snapshot, DiscoveryError>> + Send + '_>>
        {
            Box::pin(async move { self.snapshot.clone() })
        }
    }

    fn catalog() -> Arc<Catalog> {
        Arc::new(Catalog::new(build(
            &[
                Raw {
                    entity: "switch.voice_profile".to_owned(),
                    state: "off".to_owned(),
                    name: "Voice profile".to_owned(),
                    profile: true,
                    ..Raw::default()
                },
                Raw {
                    entity: "scene.focus".to_owned(),
                    state: "x".to_owned(),
                    name: "Focus".to_owned(),
                    ..Raw::default()
                },
                Raw {
                    entity: "light.lamp".to_owned(),
                    state: "on".to_owned(),
                    name: "Lamp".to_owned(),
                    ..Raw::default()
                },
                Raw {
                    entity: crate::catalog::SPEAK_SCRIPT.to_owned(),
                    state: "off".to_owned(),
                    name: "Speak text".to_owned(),
                    ..Raw::default()
                },
                Raw {
                    entity: "assist_satellite.station_satellite".to_owned(),
                    state: "idle".to_owned(),
                    name: "Яндекс Станция Миди Assist satellite".to_owned(),
                    device_id: "station-1".to_owned(),
                    device: "Яндекс Станция Миди".to_owned(),
                    ..Raw::default()
                },
                Raw {
                    entity: "light.strip".to_owned(),
                    state: "on".to_owned(),
                    name: "Strip".to_owned(),
                    light: Some(
                        Light::new(LightInput {
                            color_modes: vec!["color_temp".to_owned()],
                            min_kelvin: 2700,
                            max_kelvin: 6500,
                            brightness: -1,
                            effects: vec!["Day".to_owned()],
                            ..LightInput::default()
                        })
                        .expect("light"),
                    ),
                    ..Raw::default()
                },
            ],
            &Rules {
                scenario_label: "scenario".to_owned(),
                profile_label: "voice_profile".to_owned(),
                exclude: Vec::new(),
            },
        )))
    }

    fn config() -> ControlConfig {
        ControlConfig::load_from(
            &|key| match key {
                "HA_URL" => Some("http://ha.example:8123".to_owned()),
                "HA_TOKEN" => Some("ha-secret".to_owned()),
                "SCHEDULER_ENDPOINT" => Some("http://scheduler.example:8098".to_owned()),
                "SCHEDULER_TOKEN" => Some("scheduler-secret".to_owned()),
                "CONTROL_TOKEN" => Some("control-secret".to_owned()),
                "CONTROL_CONSUMER_ID" => Some("ha-control-home".to_owned()),
                _ => None,
            },
            &|_| Ok(Vec::new()),
        )
        .expect("config")
    }

    fn control(ha: Arc<FakeHa>) -> Control<FakeHa> {
        Control::new(
            Arc::new(config()),
            Arc::new(FakeCatalog {
                snapshot: Ok(Snapshot {
                    catalog: catalog(),
                    states_ok: true,
                }),
            }),
            ha,
        )
    }

    fn task(id: &str, payload: &str) -> Event {
        Event {
            id: id.to_owned(),
            source_id: String::new(),
            task_type: "home.control".to_owned(),
            queue: "control".to_owned(),
            label: String::new(),
            payload: payload.to_owned(),
            action: Action::Fire,
            source: EventSource::Task,
            attempts: 1,
            due_unix: 0,
            occurrence_unix: 0,
            lease_until_unix: 0,
        }
    }

    #[tokio::test]
    async fn scenario_and_devices_resolve_to_services() {
        let ha = Arc::new(FakeHa::default());
        let control = control(Arc::clone(&ha));
        for (id, entry, payload) in [
            (
                "e1",
                "scene.focus",
                r#"{"entry":"scene.focus","action":"run"}"#,
            ),
            (
                "e2",
                "light.lamp",
                r#"{"entry":"light.lamp","action":"turn_off"}"#,
            ),
            (
                "e3",
                "switch.voice_profile",
                r#"{"entry":"switch.voice_profile","action":"turn_on"}"#,
            ),
        ] {
            let outcome = control.handle(&task(id, payload)).await;
            assert_eq!(
                outcome,
                Outcome::Delivered(format!("home assistant accepted {entry}"))
            );
        }
        let calls = ha.calls.lock().expect("lock");
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[0].0, "scene");
        assert_eq!(calls[0].1, "turn_on");
        assert_eq!(calls[2].2, "switch.voice_profile");
        assert_eq!(calls[2].1, "turn_on");
    }

    #[tokio::test]
    async fn light_payload_carries_effect_and_transition() {
        let ha = Arc::new(FakeHa::default());
        let control = control(Arc::clone(&ha));
        let outcome = control
            .handle(&task(
                "e1",
                r#"{"entry":"light.strip","action":"set","brightness":100,"kelvin":2816,"transition":1.5,"effect":"Day"}"#,
            ))
            .await;
        assert_eq!(
            outcome,
            Outcome::Delivered("home assistant accepted light.strip".to_owned())
        );
        let outcome = control
            .handle(&task(
                "e2",
                r#"{"entry":"light.strip","action":"turn_off","transition":2}"#,
            ))
            .await;
        assert_eq!(
            outcome,
            Outcome::Delivered("home assistant accepted light.strip".to_owned())
        );
        let calls = ha.calls.lock().expect("lock");
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].1, "turn_on");
        assert_eq!(calls[0].3["brightness_pct"], Value::from(100));
        assert_eq!(calls[0].3["color_temp_kelvin"], Value::from(2816));
        assert_eq!(calls[0].3["effect"], Value::from("Day"));
        assert_eq!(calls[0].3["transition"], Value::from(1.5));
        assert_eq!(calls[1].1, "turn_off");
        assert_eq!(calls[1].3["transition"], Value::from(2.0));
    }

    #[tokio::test]
    async fn bad_commands_fail_without_calling_ha() {
        let ha = Arc::new(FakeHa::default());
        let control = control(Arc::clone(&ha));
        for payload in [
            "nope",
            r#"{"entry":"script.alarm_stop","action":"run"}"#,
            r#"{"entry":"light.lamp","action":"run"}"#,
            r#"{"entry":"light.other","action":"turn_on"}"#,
            r#"{"entry":"switch.voice_profile","action":"set","brightness":10}"#,
            r#"{"entry":"light.strip","action":"set","effect":"Party"}"#,
            r#"{"entry":"light.strip","action":"turn_off","brightness":10}"#,
        ] {
            let outcome = control.handle(&task("e", payload)).await;
            assert!(
                matches!(outcome, Outcome::Failed(_)),
                "{payload}: {outcome:?}"
            );
        }
        assert!(ha.calls.lock().expect("lock").is_empty());
    }

    #[tokio::test]
    async fn speak_calls_the_script_with_the_text_as_a_variable() {
        let ha = Arc::new(FakeHa::default());
        let control = control(Arc::clone(&ha));
        let outcome = control
            .handle(&task(
                "e",
                r#"{"entry":"action.station_speak","action":"speak","text":"  Добрый вечер.  "}"#,
            ))
            .await;
        assert_eq!(
            outcome,
            Outcome::Delivered("home assistant accepted action.station_speak".to_owned())
        );
        let calls = ha.calls.lock().expect("lock");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "script");
        assert_eq!(calls[0].1, "turn_on");
        assert_eq!(calls[0].2, crate::catalog::SPEAK_SCRIPT);
        assert_eq!(
            calls[0].3["variables"]["message"],
            Value::from("Добрый вечер.")
        );
    }

    #[tokio::test]
    async fn invalid_speak_text_fails_without_calling_ha_or_leaking_the_text() {
        let ha = Arc::new(FakeHa::default());
        let control = control(Arc::clone(&ha));
        let secret = "очень секретный текст";
        for payload in [
            r#"{"entry":"action.station_speak","action":"speak"}"#.to_owned(),
            r#"{"entry":"action.station_speak","action":"speak","text":"   "}"#.to_owned(),
            format!(
                r#"{{"entry":"action.station_speak","action":"speak","text":"{secret}{}"}}"#,
                "ж".repeat(500)
            ),
            format!(
                r#"{{"entry":"action.station_speak","action":"speak","text":"{secret}\u0000"}}"#
            ),
        ] {
            let outcome = control.handle(&task("e", &payload)).await;
            match outcome {
                Outcome::Failed(detail) => {
                    assert!(!detail.contains(secret), "leaked text: {detail}");
                }
                other => panic!("{other:?}"),
            }
        }
        assert!(ha.calls.lock().expect("lock").is_empty());
    }

    #[tokio::test]
    async fn wrong_queue_and_kind_are_rejected() {
        let ha = Arc::new(FakeHa::default());
        let control = control(Arc::clone(&ha));
        let mut event = task("e", r#"{"entry":"scene.focus","action":"run"}"#);
        event.queue = "home".to_owned();
        assert_eq!(
            control.handle(&event).await,
            Outcome::Failed("event queue \"home\" does not match \"control\"".to_owned())
        );
        let mut event = task("e", "");
        event.source = EventSource::Alarm;
        assert!(matches!(control.handle(&event).await, Outcome::Failed(_)));
    }

    #[tokio::test]
    async fn catalog_outage_retries_then_fails() {
        let ha = Arc::new(FakeHa::default());
        let control = Control::new(
            Arc::new(config()),
            Arc::new(FakeCatalog {
                snapshot: Err(DiscoveryError::Unavailable),
            }),
            Arc::clone(&ha),
        );
        assert_eq!(control.handle(&task("e", "")).await, Outcome::Retry);
        let mut event = task("e", "");
        event.attempts = 10;
        assert_eq!(
            control.handle(&event).await,
            Outcome::Failed("home assistant unreachable repeatedly".to_owned())
        );
    }

    #[tokio::test]
    async fn ha_failure_classification_matches_the_go_executor() {
        let ha = Arc::new(FakeHa {
            status: Some(400),
            ..FakeHa::default()
        });
        let executor = control(Arc::clone(&ha));
        assert_eq!(
            executor
                .handle(&task("e", r#"{"entry":"scene.focus","action":"run"}"#))
                .await,
            Outcome::Failed(
                "home assistant rejected the call: call service: status=400".to_owned()
            )
        );

        let ha = Arc::new(FakeHa {
            status: Some(401),
            ..FakeHa::default()
        });
        let executor = control(Arc::clone(&ha));
        assert_eq!(
            executor
                .handle(&task("e", r#"{"entry":"scene.focus","action":"run"}"#))
                .await,
            Outcome::Retry
        );
        let mut event = task("e", r#"{"entry":"scene.focus","action":"run"}"#);
        event.attempts = 10;
        assert_eq!(
            executor.handle(&event).await,
            Outcome::Failed(
                "home assistant call failed repeatedly: call service: status=401".to_owned()
            )
        );
    }

    #[test]
    fn detail_is_collapsed_and_truncated_by_characters() {
        assert_eq!(detail("  a\n b  "), "a b");
        let text = "ж".repeat(500);
        let bounded = super::sanitize_detail(&text);
        assert_eq!(bounded.chars().count(), 200);
        assert!(bounded.chars().all(|character| character == 'ж'));
    }
}
