//! The Telegram HTML message.
//!
//! The buy/sell labels are intentionally inverted relative to the model,
//! exactly as users see them today: "Покупка" prints `sell` and "Продажа"
//! prints `buy`. Numbers use Go's `%.2f`; Rust's fixed-precision formatter is
//! correctly rounded on the same binary `f64`, which is what the `.x5` format
//! vectors pin.

use std::fmt::Write as _;

use super::model::{Alert, Level, Rates};

/// Build the rates message, with alert sections when present (warnings first,
/// then criticals). The result always ends with a newline.
#[must_use]
pub fn format(rates: &Rates, alerts: &[Alert]) -> String {
    let mut out = String::new();
    out.push_str("💱 <b>Курсы валют на kurs.kz</b>\n");
    out.push('\n');
    out.push_str("🇺🇸 <b>USD → KZT</b>\n");
    let _ = writeln!(
        out,
        "💵 Покупка: <code>{}</code> ₸",
        fmt2(rates.usd_to_kzt.sell)
    );
    let _ = writeln!(
        out,
        "💸 Продажа: <code>{}</code> ₸",
        fmt2(rates.usd_to_kzt.buy)
    );
    out.push('\n');
    out.push_str("🇷🇺 <b>RUB → KZT</b>\n");
    let _ = writeln!(
        out,
        "💵 Покупка: <code>{}</code> ₸",
        fmt2(rates.rub_to_kzt.sell)
    );
    let _ = writeln!(
        out,
        "💸 Продажа: <code>{}</code> ₸",
        fmt2(rates.rub_to_kzt.buy)
    );

    if alerts.is_empty() {
        return out;
    }

    let warnings: Vec<&Alert> = alerts
        .iter()
        .filter(|alert| alert.level == Level::Warning)
        .collect();
    let criticals: Vec<&Alert> = alerts
        .iter()
        .filter(|alert| alert.level == Level::Critical)
        .collect();

    if !warnings.is_empty() {
        out.push('\n');
        out.push_str("━━━━━━━━\n");
        out.push_str("⚠️ <b>ПРЕДУПРЕЖДЕНИЯ</b>\n");
        out.push('\n');
        for alert in warnings {
            append_alert(&mut out, alert);
        }
    }
    if !criticals.is_empty() {
        out.push('\n');
        out.push_str("━━━━━━━━\n");
        out.push_str("🚨 <b>КРИТИЧЕСКИЕ ИЗМЕНЕНИЯ</b>\n");
        out.push('\n');
        for alert in criticals {
            append_alert(&mut out, alert);
        }
    }
    out
}

fn append_alert(out: &mut String, alert: &Alert) {
    let (direction_emoji, verb) = match alert.direction {
        super::model::Direction::Up => ("📈", "вырос"),
        super::model::Direction::Down => ("📉", "упал"),
    };
    let _ = writeln!(
        out,
        "{direction_emoji} {} <b>{}</b> ({}) {verb} на <code>{}%</code> за {}",
        alert.pair.emoji(),
        alert.pair.display_name(),
        alert.rate_type.display_name(),
        fmt2(alert.change_percent.abs()),
        alert.period.display
    );
    let _ = writeln!(
        out,
        "   <code>{}</code> ₸ → <code>{}</code> ₸",
        fmt2(alert.old_rate),
        fmt2(alert.new_rate)
    );
    out.push('\n');
}

/// Format with two decimals, matching Go's `%.2f`.
#[must_use]
pub fn fmt2(value: f64) -> String {
    format!("{value:.2}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::model::{Direction, ExchangeRate, Pair, Period, RateType};

    fn rates() -> Rates {
        Rates {
            usd_to_kzt: ExchangeRate {
                buy: 485.5,
                sell: 487.2,
            },
            rub_to_kzt: ExchangeRate {
                buy: 4.85,
                sell: 4.92,
            },
        }
    }

    #[test]
    fn go_percent_two_rounding_matches_the_x5_cases() {
        // These are the exact binary half-way-looking values from the vector.
        assert_eq!(fmt2(1.005), "1.00");
        assert_eq!(fmt2(2.675), "2.67");
        assert_eq!(fmt2(480.005), "480.00");
        assert_eq!(fmt2(0.805), "0.81");
        assert_eq!(fmt2(1.015), "1.01");
    }

    #[test]
    fn labels_are_inverted_and_warnings_precede_criticals() {
        let warning = Alert {
            level: Level::Warning,
            period: Period::get("HOUR").expect("HOUR"),
            pair: Pair::UsdToKzt,
            rate_type: RateType::Sell,
            direction: Direction::Up,
            change_percent: 0.8,
            old_rate: 480.0,
            new_rate: 483.84,
        };
        let critical = Alert {
            level: Level::Critical,
            period: Period::get("DAY").expect("DAY"),
            pair: Pair::UsdToKzt,
            rate_type: RateType::Sell,
            direction: Direction::Down,
            change_percent: -2.5,
            old_rate: 500.0,
            new_rate: 487.5,
        };
        let text = format(&rates(), &[critical, warning]);
        assert!(text.contains("💵 Покупка: <code>487.20</code>"));
        assert!(text.contains("💸 Продажа: <code>485.50</code>"));
        assert!(text.contains("(продажа) вырос на <code>0.80%</code> за час"));
        let warnings = text.find("ПРЕДУПРЕЖДЕНИЯ").expect("warnings");
        let criticals = text.find("КРИТИЧЕСКИЕ ИЗМЕНЕНИЯ").expect("criticals");
        assert!(warnings < criticals);
        assert!(text.ends_with('\n'));
    }
}
