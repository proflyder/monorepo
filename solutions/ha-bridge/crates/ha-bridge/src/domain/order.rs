//! Batch ordering: source, then due time, then `STOP` before `FIRE`.
//!
//! The ordering only removes races inside one leased batch; events that arrive
//! in different batches keep the server order.

use std::cmp::Ordering;

use home_worker::{Action, Event};

/// Order one leased batch in place.
pub fn order_events(events: &mut [Event]) {
    events.sort_by(compare);
}

fn compare(a: &Event, b: &Event) -> Ordering {
    a.source_id
        .cmp(&b.source_id)
        .then(a.due_unix.cmp(&b.due_unix))
        .then(action_rank(a.action).cmp(&action_rank(b.action)))
        .then(a.id.cmp(&b.id))
}

fn action_rank(action: Action) -> u8 {
    if action == Action::Stop { 0 } else { 1 }
}

#[cfg(test)]
mod tests {
    use home_worker::{Action, Event, Source};

    use super::order_events;

    fn event(id: &str, source: &str, due: i64, action: Action) -> Event {
        Event {
            id: id.to_owned(),
            source_id: source.to_owned(),
            task_type: String::new(),
            queue: "home".to_owned(),
            label: String::new(),
            payload: String::new(),
            action,
            source: Source::Alarm,
            attempts: 0,
            due_unix: due,
            occurrence_unix: 0,
            lease_until_unix: 0,
        }
    }

    #[test]
    fn stop_precedes_fire_at_the_same_due_time() {
        let mut events = vec![
            event("b-2", "b", 20, Action::Fire),
            event("a-2", "a", 10, Action::Fire),
            event("a-1", "a", 10, Action::Stop),
            event("a-0", "a", 5, Action::Fire),
        ];
        order_events(&mut events);
        let ids: Vec<&str> = events.iter().map(|event| event.id.as_str()).collect();
        assert_eq!(ids, ["a-0", "a-1", "a-2", "b-2"]);
    }
}
