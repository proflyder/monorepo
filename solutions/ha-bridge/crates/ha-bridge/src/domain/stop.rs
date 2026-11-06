//! Whether a global `alarm_stop` may silence the active alarm.
//!
//! Both identities come from scheduler events, never from bridge wall-clock
//! time: a late `STOP` for an older occurrence, or one before a snoozed
//! re-fire, must not silence the alarm that is ringing now.

use home_worker::Event;

use crate::ledger::ActiveAlarm;

/// `true` when `event` may stop the alarm recorded in `active`.
#[must_use]
pub fn stop_matches_active(event: &Event, active: &ActiveAlarm) -> bool {
    if active.source_id.is_empty() || event.source_id != active.source_id {
        return false;
    }
    if active.occurrence_unix != 0
        && event.occurrence_unix != 0
        && active.occurrence_unix != event.occurrence_unix
    {
        return false;
    }
    if active.fire_due_unix != 0 && event.due_unix != 0 && event.due_unix < active.fire_due_unix {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use home_worker::{Action, Event, Source};

    use super::stop_matches_active;
    use crate::ledger::ActiveAlarm;

    fn stop(source: &str, occurrence: i64, due: i64) -> Event {
        Event {
            id: "evt-stop".to_owned(),
            source_id: source.to_owned(),
            task_type: String::new(),
            queue: "home".to_owned(),
            label: String::new(),
            payload: String::new(),
            action: Action::Stop,
            source: Source::Alarm,
            attempts: 0,
            due_unix: due,
            occurrence_unix: occurrence,
            lease_until_unix: 0,
        }
    }

    fn active() -> ActiveAlarm {
        ActiveAlarm {
            source_id: "alarm-a".to_owned(),
            event_id: "evt-fire".to_owned(),
            occurrence_unix: 1700000000,
            fire_due_unix: 1700000600,
            since_unix: 1700000600,
        }
    }

    #[test]
    fn matches_the_started_occurrence() {
        assert!(stop_matches_active(
            &stop("alarm-a", 1700000000, 1700000650),
            &active()
        ));
    }

    #[test]
    fn rejects_a_different_source_or_occurrence() {
        assert!(!stop_matches_active(
            &stop("alarm-b", 1700000000, 1700000650),
            &active()
        ));
        assert!(!stop_matches_active(
            &stop("alarm-a", 1600000000, 1700000650),
            &active()
        ));
    }

    #[test]
    fn rejects_a_stop_before_the_snoozed_re_fire() {
        assert!(!stop_matches_active(
            &stop("alarm-a", 1700000000, 1700000300),
            &active()
        ));
    }

    #[test]
    fn zero_identities_are_wildcards() {
        let mut wildcard = active();
        wildcard.occurrence_unix = 0;
        wildcard.fire_due_unix = 0;
        assert!(stop_matches_active(&stop("alarm-a", 1, 1), &wildcard));
    }
}
