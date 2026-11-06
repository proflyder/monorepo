//! Conversions between the generated protobuf messages and the domain model.
//!
//! Nothing outside this module names both types: the store never sees protobuf
//! and the domain never sees wire enums.

use home_proto::v1 as proto;

use crate::domain::{
    AckOutcome, Event, EventAction, EventState, Repeat, Schedule, ScheduleKind, SourceKind, Timer,
    TimerState, WallTime,
};

pub fn schedule_kind(code: i32) -> ScheduleKind {
    ScheduleKind::from_code(code).unwrap_or(ScheduleKind::Unspecified)
}

pub fn repeat_kind(code: i32) -> Repeat {
    Repeat::from_code(code).unwrap_or(Repeat::Unspecified)
}

pub fn ack_outcome(code: i32) -> AckOutcome {
    AckOutcome::from_code(code).unwrap_or(AckOutcome::Unspecified)
}

pub fn proto_schedule_kind(kind: ScheduleKind) -> proto::ScheduleKind {
    match kind {
        ScheduleKind::Alarm => proto::ScheduleKind::SCHEDULE_KIND_ALARM,
        ScheduleKind::Reminder => proto::ScheduleKind::SCHEDULE_KIND_REMINDER,
        ScheduleKind::Scenario => proto::ScheduleKind::SCHEDULE_KIND_SCENARIO,
        ScheduleKind::Unspecified => proto::ScheduleKind::SCHEDULE_KIND_UNSPECIFIED,
    }
}

pub fn proto_repeat(repeat: Repeat) -> proto::RepeatKind {
    match repeat {
        Repeat::Once => proto::RepeatKind::REPEAT_KIND_ONCE,
        Repeat::Daily => proto::RepeatKind::REPEAT_KIND_DAILY,
        Repeat::Weekdays => proto::RepeatKind::REPEAT_KIND_WEEKDAYS,
        Repeat::Weekends => proto::RepeatKind::REPEAT_KIND_WEEKENDS,
        Repeat::Custom => proto::RepeatKind::REPEAT_KIND_CUSTOM,
        Repeat::Unspecified => proto::RepeatKind::REPEAT_KIND_UNSPECIFIED,
    }
}

pub fn proto_timer_state(state: TimerState) -> proto::TimerState {
    match state {
        TimerState::Running => proto::TimerState::TIMER_STATE_RUNNING,
        TimerState::Paused => proto::TimerState::TIMER_STATE_PAUSED,
        TimerState::Fired => proto::TimerState::TIMER_STATE_FIRED,
        TimerState::Cancelled => proto::TimerState::TIMER_STATE_CANCELLED,
        TimerState::Unspecified => proto::TimerState::TIMER_STATE_UNSPECIFIED,
    }
}

pub fn proto_source_kind(kind: SourceKind) -> proto::SourceKind {
    match kind {
        SourceKind::Alarm => proto::SourceKind::SOURCE_KIND_ALARM,
        SourceKind::Reminder => proto::SourceKind::SOURCE_KIND_REMINDER,
        SourceKind::Timer => proto::SourceKind::SOURCE_KIND_TIMER,
        SourceKind::Scenario => proto::SourceKind::SOURCE_KIND_SCENARIO,
        SourceKind::Task => proto::SourceKind::SOURCE_KIND_TASK,
        SourceKind::Unspecified => proto::SourceKind::SOURCE_KIND_UNSPECIFIED,
    }
}

pub fn proto_event_action(action: EventAction) -> proto::EventAction {
    match action {
        EventAction::Fire => proto::EventAction::EVENT_ACTION_FIRE,
        EventAction::Stop => proto::EventAction::EVENT_ACTION_STOP,
        EventAction::Unspecified => proto::EventAction::EVENT_ACTION_UNSPECIFIED,
    }
}

pub fn proto_event_state(state: EventState) -> proto::EventState {
    match state {
        EventState::Pending => proto::EventState::EVENT_STATE_PENDING,
        EventState::Leased => proto::EventState::EVENT_STATE_LEASED,
        EventState::Delivered => proto::EventState::EVENT_STATE_DELIVERED,
        EventState::Failed => proto::EventState::EVENT_STATE_FAILED,
        EventState::Skipped => proto::EventState::EVENT_STATE_SKIPPED,
        EventState::Unspecified => proto::EventState::EVENT_STATE_UNSPECIFIED,
    }
}

pub fn proto_ack_outcome(outcome: AckOutcome) -> proto::AckOutcome {
    match outcome {
        AckOutcome::Delivered => proto::AckOutcome::ACK_OUTCOME_DELIVERED,
        AckOutcome::Failed => proto::AckOutcome::ACK_OUTCOME_FAILED,
        AckOutcome::Skipped => proto::AckOutcome::ACK_OUTCOME_SKIPPED,
        AckOutcome::Unspecified => proto::AckOutcome::ACK_OUTCOME_UNSPECIFIED,
    }
}

pub fn schedule_to_proto(schedule: &Schedule) -> proto::Schedule {
    proto::Schedule {
        id: schedule.id.clone(),
        label: schedule.label.clone(),
        kind: proto_schedule_kind(schedule.kind).into(),
        timezone: schedule.timezone.clone(),
        at: proto::WallTime {
            hour: schedule.at.hour,
            minute: schedule.at.minute,
            second: schedule.at.second,
            ..proto::WallTime::default()
        }
        .into(),
        once_date: schedule.once_date.clone(),
        repeat: proto_repeat(schedule.repeat).into(),
        weekdays: schedule.weekdays.clone(),
        next_due_unix: schedule.next_due_unix,
        enabled: schedule.enabled,
        snooze_until_unix: schedule.snooze_until_unix,
        last_fired_unix: schedule.last_fired_unix,
        snooze_count: schedule.snooze_count,
        revision: schedule.revision,
        created_unix: schedule.created_unix,
        updated_unix: schedule.updated_unix,
        payload_json: schedule.payload_json.clone(),
        active_occurrence_unix: schedule.active_occurrence_unix,
        active_until_unix: schedule.active_until_unix,
        ..proto::Schedule::default()
    }
}

pub fn timer_to_proto(timer: &Timer) -> proto::Timer {
    proto::Timer {
        id: timer.id.clone(),
        label: timer.label.clone(),
        duration_seconds: timer.duration_seconds,
        remaining_seconds: timer.remaining_seconds,
        due_unix: timer.due_unix,
        state: proto_timer_state(timer.state).into(),
        revision: timer.revision,
        created_unix: timer.created_unix,
        updated_unix: timer.updated_unix,
        ..proto::Timer::default()
    }
}

pub fn event_to_proto(event: &Event) -> proto::DueEvent {
    proto::DueEvent {
        id: event.id.clone(),
        source_id: event.source_id.clone(),
        source_kind: proto_source_kind(event.source_kind).into(),
        due_unix: event.due_unix,
        occurrence_unix: event.occurrence_unix,
        label: event.label.clone(),
        payload_json: event.payload_json.clone(),
        state: proto_event_state(event.state).into(),
        attempts: event.attempts,
        snooze_seq: i32::try_from(event.snooze_seq).unwrap_or(i32::MAX),
        created_unix: event.created_unix,
        lease_until_unix: event.lease_until_unix,
        detail: event.detail.clone(),
        action: proto_event_action(event.action).into(),
        ack_outcome: proto_ack_outcome(event.ack_outcome).into(),
        task_type: event.task_type.clone(),
        queue: event.queue.clone(),
        acked_unix: event.acked_unix,
        ..proto::DueEvent::default()
    }
}

pub fn view_to_schedule(view: &proto::Schedule) -> Schedule {
    Schedule {
        id: view.id.to_owned(),
        label: view.label.to_owned(),
        kind: schedule_kind(view.kind.to_i32()),
        timezone: view.timezone.to_owned(),
        at: view
            .at
            .as_option()
            .map_or_else(WallTime::default, |at| WallTime {
                hour: at.hour,
                minute: at.minute,
                second: at.second,
            }),
        once_date: view.once_date.to_owned(),
        repeat: repeat_kind(view.repeat.to_i32()),
        weekdays: view.weekdays.to_vec(),
        next_due_unix: view.next_due_unix,
        enabled: view.enabled,
        payload_json: view.payload_json.to_owned(),
        revision: view.revision,
        created_unix: view.created_unix,
        updated_unix: view.updated_unix,
        snooze_until_unix: view.snooze_until_unix,
        last_fired_unix: view.last_fired_unix,
        snooze_count: view.snooze_count,
        active_occurrence_unix: view.active_occurrence_unix,
        active_until_unix: view.active_until_unix,
        ..Schedule::default()
    }
}

pub fn create_view_to_schedule(view: &proto::CreateScheduleRequest) -> Schedule {
    Schedule {
        label: view.label.to_owned(),
        kind: schedule_kind(view.kind.to_i32()),
        timezone: view.timezone.to_owned(),
        at: view
            .at
            .as_option()
            .map_or_else(WallTime::default, |at| WallTime {
                hour: at.hour,
                minute: at.minute,
                second: at.second,
            }),
        once_date: view.once_date.to_owned(),
        repeat: repeat_kind(view.repeat.to_i32()),
        weekdays: view.weekdays.to_vec(),
        enabled: view.enabled,
        payload_json: view.payload_json.to_owned(),
        ..Schedule::default()
    }
}

pub fn alarm_to_schedule(view: &proto::Alarm) -> Result<Schedule, &'static str> {
    let repeat = match view.repeat.to_i32() {
        1 => Repeat::Once,
        2 => Repeat::Daily,
        3 => Repeat::Weekdays,
        4 => Repeat::Weekends,
        5 => Repeat::Custom,
        _ => return Err("unsupported repeat"),
    };
    Ok(Schedule {
        id: view.id.to_owned(),
        label: view.label.to_owned(),
        kind: ScheduleKind::Alarm,
        timezone: view.timezone.to_owned(),
        at: WallTime {
            hour: view.hour,
            minute: view.minute,
            second: 0,
        },
        repeat,
        weekdays: view.weekdays.to_vec(),
        enabled: view.enabled,
        ..Schedule::default()
    })
}

pub fn schedule_to_alarm(schedule: &Schedule) -> proto::Alarm {
    proto::Alarm {
        id: schedule.id.clone(),
        label: schedule.label.clone(),
        hour: schedule.at.hour,
        minute: schedule.at.minute,
        timezone: schedule.timezone.clone(),
        repeat: match schedule.repeat {
            Repeat::Once => proto::Repeat::ONCE,
            Repeat::Daily => proto::Repeat::DAILY,
            Repeat::Weekdays => proto::Repeat::WEEKDAYS,
            Repeat::Weekends => proto::Repeat::WEEKENDS,
            Repeat::Custom => proto::Repeat::CUSTOM,
            Repeat::Unspecified => proto::Repeat::REPEAT_UNSPECIFIED,
        }
        .into(),
        weekdays: schedule.weekdays.clone(),
        enabled: schedule.enabled,
        next_run_unix: schedule.next_due_unix,
        snooze_until_unix: schedule.snooze_until_unix,
        last_ring_unix: schedule.last_fired_unix,
        revision: schedule.revision,
        ..proto::Alarm::default()
    }
}
