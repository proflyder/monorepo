use jiff::Timestamp;
use jiff::civil::{Date, DateTime, Time};
use jiff::tz::TimeZone;

pub const MAX_LOOKAHEAD_DAYS: i32 = 400;

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
#[repr(i32)]
pub enum Repeat {
    #[default]
    Unspecified = 0,
    Once = 1,
    Daily = 2,
    Weekdays = 3,
    Weekends = 4,
    Custom = 5,
}

impl Repeat {
    #[must_use]
    pub const fn code(self) -> i32 {
        self as i32
    }

    #[must_use]
    pub const fn from_code(code: i32) -> Option<Self> {
        match code {
            0 => Some(Self::Unspecified),
            1 => Some(Self::Once),
            2 => Some(Self::Daily),
            3 => Some(Self::Weekdays),
            4 => Some(Self::Weekends),
            5 => Some(Self::Custom),
            _ => None,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Unspecified => "REPEAT_KIND_UNSPECIFIED",
            Self::Once => "REPEAT_KIND_ONCE",
            Self::Daily => "REPEAT_KIND_DAILY",
            Self::Weekdays => "REPEAT_KIND_WEEKDAYS",
            Self::Weekends => "REPEAT_KIND_WEEKENDS",
            Self::Custom => "REPEAT_KIND_CUSTOM",
        }
    }

    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "REPEAT_KIND_UNSPECIFIED" => Some(Self::Unspecified),
            "REPEAT_KIND_ONCE" => Some(Self::Once),
            "REPEAT_KIND_DAILY" => Some(Self::Daily),
            "REPEAT_KIND_WEEKDAYS" => Some(Self::Weekdays),
            "REPEAT_KIND_WEEKENDS" => Some(Self::Weekends),
            "REPEAT_KIND_CUSTOM" => Some(Self::Custom),
            _ => None,
        }
    }
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct WallTime {
    pub hour: i32,
    pub minute: i32,
    pub second: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rule {
    pub at: WallTime,
    pub repeat: Repeat,
    pub weekdays: Vec<i32>,
    pub once_unix: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid schedule rule: {detail}")]
pub struct RuleError {
    detail: String,
}

impl RuleError {
    #[must_use]
    pub fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }

    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl Rule {
    pub fn validate(&self) -> Result<(), RuleError> {
        if !(0..=23).contains(&self.at.hour) {
            return Err(RuleError::new(format!(
                "hour {} outside 0..23",
                self.at.hour
            )));
        }
        if !(0..=59).contains(&self.at.minute) {
            return Err(RuleError::new(format!(
                "minute {} outside 0..59",
                self.at.minute
            )));
        }
        if !(0..=59).contains(&self.at.second) {
            return Err(RuleError::new(format!(
                "second {} outside 0..59",
                self.at.second
            )));
        }
        match self.repeat {
            Repeat::Once => {
                if self.once_unix <= 0 {
                    return Err(RuleError::new("once_unix is required for REPEAT_KIND_ONCE"));
                }
                if !self.weekdays.is_empty() {
                    return Err(RuleError::new(
                        "weekdays are only valid for REPEAT_KIND_CUSTOM",
                    ));
                }
            }
            Repeat::Daily | Repeat::Weekdays | Repeat::Weekends => {
                if self.once_unix != 0 {
                    return Err(RuleError::new(
                        "once_unix is only valid for REPEAT_KIND_ONCE",
                    ));
                }
                if !self.weekdays.is_empty() {
                    return Err(RuleError::new(
                        "weekdays are only valid for REPEAT_KIND_CUSTOM",
                    ));
                }
            }
            Repeat::Custom => {
                if self.once_unix != 0 {
                    return Err(RuleError::new(
                        "once_unix is only valid for REPEAT_KIND_ONCE",
                    ));
                }
                if self.weekdays.is_empty() {
                    return Err(RuleError::new("REPEAT_KIND_CUSTOM needs weekdays"));
                }
                let mut seen: Vec<i32> = Vec::with_capacity(self.weekdays.len());
                for &weekday in &self.weekdays {
                    if !(1..=7).contains(&weekday) {
                        return Err(RuleError::new(format!("weekday {weekday} outside 1..7")));
                    }
                    if seen.contains(&weekday) {
                        return Err(RuleError::new(format!("duplicate weekday {weekday}")));
                    }
                    seen.push(weekday);
                }
            }
            Repeat::Unspecified => {
                return Err(RuleError::new(format!(
                    "unsupported repeat {}",
                    self.repeat.name()
                )));
            }
        }
        Ok(())
    }

    pub fn next(&self, after_unix: i64) -> Result<Option<i64>, RuleError> {
        self.validate()?;
        if self.repeat == Repeat::Once {
            return Ok((self.once_unix > after_unix).then_some(self.once_unix));
        }

        let after = Timestamp::from_second(after_unix)
            .map_err(|error| RuleError::new(format!("invalid after instant: {error}")))?;
        let mut cursor = after.to_zoned(TimeZone::UTC).date();
        for _ in 0..MAX_LOOKAHEAD_DAYS {
            if self.matches_date(cursor)
                && let Some(occurrence) = utc_instant(cursor, self.at)
                && occurrence > after_unix
            {
                return Ok(Some(occurrence));
            }
            cursor = cursor
                .tomorrow()
                .map_err(|error| RuleError::new(format!("calendar overflow: {error}")))?;
        }
        Err(RuleError::new(format!(
            "no occurrence within {MAX_LOOKAHEAD_DAYS} days"
        )))
    }

    pub fn next_inclusive(&self, from_unix: i64) -> Result<Option<i64>, RuleError> {
        self.next(from_unix.saturating_sub(1))
    }

    fn matches_date(&self, date: Date) -> bool {
        let weekday = date.weekday().to_monday_one_offset();
        match self.repeat {
            Repeat::Daily => true,
            Repeat::Weekdays => (1..=5).contains(&weekday),
            Repeat::Weekends => weekday >= 6,
            Repeat::Custom => self.weekdays.contains(&i32::from(weekday)),
            Repeat::Unspecified | Repeat::Once => false,
        }
    }
}

#[must_use]
pub fn weekdays_text(days: &[i32]) -> String {
    let mut sorted = days.to_vec();
    sorted.sort_unstable();
    sorted
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn utc_instant(date: Date, at: WallTime) -> Option<i64> {
    let time = Time::new(
        i8::try_from(at.hour).ok()?,
        i8::try_from(at.minute).ok()?,
        i8::try_from(at.second).ok()?,
        0,
    )
    .ok()?;
    DateTime::from_parts(date, time)
        .to_zoned(TimeZone::UTC)
        .ok()
        .map(|zoned| zoned.timestamp().as_second())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daily_utc_fires_at_the_stored_time() {
        let rule = Rule {
            at: WallTime {
                hour: 22,
                minute: 0,
                second: 0,
            },
            repeat: Repeat::Daily,
            ..Rule::default()
        };
        assert_eq!(rule.next(1_767_304_740).unwrap(), Some(1_767_304_800));
    }

    #[test]
    fn custom_weekdays_use_the_utc_mask() {
        let rule = Rule {
            at: WallTime {
                hour: 22,
                minute: 0,
                second: 0,
            },
            repeat: Repeat::Custom,
            weekdays: vec![7, 1, 2, 3, 4],
            ..Rule::default()
        };
        let next = rule.next(1_767_500_000).unwrap().expect("occurrence");
        assert_eq!(
            Timestamp::from_second(next).unwrap().to_string(),
            "2026-01-04T22:00:00Z"
        );
    }

    #[test]
    fn once_returns_the_instant_only_when_in_the_future() {
        let rule = Rule {
            repeat: Repeat::Once,
            once_unix: 1_767_339_000,
            ..Rule::default()
        };
        assert_eq!(rule.next(1_767_338_999).unwrap(), Some(1_767_339_000));
        assert_eq!(rule.next(1_767_339_000).unwrap(), None);
    }

    #[test]
    fn almaty_weekdays_round_trip_through_utc() {
        let stored = local_to_utc(
            "Asia/Almaty",
            WallTime {
                hour: 3,
                minute: 0,
                second: 0,
            },
            Repeat::Weekdays,
            &[],
            "",
        );
        assert_eq!(stored.at.hour, 22);
        assert_eq!(stored.weekdays, vec![1, 2, 3, 4, 7]);
        let local = utc_to_local(
            "Asia/Almaty",
            stored.at,
            stored.repeat,
            &stored.weekdays,
            stored.once_unix,
        );
        assert_eq!(local.at.hour, 3);
        assert_eq!(local.weekdays, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn moscow_client_sees_the_same_utc_alarm_as_0100_weekdays() {
        let stored = local_to_utc(
            "Asia/Almaty",
            WallTime {
                hour: 3,
                minute: 0,
                second: 0,
            },
            Repeat::Weekdays,
            &[],
            "",
        );
        let local = utc_to_local(
            "Europe/Moscow",
            stored.at,
            stored.repeat,
            &stored.weekdays,
            stored.once_unix,
        );
        assert_eq!(local.at.hour, 1);
        assert_eq!(local.weekdays, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn missing_header_falls_back_to_utc() {
        let stored = local_to_utc(
            "UTC",
            WallTime {
                hour: 7,
                minute: 30,
                second: 0,
            },
            Repeat::Daily,
            &[],
            "",
        );
        let local = utc_to_local(
            "UTC",
            stored.at,
            stored.repeat,
            &stored.weekdays,
            stored.once_unix,
        );
        assert_eq!(local.at.hour, 7);
        assert_eq!(local.at.minute, 30);
    }

    #[test]
    fn once_requires_an_instant() {
        let rule = Rule {
            repeat: Repeat::Once,
            ..Rule::default()
        };
        assert!(rule.validate().is_err());
    }
}

const REFERENCE_YEAR: i16 = 2026;
const REFERENCE_MONTH: i8 = 1;
const REFERENCE_MONDAY: i8 = 5;

pub fn validate_local_rule(
    at: WallTime,
    repeat: Repeat,
    weekdays: &[i32],
    once_date: &str,
) -> Result<(), RuleError> {
    if !(0..=23).contains(&at.hour) {
        return Err(RuleError::new(format!("hour {} outside 0..23", at.hour)));
    }
    if !(0..=59).contains(&at.minute) {
        return Err(RuleError::new(format!(
            "minute {} outside 0..59",
            at.minute
        )));
    }
    if !(0..=59).contains(&at.second) {
        return Err(RuleError::new(format!(
            "second {} outside 0..59",
            at.second
        )));
    }
    match repeat {
        Repeat::Once => {
            if once_date.is_empty() {
                return Err(RuleError::new("once_date is required for REPEAT_KIND_ONCE"));
            }
            if parse_iso_date(once_date).is_none() {
                return Err(RuleError::new("once_date must be YYYY-MM-DD"));
            }
            if !weekdays.is_empty() {
                return Err(RuleError::new(
                    "weekdays are only valid for REPEAT_KIND_CUSTOM",
                ));
            }
        }
        Repeat::Daily | Repeat::Weekdays | Repeat::Weekends => {
            if !once_date.is_empty() {
                return Err(RuleError::new(
                    "once_date is only valid for REPEAT_KIND_ONCE",
                ));
            }
            if !weekdays.is_empty() {
                return Err(RuleError::new(
                    "weekdays are only valid for REPEAT_KIND_CUSTOM",
                ));
            }
        }
        Repeat::Custom => {
            if !once_date.is_empty() {
                return Err(RuleError::new(
                    "once_date is only valid for REPEAT_KIND_ONCE",
                ));
            }
            if weekdays.is_empty() {
                return Err(RuleError::new("REPEAT_KIND_CUSTOM needs weekdays"));
            }
            let mut seen: Vec<i32> = Vec::with_capacity(weekdays.len());
            for &weekday in weekdays {
                if !(1..=7).contains(&weekday) {
                    return Err(RuleError::new(format!("weekday {weekday} outside 1..7")));
                }
                if seen.contains(&weekday) {
                    return Err(RuleError::new(format!("duplicate weekday {weekday}")));
                }
                seen.push(weekday);
            }
        }
        Repeat::Unspecified => {
            return Err(RuleError::new(format!(
                "unsupported repeat {}",
                repeat.name()
            )));
        }
    }
    Ok(())
}

#[must_use]
pub fn local_to_utc(
    timezone: &str,
    at: WallTime,
    repeat: Repeat,
    weekdays: &[i32],
    once_date: &str,
) -> Rule {
    let zone = TimeZone::get(timezone).unwrap_or(TimeZone::UTC);
    if repeat == Repeat::Once {
        return Rule {
            repeat: Repeat::Once,
            once_unix: resolve_local(&zone, once_date, at).unwrap_or(0),
            ..Rule::default()
        };
    }
    if repeat == Repeat::Unspecified {
        return Rule::default();
    }
    let (at_utc, _) = shift_wall_time(&zone, at);
    let mask = if repeat == Repeat::Daily {
        Vec::new()
    } else {
        shift_mask(&zone, at, repeat, weekdays)
    };
    Rule {
        at: at_utc,
        repeat: if repeat == Repeat::Daily || repeat == Repeat::Custom {
            repeat
        } else {
            Repeat::Custom
        },
        weekdays: mask,
        once_unix: 0,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalRule {
    pub at: WallTime,
    pub weekdays: Vec<i32>,
    pub once_date: String,
}

#[must_use]
pub fn utc_to_local(
    zone: &str,
    at: WallTime,
    repeat: Repeat,
    weekdays: &[i32],
    once_unix: i64,
) -> LocalRule {
    let zone = TimeZone::get(zone).unwrap_or(TimeZone::UTC);
    if repeat == Repeat::Once {
        let Ok(timestamp) = Timestamp::from_second(once_unix) else {
            return LocalRule::default();
        };
        let zoned = timestamp.to_zoned(zone);
        return LocalRule {
            at: WallTime {
                hour: i32::from(zoned.hour()),
                minute: i32::from(zoned.minute()),
                second: i32::from(zoned.second()),
            },
            weekdays: Vec::new(),
            once_date: zoned.date().to_string(),
        };
    }
    let monday =
        Date::new(REFERENCE_YEAR, REFERENCE_MONTH, REFERENCE_MONDAY).expect("reference Monday");
    let Some(datetime) = local_datetime(monday, at) else {
        return LocalRule::default();
    };
    let Ok(utc) = datetime.to_zoned(TimeZone::UTC) else {
        return LocalRule::default();
    };
    let zoned = utc.timestamp().to_zoned(zone.clone());
    let mut mask: Vec<i32> = Vec::new();
    for offset in 0..7 {
        let Ok(date) = monday.checked_add(jiff::Span::new().days(offset)) else {
            continue;
        };
        let utc_weekday = i32::from(date.weekday().to_monday_one_offset());
        let stored = match repeat {
            Repeat::Daily => true,
            Repeat::Weekdays => (1..=5).contains(&utc_weekday),
            Repeat::Weekends => utc_weekday >= 6,
            Repeat::Custom => weekdays.contains(&utc_weekday),
            Repeat::Unspecified | Repeat::Once => false,
        };
        if !stored {
            continue;
        }
        let Some(datetime) = local_datetime(date, at) else {
            continue;
        };
        let Ok(utc) = datetime.to_zoned(TimeZone::UTC) else {
            continue;
        };
        let zoned = utc.timestamp().to_zoned(zone.clone());
        let local_weekday = i32::from(zoned.date().weekday().to_monday_one_offset());
        if !mask.contains(&local_weekday) {
            mask.push(local_weekday);
        }
    }
    mask.sort_unstable();
    LocalRule {
        at: WallTime {
            hour: i32::from(zoned.hour()),
            minute: i32::from(zoned.minute()),
            second: i32::from(zoned.second()),
        },
        weekdays: mask,
        once_date: String::new(),
    }
}

fn local_datetime(date: Date, at: WallTime) -> Option<DateTime> {
    let time = Time::new(
        i8::try_from(at.hour).ok()?,
        i8::try_from(at.minute).ok()?,
        i8::try_from(at.second).ok()?,
        0,
    )
    .ok()?;
    Some(DateTime::from_parts(date, time))
}

fn resolve_local(zone: &TimeZone, once_date: &str, at: WallTime) -> Option<i64> {
    let date = parse_iso_date(once_date)?;
    let datetime = local_datetime(date, at)?;
    datetime
        .to_zoned(zone.clone())
        .ok()
        .map(|zoned| zoned.timestamp().as_second())
}

fn shift_wall_time(zone: &TimeZone, at: WallTime) -> (WallTime, i64) {
    let monday = Date::new(REFERENCE_YEAR, REFERENCE_MONTH, REFERENCE_MONDAY)
        .expect("reference Monday is a valid date");
    let Some(datetime) = local_datetime(monday, at) else {
        return (at, 0);
    };
    let Ok(zoned) = datetime.to_zoned(zone.clone()) else {
        return (at, 0);
    };
    let utc = zoned.timestamp().to_zoned(TimeZone::UTC);
    (
        WallTime {
            hour: i32::from(utc.hour()),
            minute: i32::from(utc.minute()),
            second: i32::from(utc.second()),
        },
        zoned.timestamp().as_second(),
    )
}

fn shift_mask(zone: &TimeZone, at: WallTime, repeat: Repeat, weekdays: &[i32]) -> Vec<i32> {
    let monday =
        Date::new(REFERENCE_YEAR, REFERENCE_MONTH, REFERENCE_MONDAY).expect("reference Monday");
    let mut mask: Vec<i32> = Vec::new();
    for offset in 0..7 {
        let Ok(date) = monday.checked_add(jiff::Span::new().days(offset)) else {
            continue;
        };
        let local_weekday = i32::from(date.weekday().to_monday_one_offset());
        let wanted = match repeat {
            Repeat::Daily => true,
            Repeat::Weekdays => (1..=5).contains(&local_weekday),
            Repeat::Weekends => local_weekday >= 6,
            Repeat::Custom => weekdays.contains(&local_weekday),
            Repeat::Unspecified | Repeat::Once => false,
        };
        if !wanted {
            continue;
        }
        let Some(datetime) = local_datetime(date, at) else {
            continue;
        };
        let Ok(zoned) = datetime.to_zoned(zone.clone()) else {
            continue;
        };
        let utc_date = zoned.timestamp().to_zoned(TimeZone::UTC).date();
        let utc_weekday = i32::from(utc_date.weekday().to_monday_one_offset());
        if !mask.contains(&utc_weekday) {
            mask.push(utc_weekday);
        }
    }
    mask.sort_unstable();
    mask
}

#[must_use]
pub fn parse_iso_date(value: &str) -> Option<Date> {
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
    let year = value.get(0..4)?.parse::<i16>().ok()?;
    let month = value.get(5..7)?.parse::<i8>().ok()?;
    let day = value.get(8..10)?.parse::<i8>().ok()?;
    Date::new(year, month, day).ok()
}
