//! Outbox event model and stable dedupe identities.
//!
//! Event ids are UUIDv5 over the dedupe key, so a repeated tick, lease
//! redelivery or process restart produces the same id. Every key format from
//! the Go store is reproduced verbatim, including the literal `timer`
//! source-kind segment (`trigger:timer:<id>:<due>:0`).

use std::cmp::Ordering;

/// What produced an event.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum SourceKind {
    #[default]
    Unspecified = 0,
    Alarm = 1,
    Reminder = 2,
    Timer = 3,
    Scenario = 4,
    Task = 5,
}

impl SourceKind {
    /// Numeric wire/storage code (`source_kind`).
    #[must_use]
    pub const fn code(self) -> i32 {
        self as i32
    }

    /// Reconstructs a source kind from its numeric code.
    #[must_use]
    pub const fn from_code(code: i32) -> Option<Self> {
        match code {
            0 => Some(Self::Unspecified),
            1 => Some(Self::Alarm),
            2 => Some(Self::Reminder),
            3 => Some(Self::Timer),
            4 => Some(Self::Scenario),
            5 => Some(Self::Task),
            _ => None,
        }
    }
}

/// What an executor should do for an event.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum EventAction {
    /// Legacy rows: treated as [`EventAction::Fire`].
    #[default]
    Unspecified = 0,
    Fire = 1,
    Stop = 2,
}

impl EventAction {
    /// Numeric wire/storage code (`action`).
    #[must_use]
    pub const fn code(self) -> i32 {
        self as i32
    }

    /// Reconstructs an action from its numeric code.
    #[must_use]
    pub const fn from_code(code: i32) -> Option<Self> {
        match code {
            0 => Some(Self::Unspecified),
            1 => Some(Self::Fire),
            2 => Some(Self::Stop),
            _ => None,
        }
    }

    /// Resolves the legacy unspecified value to FIRE.
    #[must_use]
    pub const fn effective(self) -> Self {
        match self {
            Self::Unspecified => Self::Fire,
            other => other,
        }
    }
}

/// Durable outbox state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum EventState {
    #[default]
    Unspecified = 0,
    Pending = 1,
    Leased = 2,
    Delivered = 3,
    Failed = 4,
    Skipped = 5,
}

impl EventState {
    /// Numeric wire/storage code (`state`).
    #[must_use]
    pub const fn code(self) -> i32 {
        self as i32
    }

    /// Reconstructs a state from its numeric code.
    #[must_use]
    pub const fn from_code(code: i32) -> Option<Self> {
        match code {
            0 => Some(Self::Unspecified),
            1 => Some(Self::Pending),
            2 => Some(Self::Leased),
            3 => Some(Self::Delivered),
            4 => Some(Self::Failed),
            5 => Some(Self::Skipped),
            _ => None,
        }
    }

    /// Terminal states are never leased again.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Delivered | Self::Failed | Self::Skipped)
    }
}

/// Terminal outcome reported by an event consumer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum AckOutcome {
    #[default]
    Unspecified = 0,
    Delivered = 1,
    Failed = 2,
    Skipped = 3,
}

impl AckOutcome {
    /// Numeric wire/storage code (`ack_outcome`).
    #[must_use]
    pub const fn code(self) -> i32 {
        self as i32
    }

    /// Reconstructs an outcome from its numeric code.
    #[must_use]
    pub const fn from_code(code: i32) -> Option<Self> {
        match code {
            0 => Some(Self::Unspecified),
            1 => Some(Self::Delivered),
            2 => Some(Self::Failed),
            3 => Some(Self::Skipped),
            _ => None,
        }
    }

    /// Event state produced by this ack outcome.
    #[must_use]
    pub const fn terminal_state(self) -> EventState {
        match self {
            Self::Delivered => EventState::Delivered,
            Self::Failed => EventState::Failed,
            Self::Skipped | Self::Unspecified => EventState::Skipped,
        }
    }
}

/// An active delivery lease on a pending event.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lease {
    pub owner: String,
    pub until_unix: i64,
}

/// A durable outbox row.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Event {
    pub id: String,
    pub dedupe_key: String,
    pub source_id: String,
    pub source_kind: SourceKind,
    pub action: EventAction,
    pub due_unix: i64,
    pub occurrence_unix: i64,
    pub label: String,
    pub payload_json: String,
    pub state: EventState,
    pub lease_until_unix: i64,
    pub attempts: i32,
    pub snooze_seq: i64,
    pub detail: String,
    pub created_unix: i64,
    pub acked_unix: i64,
    pub ack_outcome: AckOutcome,
    pub task_type: String,
    pub queue: String,
}

impl Event {
    /// Whether the event reached a terminal state.
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        self.state.is_terminal()
    }

    /// Resolves a legacy unspecified action to FIRE.
    #[must_use]
    pub const fn effective_action(&self) -> EventAction {
        self.action.effective()
    }
}

/// Default task type of an event: `<source>.fire` or `<source>.stop`.
#[must_use]
pub fn default_task_type(source: SourceKind, action: EventAction) -> String {
    let prefix = match source {
        SourceKind::Alarm => "alarm",
        SourceKind::Reminder => "reminder",
        SourceKind::Timer => "timer",
        SourceKind::Scenario => "scenario",
        SourceKind::Task => "task",
        SourceKind::Unspecified => "event",
    };
    if action.effective() == EventAction::Stop {
        format!("{prefix}.stop")
    } else {
        format!("{prefix}.fire")
    }
}

/// Default queue of an event.
#[must_use]
pub fn default_queue(source: SourceKind) -> &'static str {
    match source {
        SourceKind::Task => "tasks",
        // Scenario execution is not implemented; keep its events out of the
        // home executor queue until an executor exists.
        SourceKind::Scenario => "scenario",
        _ => "home",
    }
}

/// Dedupe key of a regular schedule occurrence.
#[must_use]
pub fn schedule_trigger_key(
    source: SourceKind,
    id: &str,
    occurrence_unix: i64,
    snooze_seq: i64,
) -> String {
    format!(
        "trigger:{}:{id}:{occurrence_unix}:{snooze_seq}",
        source.code()
    )
}

/// Dedupe key of a timer occurrence. The source-kind segment is the literal
/// `timer`, not the numeric `3`; this is part of the Go on-disk contract.
#[must_use]
pub fn timer_trigger_key(id: &str, due_unix: i64) -> String {
    format!("trigger:timer:{id}:{due_unix}:0")
}

/// Dedupe key of a control STOP for a schedule occurrence.
#[must_use]
pub fn schedule_control_stop_key(id: &str, occurrence_unix: i64, snooze_seq: i64) -> String {
    format!("control:stop:{id}:{occurrence_unix}:{snooze_seq}")
}

/// Dedupe key of a control STOP for a timer.
#[must_use]
pub fn timer_control_stop_key(id: &str, due_unix: i64) -> String {
    format!("control:stop:timer:{id}:{due_unix}")
}

/// Dedupe key of the STOP emitted when a ring window expires.
#[must_use]
pub fn expiry_stop_key(source: SourceKind, id: &str, occurrence_unix: i64) -> String {
    format!("expiry:stop:{}:{id}:{occurrence_unix}", source.code())
}

/// Dedupe key of a generic task event.
#[must_use]
pub fn task_dedupe_key(task_id: &str) -> String {
    format!("task:{task_id}:0")
}

/// Dedupe key of a manual scenario run. `run_id` is unique per call unless
/// idempotency replays a stored event.
#[must_use]
pub fn scenario_manual_key(scenario_id: &str, run_id: &str) -> String {
    format!("scenario:manual:{scenario_id}:{run_id}")
}

/// Orders events exactly like `SELECT ... ORDER BY due_unix, id`.
pub fn order_events(events: &mut [Event]) {
    events.sort_by(compare_events);
}

/// Compares two events by due time and id, for deterministic merges.
#[must_use]
pub fn compare_events(left: &Event, right: &Event) -> Ordering {
    left.due_unix
        .cmp(&right.due_unix)
        .then_with(|| left.id.as_bytes().cmp(right.id.as_bytes()))
}
