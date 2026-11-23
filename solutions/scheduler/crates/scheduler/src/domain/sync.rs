use super::schedule::ScheduleKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SyncEntity {
    Alarms,
    Reminders,
    Timers,
    Scenarios,
    Notifications,
}

impl SyncEntity {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Alarms => "alarms",
            Self::Reminders => "reminders",
            Self::Timers => "timers",
            Self::Scenarios => "scenarios",
            Self::Notifications => "notifications",
        }
    }

    #[must_use]
    pub const fn of_schedule(kind: ScheduleKind) -> Option<Self> {
        match kind {
            ScheduleKind::Alarm => Some(Self::Alarms),
            ScheduleKind::Reminder => Some(Self::Reminders),
            ScheduleKind::Unspecified | ScheduleKind::Scenario => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SyncOp {
    Upsert,
    Delete,
}

impl SyncOp {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Upsert => "upsert",
            Self::Delete => "delete",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SyncChange {
    pub entity: SyncEntity,
    pub op: SyncOp,
    pub id: String,
}

impl SyncChange {
    #[must_use]
    pub fn upsert(entity: SyncEntity, id: impl Into<String>) -> Self {
        Self {
            entity,
            op: SyncOp::Upsert,
            id: id.into(),
        }
    }

    #[must_use]
    pub fn delete(entity: SyncEntity, id: impl Into<String>) -> Self {
        Self {
            entity,
            op: SyncOp::Delete,
            id: id.into(),
        }
    }

    #[must_use]
    pub fn notification(op: SyncOp, id: impl Into<String>) -> Self {
        Self {
            entity: SyncEntity::Notifications,
            op,
            id: id.into(),
        }
    }

    #[must_use]
    pub fn schedule(kind: ScheduleKind, op: SyncOp, id: &str) -> Option<Self> {
        SyncEntity::of_schedule(kind).map(|entity| Self {
            entity,
            op,
            id: id.to_owned(),
        })
    }
}
