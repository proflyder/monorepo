use home_worker::{Action, Event, Source};
use serde_json::{Map, Value, json};

use crate::domain::announce::announce_message;

#[derive(Debug, Clone, PartialEq)]
pub struct PlannedAction {
    pub script: String,
    pub variables: Map<String, Value>,
    pub skip_reason: Option<String>,
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scripts {
    pub alarm: String,
    pub announce: String,
    pub stop: String,
}

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
