//! Occurrence math: the ringing window, missed-grace classification and
//! snooze eligibility.
//!
//! The former Go store kept these decisions in `tick.go` and `schedule.go`;
//! here they are pure so the tick transaction can stay a thin adapter.

/// A due occurrence identified by its Unix second.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Occurrence(i64);

impl Occurrence {
    /// Wraps a Unix second. Storage uses `0` for "no occurrence".
    #[must_use]
    pub const fn new(unix: i64) -> Self {
        Self(unix)
    }

    /// Unix second of the occurrence.
    #[must_use]
    pub const fn unix(self) -> i64 {
        self.0
    }

    /// `false` for the persisted `0` sentinel.
    #[must_use]
    pub const fn is_set(self) -> bool {
        self.0 > 0
    }
}

/// Persisted ringing window of a schedule row.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RingWindow {
    pub active_occurrence_unix: i64,
    pub active_until_unix: i64,
    pub snooze_until_unix: i64,
}

impl RingWindow {
    /// The occurrence that is currently eligible for stop/snooze.
    ///
    /// There is intentionally no event fallback: `StopSchedule` clears the
    /// window, so a stale DELIVERED event must not resurrect snooze
    /// eligibility.
    #[must_use]
    pub fn active_occurrence(&self, now_unix: i64) -> Option<Occurrence> {
        if self.active_occurrence_unix <= 0 {
            return None;
        }
        if self.active_until_unix > now_unix || self.snooze_until_unix > now_unix {
            return Some(Occurrence::new(self.active_occurrence_unix));
        }
        None
    }

    /// The occurrence that a rule change must close.
    ///
    /// Covers a ringing window and a pending snooze, including a snooze whose
    /// due time has passed but whose activation tick has not run yet.
    #[must_use]
    pub fn occurrence_to_close(&self, now_unix: i64) -> Option<Occurrence> {
        self.active_occurrence(now_unix).or_else(|| {
            (self.active_occurrence_unix > 0 && self.snooze_until_unix > 0)
                .then_some(Occurrence::new(self.active_occurrence_unix))
        })
    }

    /// Whether the window is still ringing or waiting for a snooze.
    #[must_use]
    pub fn is_open(&self, now_unix: i64) -> bool {
        self.active_occurrence_unix > 0
            && (self.active_until_unix > now_unix || self.snooze_until_unix > now_unix)
    }
}

/// What a tick should do with a schedule whose `next_due_unix` has passed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DueClass {
    /// Nothing is due yet (or the row has no next occurrence).
    NotDue,
    /// Within the missed grace: emit a regular FIRE.
    Fire {
        occurrence: Occurrence,
        late_seconds: i64,
    },
    /// Later than the missed grace: emit a terminal SKIPPED and fast-forward.
    Missed {
        occurrence: Occurrence,
        late_seconds: i64,
    },
}

/// Classifies a due occurrence against `missed_grace_seconds`.
#[must_use]
pub fn classify_due(next_due_unix: i64, now_unix: i64, missed_grace_seconds: i64) -> DueClass {
    if next_due_unix <= 0 || next_due_unix > now_unix {
        return DueClass::NotDue;
    }
    let occurrence = Occurrence::new(next_due_unix);
    let late_seconds = now_unix - next_due_unix;
    if late_seconds > missed_grace_seconds {
        DueClass::Missed {
            occurrence,
            late_seconds,
        }
    } else {
        DueClass::Fire {
            occurrence,
            late_seconds,
        }
    }
}

/// Detail text of a terminal SKIPPED event for a missed occurrence.
#[must_use]
pub fn missed_grace_detail(late_seconds: i64) -> String {
    format!("missed_grace: {late_seconds} seconds late")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_occurrence_uses_window_or_snooze() {
        let window = RingWindow {
            active_occurrence_unix: 100,
            active_until_unix: 200,
            snooze_until_unix: 0,
        };
        assert_eq!(window.active_occurrence(150), Some(Occurrence::new(100)));
        assert_eq!(window.active_occurrence(200), None);

        let snoozing = RingWindow {
            active_occurrence_unix: 100,
            active_until_unix: 0,
            snooze_until_unix: 300,
        };
        assert_eq!(snoozing.active_occurrence(250), Some(Occurrence::new(100)));
        assert_eq!(
            snoozing.occurrence_to_close(250),
            Some(Occurrence::new(100))
        );
    }

    #[test]
    fn occurrence_to_close_covers_pending_snooze() {
        let window = RingWindow {
            active_occurrence_unix: 100,
            active_until_unix: 0,
            snooze_until_unix: 50,
        };
        assert_eq!(window.active_occurrence(100), None);
        assert_eq!(window.occurrence_to_close(100), Some(Occurrence::new(100)));
    }

    #[test]
    fn classify_due_matches_go_grace_boundary() {
        assert_eq!(classify_due(0, 1000, 600), DueClass::NotDue);
        assert_eq!(classify_due(1001, 1000, 600), DueClass::NotDue);
        assert_eq!(
            classify_due(400, 1000, 600),
            DueClass::Fire {
                occurrence: Occurrence::new(400),
                late_seconds: 600,
            }
        );
        assert_eq!(
            classify_due(399, 1000, 600),
            DueClass::Missed {
                occurrence: Occurrence::new(399),
                late_seconds: 601,
            }
        );
        assert_eq!(missed_grace_detail(601), "missed_grace: 601 seconds late");
    }
}
