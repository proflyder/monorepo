//! Pure time-tool domain: RFC3339 parsing, validation and spoken text.
//!
//! The tools accept one absolute instant (RFC3339 with an explicit offset)
//! because the voice model is bad at local-time arithmetic. Everything that
//! is shown to the owner is rendered in the home timezone from configuration,
//! never in the model's guess.

use jiff::Timestamp;
use jiff::tz::TimeZone;

use crate::scheduler::WallTime;

/// Accepted repeat modes.
pub const REPEAT_ONCE: &str = "once";
/// Accepted repeat modes.
pub const REPEAT_DAILY: &str = "daily";
/// Accepted repeat modes.
pub const REPEAT_WEEKDAYS: &str = "weekdays";
/// Accepted repeat modes.
pub const REPEAT_WEEKENDS: &str = "weekends";

/// A validated absolute alarm target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlarmTarget {
    /// Instant requested by the model.
    pub instant: Timestamp,
    /// Local wall time of day.
    pub wall: WallTime,
    /// `YYYY-MM-DD` for a one-shot alarm, empty for a repeating rule.
    pub once_date: String,
    /// Scheduler `REPEAT_KIND_*` value.
    pub repeat: String,
}

/// Parse an RFC3339 instant and convert it to the home timezone.
pub fn parse_instant(raw: &str) -> Result<Timestamp, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(
            "Не указано время. Назови дату и время, например 2026-11-12T22:00:00+05:00.".to_owned(),
        );
    }
    raw.parse::<Timestamp>().map_err(|_| {
        "Не понял время. Нужен формат RFC3339 с часовым поясом, например 2026-11-12T22:00:00+05:00.".to_owned()
    })
}

/// Validate an alarm target. One-shot alarms must be in the future and no
/// farther than seven days; repeating alarms only need a valid instant.
pub fn alarm_target(
    raw: &str,
    repeat: Option<&str>,
    now: Timestamp,
) -> Result<AlarmTarget, String> {
    let instant = parse_instant(raw)?;
    let repeat = normalize_repeat(repeat)?;
    if repeat == REPEAT_ONCE {
        if instant <= now {
            return Err(format!(
                "Это время уже прошло ({}). Назови будущее время.",
                human_instant(now, &TimeZone::UTC)
            ));
        }
        if instant.as_second() - now.as_second() > 7 * 24 * 3600 {
            return Err(
                "Однократный будильник можно поставить не дальше чем на 7 дней вперёд.".to_owned(),
            );
        }
    }
    Ok(AlarmTarget {
        instant,
        wall: WallTime::default(),
        once_date: String::new(),
        repeat: scheduler_repeat(&repeat).to_owned(),
    })
}

/// Normalize the spoken repeat argument.
pub fn normalize_repeat(raw: Option<&str>) -> Result<String, String> {
    let value = raw.unwrap_or(REPEAT_ONCE).trim().to_lowercase();
    let value = value.replace('_', "-");
    match value.as_str() {
        "" | "once" | "one" | "single" | "однократно" | "один раз" => {
            Ok(REPEAT_ONCE.to_owned())
        }
        "daily" | "every-day" | "каждый день" | "ежедневно" => {
            Ok(REPEAT_DAILY.to_owned())
        }
        "weekdays" | "по будням" | "будни" => Ok(REPEAT_WEEKDAYS.to_owned()),
        "weekends" | "по выходным" | "выходные" => Ok(REPEAT_WEEKENDS.to_owned()),
        other => Err(format!(
            "Неизвестный повтор {other:?}. Допустимо: once, daily, weekdays, weekends."
        )),
    }
}

/// Scheduler enum for a normalized repeat.
pub fn scheduler_repeat(repeat: &str) -> &'static str {
    match repeat {
        REPEAT_DAILY => "REPEAT_KIND_DAILY",
        REPEAT_WEEKDAYS => "REPEAT_KIND_WEEKDAYS",
        REPEAT_WEEKENDS => "REPEAT_KIND_WEEKENDS",
        _ => "REPEAT_KIND_ONCE",
    }
}

/// Human repeat text for speech.
pub fn repeat_human(repeat: &str) -> &'static str {
    match repeat {
        REPEAT_DAILY => "каждый день",
        REPEAT_WEEKDAYS => "по будням",
        REPEAT_WEEKENDS => "по выходным",
        _ => "однократно",
    }
}

/// Convert an instant to the home wall time and one-shot date.
pub fn local_parts(instant: Timestamp, zone: &TimeZone) -> (WallTime, String) {
    let zoned = instant.to_zoned(zone.clone());
    let wall = WallTime {
        hour: i32::from(zoned.hour()),
        minute: i32::from(zoned.minute()),
        second: i32::from(zoned.second()),
    };
    let date = zoned.date().strftime("%Y-%m-%d").to_string();
    (wall, date)
}

/// `22:00` or `07:30:15` in the home timezone.
pub fn clock_text(instant: Timestamp, zone: &TimeZone) -> String {
    let zoned = instant.to_zoned(zone.clone());
    if zoned.second() == 0 {
        zoned.strftime("%H:%M").to_string()
    } else {
        zoned.strftime("%H:%M:%S").to_string()
    }
}

/// `12 ноября, 22:00` in the home timezone.
pub fn human_instant(instant: Timestamp, zone: &TimeZone) -> String {
    let zoned = instant.to_zoned(zone.clone());
    format!(
        "{} {}, {}",
        zoned.day(),
        month_name(i32::from(zoned.month())),
        if zoned.second() == 0 {
            zoned.strftime("%H:%M").to_string()
        } else {
            zoned.strftime("%H:%M:%S").to_string()
        }
    )
}

/// Full `time_now` answer with weekday, date, clock and zone name.
pub fn now_text(instant: Timestamp, zone: &TimeZone, zone_name: &str) -> String {
    let zoned = instant.to_zoned(zone.clone());
    format!(
        "{}, {} {} {}, {} ({zone_name})",
        weekday_name(i32::from(zoned.weekday().to_monday_one_offset())),
        zoned.day(),
        month_name(i32::from(zoned.month())),
        zoned.year(),
        zoned.strftime("%H:%M:%S"),
    )
}

/// RFC3339 representation in the home offset.
pub fn rfc3339_local(instant: Timestamp, zone: &TimeZone) -> String {
    instant
        .to_zoned(zone.clone())
        .strftime("%Y-%m-%dT%H:%M:%S%:z")
        .to_string()
}

/// Default label used by the HA client today: `Будильник 07:30`.
pub fn default_alarm_label(instant: Timestamp, zone: &TimeZone) -> String {
    format!("Будильник {}", clock_text(instant, zone))
}

/// Human duration: `10 минут`, `1 час 30 минут`, `45 секунд`.
pub fn duration_text(seconds: i64) -> String {
    if seconds < 60 {
        return format!(
            "{seconds} {}",
            plural(seconds, "секунда", "секунды", "секунд")
        );
    }
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let remainder = seconds % 60;
    let mut parts = Vec::new();
    if hours > 0 {
        parts.push(format!("{hours} {}", plural(hours, "час", "часа", "часов")));
    }
    if minutes > 0 {
        parts.push(format!(
            "{minutes} {}",
            plural(minutes, "минута", "минуты", "минут")
        ));
    }
    if remainder > 0 {
        parts.push(format!(
            "{remainder} {}",
            plural(remainder, "секунда", "секунды", "секунд")
        ));
    }
    parts.join(" ")
}

fn plural(value: i64, one: &'static str, few: &'static str, many: &'static str) -> &'static str {
    let value = value.rem_euclid(100);
    if (11..=14).contains(&value) {
        many
    } else {
        match value.rem_euclid(10) {
            1 => one,
            2..=4 => few,
            _ => many,
        }
    }
}

fn month_name(month: i32) -> &'static str {
    match month {
        1 => "января",
        2 => "февраля",
        3 => "марта",
        4 => "апреля",
        5 => "мая",
        6 => "июня",
        7 => "июля",
        8 => "августа",
        9 => "сентября",
        10 => "октября",
        11 => "ноября",
        _ => "декабря",
    }
}

fn weekday_name(day: i32) -> &'static str {
    match day {
        1 => "понедельник",
        2 => "вторник",
        3 => "среда",
        4 => "четверг",
        5 => "пятница",
        6 => "суббота",
        _ => "воскресенье",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zone() -> TimeZone {
        TimeZone::get("Asia/Almaty").expect("zone")
    }

    fn at(raw: &str) -> Timestamp {
        raw.parse().expect("timestamp")
    }

    #[test]
    fn rfc3339_without_offset_is_rejected() {
        assert!(parse_instant("2026-11-12T22:00:00").is_err());
        assert!(parse_instant("2026-11-12T22:00:00+05:00").is_ok());
    }

    #[test]
    fn one_shot_past_and_too_far_are_rejected() {
        let now = at("2026-11-12T12:00:00+05:00");
        assert!(alarm_target("2026-11-12T11:59:00+05:00", None, now).is_err());
        assert!(alarm_target("2026-11-20T12:00:00+05:00", None, now).is_err());
        assert!(alarm_target("2026-11-19T12:00:00+05:00", None, now).is_ok());
    }

    #[test]
    fn repeating_alarm_accepts_an_earlier_clock_time() {
        let now = at("2026-11-12T12:00:00+05:00");
        let target =
            alarm_target("2026-11-12T07:30:00+05:00", Some("weekdays"), now).expect("target");
        assert_eq!(target.repeat, "REPEAT_KIND_WEEKDAYS");
    }

    #[test]
    fn local_text_uses_the_home_timezone() {
        let instant = at("2026-11-12T17:00:00Z");
        assert_eq!(clock_text(instant, &zone()), "22:00");
        assert_eq!(human_instant(instant, &zone()), "12 ноября, 22:00");
        assert_eq!(
            now_text(instant, &zone(), "Asia/Almaty"),
            "четверг, 12 ноября 2026, 22:00:00 (Asia/Almaty)"
        );
        assert_eq!(rfc3339_local(instant, &zone()), "2026-11-12T22:00:00+05:00");
    }

    #[test]
    fn durations_are_russian_and_plural() {
        assert_eq!(duration_text(45), "45 секунд");
        assert_eq!(duration_text(60), "1 минута");
        assert_eq!(duration_text(90), "1 минута 30 секунд");
        assert_eq!(duration_text(5_400), "1 час 30 минут");
        assert_eq!(duration_text(7_200), "2 часа");
    }
}
