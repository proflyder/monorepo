use home_worker::{Event, Source};

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
