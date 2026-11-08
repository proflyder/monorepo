//! Local wall-clock recurrence in an IANA time zone.
//!
//! Deterministic DST policy (identical to the Go planner):
//!
//! - a wall time that does not exist (spring-forward gap) is skipped: that
//!   civil date produces no occurrence and the search moves on;
//! - an ambiguous wall time (fall-back overlap) resolves to the earliest of
//!   the two UTC instants;
//! - the candidate calendar is iterated by civil dates, never by adding
//!   durations in the zone, so a skipped civil date (`Pacific/Apia`
//!   2011-12-30) cannot stall the cursor.

use jiff::Timestamp;
use jiff::civil::{Date, DateTime};
use jiff::tz::{AmbiguousOffset, TimeZone};

/// Upper bound for the recurrence search. Every supported repeat kind has a
/// matching date within seven days; this is a safety net, not a normal path.
pub const MAX_LOOKAHEAD_DAYS: i32 = 400;

/// Repeat policy of a schedule rule.
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
    /// Numeric wire/storage code (`repeat_kind`).
    #[must_use]
    pub const fn code(self) -> i32 {
        self as i32
    }

    /// Reconstructs a repeat from its numeric code.
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

    /// Protobuf enum name, used verbatim in validation messages.
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

    /// Parses the protobuf enum name used by the golden planner vectors.
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

/// Local wall-clock time of day in [`Rule::timezone`].
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct WallTime {
    pub hour: i32,
    pub minute: i32,
    pub second: i32,
}

/// A local wall-clock recurrence.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rule {
    /// IANA zone name, for example `Europe/Berlin`.
    pub timezone: String,
    pub at: WallTime,
    pub repeat: Repeat,
    /// ISO weekdays 1=Mon..7=Sun, `Repeat::Custom` only.
    pub weekdays: Vec<i32>,
    /// `YYYY-MM-DD`, `Repeat::Once` only.
    pub once_date: String,
}

/// A rule validation failure. Its `Display` output starts with the Go error
/// prefix `invalid schedule rule`, so wrapping it in an `invalid argument`
/// domain error reproduces the Go message exactly.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid schedule rule: {detail}")]
pub struct RuleError {
    detail: String,
}

impl RuleError {
    /// Builds a rule error from a detail message.
    #[must_use]
    pub fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }

    /// The message without the `invalid schedule rule: ` prefix.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl Rule {
    /// Reports whether the rule is well formed.
    pub fn validate(&self) -> Result<(), RuleError> {
        if self.timezone.is_empty() {
            return Err(RuleError::new("timezone is required"));
        }
        TimeZone::get(&self.timezone)
            .map_err(|_| RuleError::new(format!("unknown timezone {:?}", self.timezone)))?;
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
                if self.once_date.is_empty() {
                    return Err(RuleError::new("once_date is required for REPEAT_KIND_ONCE"));
                }
                if parse_date(&self.once_date).is_none() {
                    return Err(RuleError::new("once_date must be YYYY-MM-DD"));
                }
                if !self.weekdays.is_empty() {
                    return Err(RuleError::new(
                        "weekdays are only valid for REPEAT_KIND_CUSTOM",
                    ));
                }
            }
            Repeat::Daily | Repeat::Weekdays | Repeat::Weekends => {
                if !self.once_date.is_empty() {
                    return Err(RuleError::new(
                        "once_date is only valid for REPEAT_KIND_ONCE",
                    ));
                }
                if !self.weekdays.is_empty() {
                    return Err(RuleError::new(
                        "weekdays are only valid for REPEAT_KIND_CUSTOM",
                    ));
                }
            }
            Repeat::Custom => {
                if !self.once_date.is_empty() {
                    return Err(RuleError::new(
                        "once_date is only valid for REPEAT_KIND_ONCE",
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

    /// Returns the first occurrence strictly after `after_unix`.
    ///
    /// `Ok(None)` means the rule is exhausted: a one-shot date in the past or
    /// a one-shot wall time that falls in a DST gap.
    pub fn next(&self, after_unix: i64) -> Result<Option<i64>, RuleError> {
        self.validate()?;
        let zone = TimeZone::get(&self.timezone).map_err(|error| {
            // Validate already checked this; keep the branch defensive.
            RuleError::new(error.to_string())
        })?;

        if self.repeat == Repeat::Once {
            let (year, month, day) = parse_date(&self.once_date)
                .ok_or_else(|| RuleError::new("once_date must be YYYY-MM-DD"))?;
            let date = Date::new(year, month, day).map_err(|error| {
                RuleError::new(format!("once_date must be YYYY-MM-DD: {error}"))
            })?;
            let Some(occurrence) = resolve_civil(&zone, date, self.at) else {
                return Ok(None);
            };
            return Ok((occurrence > after_unix).then_some(occurrence));
        }

        let after = Timestamp::from_second(after_unix)
            .map_err(|error| RuleError::new(format!("invalid after instant: {error}")))?;
        let mut cursor = after.to_zoned(zone.clone()).date();
        for _ in 0..MAX_LOOKAHEAD_DAYS {
            if self.matches_date(cursor)
                && let Some(occurrence) = resolve_civil(&zone, cursor, self.at)
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

    /// Returns the first occurrence at or after `from_unix`, used to
    /// fast-forward a lagging schedule.
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

/// Sorts ISO weekdays ascending and joins them with commas.
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

/// Maps a civil date/time in `zone` to a UTC instant.
///
/// Returns `None` when the wall time does not exist (DST gap). Ambiguous
/// times resolve to the earliest instant (fold).
fn resolve_civil(zone: &TimeZone, date: Date, at: WallTime) -> Option<i64> {
    // `Rule::validate` guarantees 0..=23 / 0..=59, so the constructor only
    // fails for an impossible civil date, which then has no occurrence.
    let datetime = DateTime::from_parts(
        date,
        jiff::civil::Time::new(
            i8::try_from(at.hour).ok()?,
            i8::try_from(at.minute).ok()?,
            i8::try_from(at.second).ok()?,
            0,
        )
        .ok()?,
    );
    let ambiguous = zone.to_ambiguous_zoned(datetime);
    match ambiguous.offset() {
        AmbiguousOffset::Gap { .. } => None,
        AmbiguousOffset::Fold { .. } => ambiguous
            .earlier()
            .ok()
            .map(|zoned| zoned.timestamp().as_second()),
        AmbiguousOffset::Unambiguous { .. } => ambiguous
            .compatible()
            .ok()
            .map(|zoned| zoned.timestamp().as_second()),
    }
}

fn parse_date(value: &str) -> Option<(i16, i8, i8)> {
    let bytes = value.as_bytes();
    // Go's `2006-01-02` layout is strict: four year digits, then two digits
    // for month and day. `Date::new` additionally rejects 2026-02-30.
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
    Date::new(year, month, day).ok()?;
    Some((year, month, day))
}
