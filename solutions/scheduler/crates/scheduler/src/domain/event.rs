use std::cmp::Ordering;

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
    #[must_use]
    pub const fn code(self) -> i32 {
        self as i32
    }

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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum EventAction {
    #[default]
    Unspecified = 0,
    Fire = 1,
    Stop = 2,
}

impl EventAction {
    #[must_use]
    pub const fn code(self) -> i32 {
        self as i32
    }

    #[must_use]
    pub const fn from_code(code: i32) -> Option<Self> {
        match code {
            0 => Some(Self::Unspecified),
            1 => Some(Self::Fire),
            2 => Some(Self::Stop),
            _ => None,
        }
    }

    #[must_use]
    pub const fn effective(self) -> Self {
        match self {
            Self::Unspecified => Self::Fire,
            other => other,
        }
    }
}

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
    #[must_use]
    pub const fn code(self) -> i32 {
        self as i32
    }

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

    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Delivered | Self::Failed | Self::Skipped)
    }
}

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
    #[must_use]
    pub const fn code(self) -> i32 {
        self as i32
    }

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

    #[must_use]
    pub const fn terminal_state(self) -> EventState {
        match self {
            Self::Delivered => EventState::Delivered,
            Self::Failed => EventState::Failed,
            Self::Skipped | Self::Unspecified => EventState::Skipped,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lease {
    pub owner: String,
    pub until_unix: i64,
}

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
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        self.state.is_terminal()
    }

    #[must_use]
    pub const fn effective_action(&self) -> EventAction {
        self.action.effective()
    }
}

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

#[must_use]
pub fn default_queue(source: SourceKind) -> &'static str {
    match source {
        SourceKind::Task => "tasks",
        SourceKind::Scenario => "scenario",
        _ => "home",
    }
}

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

#[must_use]
pub fn timer_trigger_key(id: &str, due_unix: i64) -> String {
    format!("trigger:timer:{id}:{due_unix}:0")
}

#[must_use]
pub fn schedule_control_stop_key(id: &str, occurrence_unix: i64, snooze_seq: i64) -> String {
    format!("control:stop:{id}:{occurrence_unix}:{snooze_seq}")
}

#[must_use]
pub fn timer_control_stop_key(id: &str, due_unix: i64) -> String {
    format!("control:stop:timer:{id}:{due_unix}")
}

#[must_use]
pub fn expiry_stop_key(source: SourceKind, id: &str, occurrence_unix: i64) -> String {
    format!("expiry:stop:{}:{id}:{occurrence_unix}", source.code())
}

#[must_use]
pub fn task_dedupe_key(task_id: &str) -> String {
    format!("task:{task_id}:0")
}

#[must_use]
pub fn scenario_manual_key(scenario_id: &str, run_id: &str) -> String {
    format!("scenario:manual:{scenario_id}:{run_id}")
}

pub fn order_events(events: &mut [Event]) {
    events.sort_by(compare_events);
}

#[must_use]
pub fn compare_events(left: &Event, right: &Event) -> Ordering {
    left.due_unix
        .cmp(&right.due_unix)
        .then_with(|| left.id.as_bytes().cmp(right.id.as_bytes()))
}
