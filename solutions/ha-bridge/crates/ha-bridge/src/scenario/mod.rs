//! Scenario definitions and the pure condition rules.
//!
//! The scheduler owns definitions and materializes time triggers; this module
//! only converts the wire `home.v1.Scenario` into the small model the executor
//! needs and evaluates conditions. Steps run in [`executor`].

pub mod executor;
pub mod ledger;

use home_proto::v1;
use jiff::Zoned;

/// One trigger from the definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trigger {
    /// Only `RunScenario` starts it.
    Manual,
    /// A wall-clock trigger; only its timezone matters for execution.
    Time { timezone: String },
}

/// A condition the executor evaluates before running any step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Condition {
    /// The Home Assistant entity state must equal this exact string.
    EntityState { entity_id: String, state: String },
    /// Local wall-clock window; `end < start` wraps over midnight.
    TimeWindow {
        start: WallTime,
        end: WallTime,
        weekdays: Vec<i32>,
    },
}

/// Local hour/minute/second in the scenario timezone.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WallTime {
    pub hour: i32,
    pub minute: i32,
    pub second: i32,
}

impl WallTime {
    fn seconds(self) -> i32 {
        self.hour * 3600 + self.minute * 60 + self.second
    }
}

/// One ordered step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// A `home.control`-shaped catalog call resolved against the live catalog.
    Catalog {
        entry_id: String,
        action: String,
        params_json: String,
    },
    /// Speak text on the station through the same path as `action.station_speak`.
    Speak { text: String },
    /// Wait before the next step; cancellation interrupts the wait.
    Delay { seconds: i32 },
}

/// The executor-side view of a stored definition.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Definition {
    pub id: String,
    pub label: String,
    pub enabled: bool,
    pub triggers: Vec<Trigger>,
    pub conditions: Vec<Condition>,
    pub steps: Vec<Step>,
    pub continue_on_error: bool,
}

impl Definition {
    /// Convert the wire definition. Unknown oneof variants are dropped; the
    /// scheduler rejects them on write, so a dropped variant never reaches a
    /// step or condition by accident.
    #[must_use]
    pub fn from_proto(scenario: &v1::Scenario) -> Self {
        let triggers = scenario
            .triggers
            .iter()
            .filter_map(|trigger| match trigger.kind.as_ref()? {
                v1::scenario_trigger::Kind::Manual(_) => Some(Trigger::Manual),
                v1::scenario_trigger::Kind::Time(time) => Some(Trigger::Time {
                    timezone: time.timezone.clone(),
                }),
            })
            .collect();
        let conditions = scenario
            .conditions
            .iter()
            .filter_map(|condition| match condition.kind.as_ref()? {
                v1::scenario_condition::Kind::EntityState(state) => Some(Condition::EntityState {
                    entity_id: state.entity_id.clone(),
                    state: state.state.clone(),
                }),
                v1::scenario_condition::Kind::TimeWindow(window) => Some(Condition::TimeWindow {
                    start: wall(window.start.as_option().unwrap_or(&v1::WallTime::default())),
                    end: wall(window.end.as_option().unwrap_or(&v1::WallTime::default())),
                    weekdays: window.weekdays.clone(),
                }),
            })
            .collect();
        let steps = scenario
            .steps
            .iter()
            .filter_map(|step| match step.kind.as_ref()? {
                v1::scenario_step::Kind::Catalog(catalog) => Some(Step::Catalog {
                    entry_id: catalog.entry_id.clone(),
                    action: catalog.action.clone(),
                    params_json: catalog.params_json.clone(),
                }),
                v1::scenario_step::Kind::Speak(speak) => Some(Step::Speak {
                    text: speak.text.clone(),
                }),
                v1::scenario_step::Kind::Delay(delay) => Some(Step::Delay {
                    seconds: delay.seconds,
                }),
            })
            .collect();
        Self {
            id: scenario.id.clone(),
            label: scenario.label.clone(),
            enabled: scenario.enabled,
            triggers,
            conditions,
            steps,
            continue_on_error: scenario.continue_on_error,
        }
    }

    /// Timezone of the first time trigger, if any. Conditions use it, falling
    /// back to the process timezone.
    #[must_use]
    pub fn timezone(&self) -> Option<&str> {
        self.triggers.iter().find_map(|trigger| match trigger {
            Trigger::Time { timezone } => Some(timezone.as_str()),
            Trigger::Manual => None,
        })
    }

    /// Whether this definition has at least one time trigger.
    #[must_use]
    pub fn has_time_trigger(&self) -> bool {
        self.timezone().is_some()
    }
}

fn wall(at: &v1::WallTime) -> WallTime {
    WallTime {
        hour: at.hour,
        minute: at.minute,
        second: at.second,
    }
}

/// Evaluate the pure time-window conditions. `now` must already be in the
/// scenario timezone.
#[must_use]
pub fn time_windows_met(conditions: &[Condition], now: &Zoned) -> bool {
    conditions.iter().all(|condition| match condition {
        Condition::EntityState { .. } => true,
        Condition::TimeWindow {
            start,
            end,
            weekdays,
        } => window_met(*start, *end, weekdays, now),
    })
}

fn window_met(start: WallTime, end: WallTime, weekdays: &[i32], now: &Zoned) -> bool {
    let now_seconds =
        i32::from(now.hour()) * 3600 + i32::from(now.minute()) * 60 + i32::from(now.second());
    let start_seconds = start.seconds();
    let end_seconds = end.seconds();

    let (inside, day) = if start_seconds == end_seconds {
        // An empty window is the whole day: the common "00:00-00:00" intent.
        (true, now.clone())
    } else if end_seconds > start_seconds {
        (
            now_seconds >= start_seconds && now_seconds < end_seconds,
            now.clone(),
        )
    } else if now_seconds >= start_seconds {
        // Evening part of a window that wraps over midnight.
        (true, now.clone())
    } else if now_seconds < end_seconds {
        // Morning part: the window started yesterday, so weekdays apply to
        // the day the window began.
        (true, previous_day(now))
    } else {
        (false, now.clone())
    };
    if !inside {
        return false;
    }
    weekdays.is_empty() || weekdays.contains(&(i32::from(day.weekday().to_monday_one_offset())))
}

fn previous_day(now: &Zoned) -> Zoned {
    now.yesterday().unwrap_or_else(|_| now.clone())
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;
    use jiff::tz::TimeZone;

    use super::{Condition, Definition, Trigger, WallTime, time_windows_met};

    fn at(hour: i32, minute: i32) -> WallTime {
        WallTime {
            hour,
            minute,
            second: 0,
        }
    }

    fn zoned(year: i16, month: i8, day: i8, hour: i8, minute: i8) -> jiff::Zoned {
        let zone = TimeZone::get("Asia/Almaty").expect("zone");
        date(year, month, day)
            .at(hour, minute, 0, 0)
            .to_zoned(zone)
            .expect("zoned")
    }

    fn window(start: WallTime, end: WallTime, weekdays: Vec<i32>) -> Vec<Condition> {
        vec![Condition::TimeWindow {
            start,
            end,
            weekdays,
        }]
    }

    #[test]
    fn a_plain_window_contains_its_start_and_excludes_its_end() {
        let conditions = window(at(9, 0), at(18, 0), Vec::new());
        assert!(time_windows_met(&conditions, &zoned(2026, 11, 9, 9, 0)));
        assert!(time_windows_met(&conditions, &zoned(2026, 11, 9, 17, 59)));
        assert!(!time_windows_met(&conditions, &zoned(2026, 11, 9, 18, 0)));
        assert!(!time_windows_met(&conditions, &zoned(2026, 11, 9, 8, 59)));
    }

    #[test]
    fn a_window_wraps_over_midnight() {
        let conditions = window(at(22, 0), at(6, 0), Vec::new());
        assert!(time_windows_met(&conditions, &zoned(2026, 11, 9, 23, 0)));
        assert!(time_windows_met(&conditions, &zoned(2026, 11, 9, 5, 59)));
        assert!(time_windows_met(&conditions, &zoned(2026, 11, 10, 0, 0)));
        assert!(!time_windows_met(&conditions, &zoned(2026, 11, 9, 12, 0)));
        assert!(!time_windows_met(&conditions, &zoned(2026, 11, 9, 6, 0)));
    }

    #[test]
    fn weekdays_apply_to_the_day_the_window_started() {
        // 2026-11-09 is a Monday; the wrapped window starts on Sunday evening.
        let conditions = window(at(22, 0), at(6, 0), vec![7]);
        assert!(time_windows_met(&conditions, &zoned(2026, 11, 8, 23, 0)));
        assert!(time_windows_met(&conditions, &zoned(2026, 11, 9, 5, 0)));
        assert!(!time_windows_met(&conditions, &zoned(2026, 11, 9, 23, 0)));
        assert!(!time_windows_met(&conditions, &zoned(2026, 11, 10, 5, 0)));
    }

    #[test]
    fn weekdays_filter_a_plain_window() {
        let conditions = window(at(9, 0), at(18, 0), vec![1, 3, 5]);
        assert!(time_windows_met(&conditions, &zoned(2026, 11, 9, 12, 0)));
        assert!(!time_windows_met(&conditions, &zoned(2026, 11, 10, 12, 0)));
    }

    #[test]
    fn equal_bounds_mean_the_whole_day() {
        let conditions = window(at(0, 0), at(0, 0), Vec::new());
        for hour in [0, 6, 12, 23] {
            assert!(time_windows_met(&conditions, &zoned(2026, 11, 9, hour, 30)));
        }
    }

    #[test]
    fn timezone_comes_from_the_first_time_trigger() {
        let definition = Definition {
            triggers: vec![
                Trigger::Manual,
                Trigger::Time {
                    timezone: "Asia/Almaty".to_owned(),
                },
                Trigger::Time {
                    timezone: "UTC".to_owned(),
                },
            ],
            ..Definition::default()
        };
        assert_eq!(definition.timezone(), Some("Asia/Almaty"));
        assert!(definition.has_time_trigger());
        assert!(!Definition::default().has_time_trigger());
    }
}
