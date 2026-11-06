//! Durable schedule rows.

use crate::domain::event::SourceKind;
use crate::domain::hash::canonical_schedule;
use crate::domain::occurrence::{Occurrence, RingWindow};
use crate::domain::rule::{Repeat, Rule, WallTime};

/// What a schedule produces when it fires.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum ScheduleKind {
    #[default]
    Unspecified = 0,
    Alarm = 1,
    Reminder = 2,
    Scenario = 3,
}

impl ScheduleKind {
    /// Numeric wire/storage code (`kind`).
    #[must_use]
    pub const fn code(self) -> i32 {
        self as i32
    }

    /// Reconstructs a kind from its numeric code.
    #[must_use]
    pub const fn from_code(code: i32) -> Option<Self> {
        match code {
            0 => Some(Self::Unspecified),
            1 => Some(Self::Alarm),
            2 => Some(Self::Reminder),
            3 => Some(Self::Scenario),
            _ => None,
        }
    }

    /// Event source kind that this schedule produces.
    #[must_use]
    pub const fn source_kind(self) -> SourceKind {
        match self {
            Self::Alarm => SourceKind::Alarm,
            Self::Reminder => SourceKind::Reminder,
            Self::Scenario => SourceKind::Scenario,
            Self::Unspecified => SourceKind::Unspecified,
        }
    }
}

/// A schedule row, including the server-derived ring-window columns.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Schedule {
    pub id: String,
    pub label: String,
    pub kind: ScheduleKind,
    pub timezone: String,
    pub at: WallTime,
    pub once_date: String,
    pub repeat: Repeat,
    pub weekdays: Vec<i32>,
    pub next_due_unix: i64,
    pub enabled: bool,
    pub payload_json: String,
    pub revision: i64,
    pub created_unix: i64,
    pub updated_unix: i64,
    pub deleted_unix: i64,
    pub last_fired_unix: i64,
    pub snooze_until_unix: i64,
    pub snooze_count: i64,
    pub active_occurrence_unix: i64,
    pub active_until_unix: i64,
}

impl Schedule {
    /// The recurrence rule described by the wall-time columns.
    #[must_use]
    pub fn rule(&self) -> Rule {
        Rule {
            timezone: self.timezone.clone(),
            at: self.at,
            repeat: self.repeat,
            weekdays: self.weekdays.clone(),
            once_date: self.once_date.clone(),
        }
    }

    /// Persisted ring window of this row.
    #[must_use]
    pub const fn ring_window(&self) -> RingWindow {
        RingWindow {
            active_occurrence_unix: self.active_occurrence_unix,
            active_until_unix: self.active_until_unix,
            snooze_until_unix: self.snooze_until_unix,
        }
    }

    /// Occurrence eligible for stop/snooze at `now_unix`.
    #[must_use]
    pub fn active_occurrence(&self, now_unix: i64) -> Option<Occurrence> {
        self.ring_window().active_occurrence(now_unix)
    }

    /// Occurrence a rule change must close at `now_unix`.
    #[must_use]
    pub fn occurrence_to_close(&self, now_unix: i64) -> Option<Occurrence> {
        self.ring_window().occurrence_to_close(now_unix)
    }

    /// Canonical request string used for idempotency conflict detection.
    #[must_use]
    pub fn canonical(&self) -> String {
        canonical_schedule(
            &self.label,
            self.kind,
            &self.timezone,
            &self.once_date,
            self.at,
            self.repeat,
            &self.weekdays,
            self.enabled,
            &self.payload_json,
        )
    }
}
