//! Domain model, pure functions and golden-vector tests.

pub mod clock;
pub mod event;
pub mod hash;
pub mod id;
pub mod occurrence;
pub mod rule;
pub mod scenario;
pub mod schedule;
pub mod timer;
pub mod validation;

pub use clock::{Clock, FixedClock, SystemClock};
pub use event::{
    AckOutcome, Event, EventAction, EventState, Lease, SourceKind, default_queue,
    default_task_type, expiry_stop_key, order_events, scenario_manual_key,
    schedule_control_stop_key, schedule_trigger_key, task_dedupe_key, timer_control_stop_key,
    timer_trigger_key,
};
pub use hash::{canonical_scenario, canonical_schedule, canonical_task, canonical_timer, sha1_hex};
pub use id::{event_id, random_id, resource_id};
pub use occurrence::{DueClass, Occurrence, RingWindow, classify_due, missed_grace_detail};
pub use rule::{MAX_LOOKAHEAD_DAYS, Repeat, Rule, RuleError, WallTime, weekdays_text};
pub use scenario::{
    Scenario, ScenarioCondition, ScenarioDefinition, ScenarioStep, ScenarioTimeTrigger,
    ScenarioTrigger,
};
pub use schedule::{Schedule, ScheduleKind};
pub use timer::{Timer, TimerState};
pub use validation::{
    DomainError, ErrorKind, MAX_DETAIL_BYTES, MAX_IDEMPOTENCY_KEY_BYTES, MAX_LABEL_BYTES,
    MAX_TASK_DELAY_SECONDS, format_go_duration, validate_detail, validate_duration,
    validate_event_id, validate_id, validate_idempotency_key, validate_label, validate_payload,
    validate_queue, validate_reminder_repeat, validate_run_at, validate_schedule_kind,
    validate_task_type,
};

#[cfg(test)]
mod vectors;
