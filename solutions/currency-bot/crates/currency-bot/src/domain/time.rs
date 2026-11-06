//! Timestamp formatting and parsing with Go's exact layouts.
//!
//! `time.RFC3339Nano` trims trailing zeros in the fraction (and the dot when
//! the fraction is zero); jiff's `Display` keeps a time-zone annotation or a
//! fixed width, so both are rendered explicitly here.

use jiff::Timestamp;
use jiff::Zoned;
use jiff::civil::Date;
use jiff::tz::TimeZone;

/// Render an instant as UTC `RFC3339Nano`, e.g. `2026-03-17T23:59:59.999Z`.
#[must_use]
pub fn rfc3339_nano(timestamp: Timestamp) -> String {
    let zoned = timestamp.to_zoned(TimeZone::UTC);
    let datetime = zoned.datetime();
    let nanos = timestamp.subsec_nanosecond().unsigned_abs();
    let mut out = format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        datetime.year(),
        datetime.month(),
        datetime.day(),
        datetime.hour(),
        datetime.minute(),
        datetime.second()
    );
    if nanos != 0 {
        let mut fraction = format!("{nanos:09}");
        while fraction.ends_with('0') {
            fraction.pop();
        }
        out.push('.');
        out.push_str(&fraction);
    }
    out.push('Z');
    out
}

/// Render a zoned wall-clock as `RFC3339` (seconds precision) with its offset,
/// matching `time.Time.Format(time.RFC3339)` in a local location.
#[must_use]
pub fn rfc3339_zoned(zoned: &Zoned) -> String {
    let datetime = zoned.datetime();
    let offset = zoned.offset().seconds();
    let suffix = if offset == 0 {
        "Z".to_owned()
    } else {
        let sign = if offset < 0 { '-' } else { '+' };
        let absolute = offset.unsigned_abs();
        format!("{sign}{:02}:{:02}", absolute / 3600, (absolute % 3600) / 60)
    };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{suffix}",
        datetime.year(),
        datetime.month(),
        datetime.day(),
        datetime.hour(),
        datetime.minute(),
        datetime.second()
    )
}

/// Parse the strict `YYYY-MM-DD` date accepted by `?date=`.
#[must_use]
pub fn parse_date(value: &str) -> Option<Date> {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    if bytes
        .iter()
        .enumerate()
        .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return None;
    }
    let year: i16 = value[0..4].parse().ok()?;
    let month: i8 = value[5..7].parse().ok()?;
    let day: i8 = value[8..10].parse().ok()?;
    Date::new(year, month, day).ok()
}

/// Parse an RFC3339 instant. Go's `time.Parse(time.RFC3339, …)` also accepts a
/// fractional second after the seconds field.
#[must_use]
pub fn parse_rfc3339(value: &str) -> Option<Timestamp> {
    value.parse::<Timestamp>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3339_nano_trims_the_fraction_like_go() {
        for (text, want) in [
            ("2026-10-31T12:00:00Z", "2026-10-31T12:00:00Z"),
            ("2026-03-17T23:59:59.999Z", "2026-03-17T23:59:59.999Z"),
            ("2026-03-17T23:59:59.120Z", "2026-03-17T23:59:59.12Z"),
        ] {
            let timestamp: Timestamp = text.parse().expect(text);
            assert_eq!(rfc3339_nano(timestamp), want);
        }
    }

    #[test]
    fn date_is_strict() {
        assert!(parse_date("2026-03-18").is_some());
        for bad in ["", "not-a-date", "2026-3-18", "2026-02-30", "2026/03/18"] {
            assert!(parse_date(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn rfc3339_zoned_keeps_the_local_offset() {
        let zone = TimeZone::get("Asia/Almaty").expect("zone");
        let zoned = zone
            .to_zoned(
                "2026-10-31T11:00:00"
                    .parse::<jiff::civil::DateTime>()
                    .expect("datetime"),
            )
            .expect("zoned");
        assert_eq!(rfc3339_zoned(&zoned), "2026-10-31T11:00:00+05:00");
    }
}
