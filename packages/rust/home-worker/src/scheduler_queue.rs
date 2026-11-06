//! `Queue` implementation over the `home-proto` scheduler client.

use std::future::Future;

use buffa::Enumeration;
use home_proto::SchedulerClient;
use home_proto::v1::{AckOutcome, DueEvent, EventAction, ListDueEventsRequest, SourceKind};

use super::{Ack, AckOutcome as Outcome, Action, Event, LeaseRequest, Queue, QueueError, Source};

/// Durable queue backed by `ricardo.v1.SchedulerService`.
#[derive(Debug, Clone)]
pub struct SchedulerQueue {
    client: SchedulerClient,
}

impl SchedulerQueue {
    #[must_use]
    pub fn new(client: SchedulerClient) -> Self {
        Self { client }
    }
}

impl Queue for SchedulerQueue {
    fn lease(
        &self,
        request: LeaseRequest,
    ) -> impl Future<Output = Result<Vec<Event>, QueueError>> + Send {
        let client = self.client.clone();
        async move {
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
        let client = self.client.clone();
        async move {
            client
                .ack_event(
                    &ack.event_id,
                    &ack.consumer_id,
                    outcome_of(ack.outcome),
                    &ack.detail,
                )
                .await?;
            Ok(())
        }
    }
}

fn outcome_of(outcome: Outcome) -> AckOutcome {
    match outcome {
        Outcome::Delivered => AckOutcome::ACK_OUTCOME_DELIVERED,
        Outcome::Failed => AckOutcome::ACK_OUTCOME_FAILED,
        Outcome::Skipped => AckOutcome::ACK_OUTCOME_SKIPPED,
    }
}

impl From<DueEvent> for Event {
    fn from(event: DueEvent) -> Self {
        Self {
            id: event.id,
            source_id: event.source_id,
            task_type: event.task_type,
            queue: event.queue,
            label: event.label,
            payload: event.payload_json,
            action: action_of(&event.action),
            source: source_of(event.source_kind.to_i32()),
            attempts: event.attempts,
            due_unix: event.due_unix,
            occurrence_unix: event.occurrence_unix,
            lease_until_unix: event.lease_until_unix,
        }
    }
}

fn action_of(action: &buffa::EnumValue<EventAction>) -> Action {
    match action.as_known() {
        Some(EventAction::EVENT_ACTION_STOP) => Action::Stop,
        // Legacy rows carry UNSPECIFIED; the proto contract says FIRE.
        Some(EventAction::EVENT_ACTION_UNSPECIFIED | EventAction::EVENT_ACTION_FIRE) => {
            Action::Fire
        }
        // An unknown wire value must not be guessed as FIRE: the handler
        // decides whether it can act on it.
        None => Action::Other(action.to_i32()),
    }
}

fn source_of(value: i32) -> Source {
    match value {
        value if value == SourceKind::SOURCE_KIND_ALARM.to_i32() => Source::Alarm,
        value if value == SourceKind::SOURCE_KIND_REMINDER.to_i32() => Source::Reminder,
        value if value == SourceKind::SOURCE_KIND_TIMER.to_i32() => Source::Timer,
        value if value == SourceKind::SOURCE_KIND_SCENARIO.to_i32() => Source::Scenario,
        value if value == SourceKind::SOURCE_KIND_TASK.to_i32() => Source::Task,
        value if value == SourceKind::SOURCE_KIND_UNSPECIFIED.to_i32() => Source::Unspecified,
        other => Source::Other(other),
    }
}

#[cfg(test)]
mod tests {
    use buffa::EnumValue;
    use buffa::Enumeration;
    use home_proto::v1::{DueEvent, EventAction, SourceKind};

    use super::{Action, Event, Source};
    use crate::AckOutcome;

    #[test]
    fn due_event_maps_all_executor_fields() {
        let event = Event::from(DueEvent {
            id: "ev_1".to_owned(),
            source_id: "alarm-1".to_owned(),
            task_type: "alarm.fire".to_owned(),
            queue: "home".to_owned(),
            payload_json: "{\"label\":\"wake\"}".to_owned(),
            action: EnumValue::Known(EventAction::EVENT_ACTION_STOP),
            source_kind: EnumValue::Known(SourceKind::SOURCE_KIND_ALARM),
            attempts: 2,
            due_unix: 10,
            occurrence_unix: 9,
            lease_until_unix: 20,
            ..Default::default()
        });
        assert_eq!(event.id, "ev_1");
        assert_eq!(event.source_id, "alarm-1");
        assert_eq!(event.task_type, "alarm.fire");
        assert_eq!(event.queue, "home");
        assert_eq!(event.action, Action::Stop);
        assert_eq!(event.source, Source::Alarm);
        assert_eq!(event.attempts, 2);
        assert_eq!(event.lease_until_unix, 20);
    }

    #[test]
    fn legacy_unspecified_action_is_fire() {
        let event = Event::from(DueEvent {
            action: EnumValue::Known(EventAction::EVENT_ACTION_UNSPECIFIED),
            ..Default::default()
        });
        assert_eq!(event.action, Action::Fire);
    }

    #[test]
    fn unknown_action_and_source_are_preserved() {
        let event = Event::from(DueEvent {
            action: EnumValue::Unknown(99),
            source_kind: EnumValue::Unknown(77),
            ..Default::default()
        });
        assert_eq!(event.action, Action::Other(99));
        assert_eq!(event.source, Source::Other(77));
    }

    #[test]
    fn ack_outcomes_match_the_proto_names() {
        assert_eq!(
            super::outcome_of(AckOutcome::Delivered).to_i32(),
            home_proto::v1::AckOutcome::ACK_OUTCOME_DELIVERED.to_i32()
        );
        assert_eq!(
            super::outcome_of(AckOutcome::Skipped).to_i32(),
            home_proto::v1::AckOutcome::ACK_OUTCOME_SKIPPED.to_i32()
        );
    }
}
