//! Durable countdown timers.

use crate::domain::hash::canonical_timer;

/// Durable state of a countdown timer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum TimerState {
    #[default]
    Unspecified = 0,
    Running = 1,
    Paused = 2,
    Fired = 3,
    Cancelled = 4,
}

impl TimerState {
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
            1 => Some(Self::Running),
            2 => Some(Self::Paused),
            3 => Some(Self::Fired),
            4 => Some(Self::Cancelled),
            _ => None,
        }
    }

    /// Protobuf enum name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Unspecified => "TIMER_STATE_UNSPECIFIED",
            Self::Running => "TIMER_STATE_RUNNING",
            Self::Paused => "TIMER_STATE_PAUSED",
            Self::Fired => "TIMER_STATE_FIRED",
            Self::Cancelled => "TIMER_STATE_CANCELLED",
        }
    }

    /// Lower-case enum name, as used by Go's `strings.ToLower(state.String())`
    /// in precondition errors (`timer is timer_state_running`).
    #[must_use]
    pub fn lowercase_name(self) -> String {
        self.name().to_ascii_lowercase()
    }

    /// Whether the timer is still counting or paused.
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Running | Self::Paused)
    }

    /// Whether the timer reached a terminal state.
    #[must_use]
    pub const fn is_finished(self) -> bool {
        matches!(self, Self::Fired | Self::Cancelled)
    }
}

/// A timer row.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Timer {
    pub id: String,
    pub label: String,
    pub duration_seconds: i64,
    pub remaining_seconds: i64,
    pub due_unix: i64,
    pub state: TimerState,
    pub revision: i64,
    pub created_unix: i64,
    pub updated_unix: i64,
}

impl Timer {
    /// Canonical request string used for idempotency conflict detection.
    #[must_use]
    pub fn canonical(&self) -> String {
        canonical_timer(&self.label, self.duration_seconds)
    }
}
