//! Map one allowed `(source kind, action)` pair to one configured script.
//!
//! The plan never derives a service or entity from the event payload. Anything
//! not in the allowlist is a structural failure that the bridge acknowledges
//! as terminal `FAILED`.

use home_worker::{Action, Event, Source};
use serde_json::{Map, Value, json};

use crate::domain::announce::announce_message;

/// A configured script and the variables passed to it, or a reason to skip HA.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedAction {
    /// Target script; empty when the event is skipped.
    pub script: String,
    pub variables: Map<String, Value>,
    /// Present when the event is acknowledged `SKIPPED` without an HA call.
    pub skip_reason: Option<String>,
}

/// A structural rejection. The exact text is part of the golden vectors.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlanError {
    #[error("event queue {queue:?} does not match the configured queue {configured:?}")]
    ForeignQueue { queue: String, configured: String },
    #[error(
        "generic task events are not Home Assistant actions (task_type={task_type:?}); the HA executor must not lease the \"tasks\" queue"
    )]
    GenericTask { task_type: String },
    #[error("unsupported event kind={kind:?} action={action:?}")]
    Unsupported { kind: String, action: String },
}

/// The script names the bridge is allowed to call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scripts {
    pub alarm: String,
    pub announce: String,
    pub stop: String,
}

/// Plan one event against the configured queue and scripts.
///
/// `configured_queue` is the queue the worker leases; an empty event queue is
/// accepted because the scheduler may omit the default value on the wire.
pub fn plan(
    event: &Event,
    configured_queue: &str,
    scripts: &Scripts,
) -> Result<PlannedAction, PlanError> {
    if !event.queue.is_empty() && event.queue != configured_queue {
        return Err(PlanError::ForeignQueue {
            queue: event.queue.clone(),
            configured: configured_queue.to_owned(),
        });
    }

    let mut base = Map::new();
    base.insert("event_id".to_owned(), json!(event.id));
    base.insert("source_id".to_owned(), json!(event.source_id));
    base.insert("label".to_owned(), json!(event.label));
    base.insert("kind".to_owned(), json!(source_kind_label(event.source)));

    match event.action {
        Action::Stop => match event.source {
            // The only sound this bridge can safely stop globally is the alarm
            // it started. A one-shot timer/reminder announcement has no safe
            // global HA stop.
            Source::Alarm => {
                let mut variables = clone_variables(&base);
                variables.insert("reason".to_owned(), json!("scheduler_stop"));
                Ok(PlannedAction {
                    script: scripts.stop.clone(),
                    variables,
                    skip_reason: None,
                })
            }
            Source::Timer | Source::Reminder => Ok(PlannedAction {
                script: String::new(),
                variables: Map::new(),
                skip_reason: Some(
                    "timer/reminder announcements are one-shot; no safe global HA stop exists"
                        .to_owned(),
                ),
            }),
            _ => Err(PlanError::Unsupported {
                kind: source_kind_label(event.source),
                action: action_label(event.action),
            }),
        },
        Action::Fire => match event.source {
            Source::Alarm => Ok(PlannedAction {
                script: scripts.alarm.clone(),
                variables: base,
                skip_reason: None,
            }),
            Source::Timer | Source::Reminder => {
                let mut variables = clone_variables(&base);
                variables.insert("message".to_owned(), json!(announce_message(event)));
                variables.insert("chime".to_owned(), json!(true));
                Ok(PlannedAction {
                    script: scripts.announce.clone(),
                    variables,
                    skip_reason: None,
                })
            }
            Source::Task => Err(PlanError::GenericTask {
                task_type: event.task_type.clone(),
            }),
            _ => Err(PlanError::Unsupported {
                kind: source_kind_label(event.source),
                action: action_label(event.action),
            }),
        },
        Action::Other(_) => Err(PlanError::Unsupported {
            kind: source_kind_label(event.source),
            action: action_label(event.action),
        }),
    }
}

/// The Go bridge's `SourceKind` string. An unknown wire value was normalized
/// by the client to `enum-<n>`.
#[must_use]
pub fn source_kind_label(source: Source) -> String {
    match source {
        Source::Unspecified => String::new(),
        Source::Alarm => "alarm".to_owned(),
        Source::Reminder => "reminder".to_owned(),
        Source::Timer => "timer".to_owned(),
        Source::Scenario => "scenario".to_owned(),
        Source::Task => "task".to_owned(),
        Source::Other(value) => format!("enum-{value}"),
    }
}

/// The Go bridge's `Action` string. Unknown wire values stay `ENUM-<n>`.
#[must_use]
pub fn action_label(action: Action) -> String {
    match action {
        Action::Fire => "FIRE".to_owned(),
        Action::Stop => "STOP".to_owned(),
        Action::Other(value) => format!("ENUM-{value}"),
    }
}

fn clone_variables(base: &Map<String, Value>) -> Map<String, Value> {
    base.clone()
}

#[cfg(test)]
mod tests {
    use home_worker::{Action, Event, Source};
    use serde_json::json;

    use super::{PlanError, Scripts, plan};

    fn scripts() -> Scripts {
        Scripts {
            alarm: "alarm_start".to_owned(),
            announce: "schedule_announce".to_owned(),
            stop: "alarm_stop".to_owned(),
        }
    }

    fn event(action: Action, source: Source) -> Event {
        Event {
            id: "evt-1".to_owned(),
            source_id: "alarm-1".to_owned(),
            task_type: String::new(),
            queue: "home".to_owned(),
            label: "Wake up".to_owned(),
            payload: String::new(),
            action,
            source,
            attempts: 0,
            due_unix: 0,
            occurrence_unix: 0,
            lease_until_unix: 0,
        }
    }

    #[test]
    fn alarm_fire_uses_the_alarm_script() {
        let planned = plan(&event(Action::Fire, Source::Alarm), "home", &scripts()).expect("plan");
        assert_eq!(planned.script, "alarm_start");
        assert_eq!(planned.variables["kind"], json!("alarm"));
        assert_eq!(planned.variables["label"], json!("Wake up"));
    }

    #[test]
    fn timer_fire_announces_with_chime() {
        let mut timer = event(Action::Fire, Source::Timer);
        timer.label.clear();
        let planned = plan(&timer, "home", &scripts()).expect("plan");
        assert_eq!(planned.script, "schedule_announce");
        assert_eq!(planned.variables["message"], json!("Timer"));
        assert_eq!(planned.variables["chime"], json!(true));
    }

    #[test]
    fn stop_is_only_planned_for_alarms() {
        let planned = plan(&event(Action::Stop, Source::Alarm), "home", &scripts()).expect("plan");
        assert_eq!(planned.script, "alarm_stop");
        assert_eq!(planned.variables["reason"], json!("scheduler_stop"));

        let planned = plan(&event(Action::Stop, Source::Timer), "home", &scripts()).expect("plan");
        assert!(planned.script.is_empty());
        assert!(planned.skip_reason.is_some());
    }

    #[test]
    fn structural_events_are_rejected() {
        let mut task = event(Action::Fire, Source::Task);
        task.task_type = "generic.ping".to_owned();
        assert!(matches!(
            plan(&task, "home", &scripts()),
            Err(PlanError::GenericTask { .. })
        ));
        assert!(matches!(
            plan(&event(Action::Fire, Source::Scenario), "home", &scripts()),
            Err(PlanError::Unsupported { .. })
        ));
        let mut foreign = event(Action::Fire, Source::Alarm);
        foreign.queue = "tasks".to_owned();
        assert!(matches!(
            plan(&foreign, "home", &scripts()),
            Err(PlanError::ForeignQueue { .. })
        ));
    }
}
