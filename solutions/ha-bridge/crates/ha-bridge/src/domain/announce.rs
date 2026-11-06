//! The message an announcement event speaks.

use home_worker::{Event, Source};

/// A label wins; otherwise the announcement falls back to its kind.
///
/// The Go bridge used the same defaults (`Timer`, `Reminder`, `Alarm`).
#[must_use]
pub fn announce_message(event: &Event) -> String {
    let label = event.label.trim();
    if !label.is_empty() {
        return label.to_owned();
    }
    match event.source {
        Source::Timer => "Timer".to_owned(),
        Source::Reminder => "Reminder".to_owned(),
        _ => "Alarm".to_owned(),
    }
}
