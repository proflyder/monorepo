//! Currency-bot domain types.
//!
//! These mirror the former Go `internal/model` types one for one. The names
//! are the wire and database spellings (`USD_TO_KZT`, `BUY`, `WARNING`), so
//! the SQL rows and the HTTP field values stay byte-compatible.

use jiff::Timestamp;

/// One currency pair as users see it on kurs.kz: `buy` is what an exchange
/// office pays when buying currency from you, `sell` what it charges when
/// selling currency to you.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExchangeRate {
    pub buy: f64,
    pub sell: f64,
}

/// The aggregated USD/KZT and RUB/KZT snapshot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rates {
    pub usd_to_kzt: ExchangeRate,
    pub rub_to_kzt: ExchangeRate,
}

/// One persisted rate snapshot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Record {
    pub timestamp: Timestamp,
    pub rates: Rates,
}

/// The currency pair an alert belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Pair {
    UsdToKzt,
    RubToKzt,
}

impl Pair {
    /// The database/JSON spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UsdToKzt => "USD_TO_KZT",
            Self::RubToKzt => "RUB_TO_KZT",
        }
    }

    /// Parse the stored spelling. Unknown values are preserved by the store
    /// and only feed dedup keys, so an unknown pair is simply `None`.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "USD_TO_KZT" => Some(Self::UsdToKzt),
            "RUB_TO_KZT" => Some(Self::RubToKzt),
            _ => None,
        }
    }

    /// The human-readable label used in messages.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::RubToKzt => "RUB → KZT",
            Self::UsdToKzt => "USD → KZT",
        }
    }

    /// The flag shown next to the pair.
    #[must_use]
    pub const fn emoji(self) -> &'static str {
        match self {
            Self::RubToKzt => "🇷🇺",
            Self::UsdToKzt => "🇺🇸",
        }
    }
}

/// The buy or sell side of a pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RateType {
    Buy,
    Sell,
}

impl RateType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Buy => "BUY",
            Self::Sell => "SELL",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "BUY" => Some(Self::Buy),
            "SELL" => Some(Self::Sell),
            _ => None,
        }
    }

    /// Russian label used in messages.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Buy => "покупка",
            Self::Sell => "продажа",
        }
    }
}

/// Alert severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Warning,
    Critical,
}

impl Level {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "WARNING",
            Self::Critical => "CRITICAL",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "WARNING" => Some(Self::Warning),
            "CRITICAL" => Some(Self::Critical),
            _ => None,
        }
    }
}

/// Sign of the observed change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
}

impl Direction {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Up => "UP",
            Self::Down => "DOWN",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "UP" => Some(Self::Up),
            "DOWN" => Some(Self::Down),
            _ => None,
        }
    }
}

/// One threshold lookback window.
///
/// `MONTH` is fixed at 720 hours (30 days), exactly as in Kotlin and Go; there
/// is no calendar-month handling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Period {
    pub name: &'static str,
    pub seconds: i64,
    pub display: &'static str,
    pub warning: f64,
    pub critical: f64,
    pub re_alert: f64,
}

/// The evaluation order used by the alert planner: HOUR, DAY, WEEK, MONTH.
pub const PERIODS: [Period; 4] = [
    Period {
        name: "HOUR",
        seconds: 3_600,
        display: "час",
        warning: 0.5,
        critical: 1.0,
        re_alert: 0.25,
    },
    Period {
        name: "DAY",
        seconds: 86_400,
        display: "сутки",
        warning: 1.0,
        critical: 2.0,
        re_alert: 0.50,
    },
    Period {
        name: "WEEK",
        seconds: 604_800,
        display: "неделю",
        warning: 2.0,
        critical: 4.0,
        re_alert: 1.00,
    },
    Period {
        name: "MONTH",
        seconds: 2_592_000,
        display: "месяц",
        warning: 3.0,
        critical: 5.0,
        re_alert: 1.50,
    },
];

impl Period {
    /// Look up a period by its stored name.
    #[must_use]
    pub fn get(name: &str) -> Option<Self> {
        PERIODS.iter().copied().find(|period| period.name == name)
    }
}

/// Uniquely identifies a deduplicated alert slot.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AlertKey {
    pub pair: Pair,
    pub period: String,
    pub rate_type: RateType,
}

impl AlertKey {
    #[must_use]
    pub fn new(pair: Pair, period: impl Into<String>, rate_type: RateType) -> Self {
        Self {
            pair,
            period: period.into(),
            rate_type,
        }
    }
}

/// One threshold crossing that may be sent to Telegram.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Alert {
    pub level: Level,
    pub period: Period,
    pub pair: Pair,
    pub rate_type: RateType,
    pub direction: Direction,
    pub change_percent: f64,
    pub old_rate: f64,
    pub new_rate: f64,
}

impl Alert {
    #[must_use]
    pub fn key(&self) -> AlertKey {
        AlertKey::new(self.pair, self.period.name, self.rate_type)
    }
}

/// The last delivered alert for one [`AlertKey`].
#[derive(Debug, Clone, PartialEq)]
pub struct SentAlert {
    pub key: AlertKey,
    pub level: Level,
    pub direction: Direction,
    pub rate_at_alert: f64,
    pub change_percent: f64,
    /// Go kept the field in the model but never stored it; it is optional here
    /// for the same reason.
    pub sent_at: Option<Timestamp>,
}
