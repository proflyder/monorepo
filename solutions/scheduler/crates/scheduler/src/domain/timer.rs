use crate::domain::hash::canonical_timer;

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
    #[must_use]
    pub const fn code(self) -> i32 {
        self as i32
    }

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

    #[must_use]
    pub fn lowercase_name(self) -> String {
        self.name().to_ascii_lowercase()
    }

    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Running | Self::Paused)
    }

    #[must_use]
    pub const fn is_finished(self) -> bool {
        matches!(self, Self::Fired | Self::Cancelled)
    }
}

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
    #[must_use]
    pub fn canonical(&self) -> String {
        canonical_timer(&self.label, self.duration_seconds)
    }
}
