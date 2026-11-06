//! The two wall-clock schedules, with the same "strictly after now" and
//! no-catch-up policy as Go.
//!
//! Disambiguation follows the shared planner decision (jiff compatible): a
//! wall time in a spring-forward gap moves forward, and an ambiguous fall-back
//! time resolves to the earlier instant. The committed schedule vectors pin
//! the UTC, Asia/Almaty and both Europe/Berlin DST cases.

use jiff::civil::{Date, DateTime, Time};
use jiff::tz::TimeZone;
use jiff::{Timestamp, Zoned};

/// The next occurrence at `minute:00` in `zone`, strictly after `now`.
#[must_use]
pub fn next_hourly(now: Timestamp, zone: &TimeZone, minute: u8) -> Option<Zoned> {
    let local = now.to_zoned(zone.clone());
    let date = local.date();
    let hour = local.hour();
    let datetime = DateTime::from_parts(
        date,
        Time::new(hour, i8::try_from(minute).ok()?, 0, 0).ok()?,
    );
    let candidate = zone.to_zoned(datetime).ok()?;
    if candidate.timestamp() > now {
        return Some(candidate);
    }
    // Adding one hour is an absolute addition in Go (`Add`), so the candidate
    // can cross a DST gap (01:00 CET + 1h = 03:00 CEST).
    let next =
        Timestamp::from_second(candidate.timestamp().as_second().checked_add(3_600)?).ok()?;
    Some(next.to_zoned(zone.clone()))
}

/// The next occurrence of `weekday` (Go numbering: 0 = Sunday) at `hour:00:00`
/// in `zone`, strictly after `now`.
#[must_use]
pub fn next_weekly(now: Timestamp, zone: &TimeZone, weekday: i8, hour: u8) -> Option<Zoned> {
    let local = now.to_zoned(zone.clone());
    let mut date = local.date();
    let hour = i8::try_from(hour).ok()?;
    for _ in 0..=8 {
        let datetime = DateTime::from_parts(date, Time::new(hour, 0, 0, 0).ok()?);
        let candidate = zone.to_zoned(datetime).ok()?;
        if weekday_matches(date, weekday) && candidate.timestamp() > now {
            return Some(candidate);
        }
        date = tomorrow(date)?;
    }
    None
}

fn weekday_matches(date: Date, go_weekday: i8) -> bool {
    let monday_one = date.weekday().to_monday_one_offset();
    let expected = if go_weekday == 0 { 7 } else { go_weekday };
    monday_one == expected
}

fn tomorrow(date: Date) -> Option<Date> {
    date.tomorrow().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instant(text: &str) -> Timestamp {
        text.parse().expect(text)
    }

    #[test]
    fn strictly_after_now() {
        let utc = TimeZone::get("UTC").expect("UTC");
        assert_eq!(
            next_hourly(instant("2026-10-31T12:30:00Z"), &utc, 0)
                .map(|zoned| crate::domain::time::rfc3339_zoned(&zoned))
                .as_deref(),
            Some("2026-10-31T13:00:00Z")
        );
        assert_eq!(
            next_hourly(instant("2026-10-31T12:30:00Z"), &utc, 45)
                .map(|zoned| crate::domain::time::rfc3339_zoned(&zoned))
                .as_deref(),
            Some("2026-10-31T12:45:00Z")
        );
    }

    #[test]
    fn weekly_moves_to_the_next_matching_day() {
        let utc = TimeZone::get("UTC").expect("UTC");
        // Sunday, 2026-11-01.
        assert_eq!(
            next_weekly(instant("2026-10-31T12:00:00Z"), &utc, 0, 0)
                .map(|zoned| crate::domain::time::rfc3339_zoned(&zoned))
                .as_deref(),
            Some("2026-11-01T00:00:00Z")
        );
        assert_eq!(
            next_weekly(instant("2026-11-02T00:00:00Z"), &utc, 0, 0)
                .map(|zoned| crate::domain::time::rfc3339_zoned(&zoned))
                .as_deref(),
            Some("2026-11-08T00:00:00Z")
        );
    }
}
