//! Threshold evaluation and deduplication as pure functions.
//!
//! The Go checker read the store inside its loop. Here [`plan`] receives two
//! lookup closures instead, so every rule and its order can be replayed from
//! the JSON vectors without a database or a fake. The caller (`pipeline`)
//! supplies the closures over the real store and applies [`Plan::clears`].

use super::model::{
    Alert, AlertKey, Direction, Level, PERIODS, Pair, Period, RateType, Rates, Record, SentAlert,
};

/// The effects of one threshold pass, in evaluation order.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Plan {
    /// Alerts that should be delivered.
    pub alerts: Vec<Alert>,
    /// Dedup rows whose threshold no longer fires. Applied before sending.
    pub clears: Vec<AlertKey>,
}

/// Evaluate all four periods over the current rates.
///
/// `history` returns the newest record at or before the period cutoff, or
/// `None` when the lookup failed (Go logged the error and skipped the period,
/// including its clears). `sent` returns the last delivered alert for a key,
/// or `None` when absent or unreadable.
///
/// `skip_dedup` is the manual-trigger contract: candidates are accepted
/// unconditionally and no dedup row is read or cleared.
pub fn plan<H, S>(rates: &Rates, skip_dedup: bool, mut history: H, mut sent: S) -> Plan
where
    H: FnMut(&Period) -> Option<Record>,
    S: FnMut(&AlertKey) -> Option<SentAlert>,
{
    let mut plan = Plan::default();
    for period in PERIODS {
        let Some(historical) = history(&period) else {
            continue;
        };
        let checks = [
            (
                Pair::UsdToKzt,
                RateType::Sell,
                rates.usd_to_kzt.sell,
                historical.rates.usd_to_kzt.sell,
            ),
            (
                Pair::UsdToKzt,
                RateType::Buy,
                rates.usd_to_kzt.buy,
                historical.rates.usd_to_kzt.buy,
            ),
            (
                Pair::RubToKzt,
                RateType::Sell,
                rates.rub_to_kzt.sell,
                historical.rates.rub_to_kzt.sell,
            ),
            (
                Pair::RubToKzt,
                RateType::Buy,
                rates.rub_to_kzt.buy,
                historical.rates.rub_to_kzt.buy,
            ),
        ];
        for (pair, rate_type, current, old) in checks {
            let key = AlertKey::new(pair, period.name, rate_type);
            let candidate = evaluate(period, pair, rate_type, current, old);
            match candidate {
                None => {
                    if !skip_dedup {
                        plan.clears.push(key);
                    }
                }
                Some(alert) => {
                    if skip_dedup {
                        plan.alerts.push(alert);
                    } else {
                        let last = sent(&key);
                        if should_send(&alert, last.as_ref(), &period) {
                            plan.alerts.push(alert);
                        }
                    }
                }
            }
        }
    }
    plan
}

/// Evaluate one threshold pair. Thresholds are inclusive and the direction is
/// `DOWN` for zero/NaN; rates must be finite and positive, so a broken
/// historical row cannot divide by zero and emit a `+Inf` alert.
#[must_use]
pub fn evaluate(
    period: Period,
    pair: Pair,
    rate_type: RateType,
    current: f64,
    historical: f64,
) -> Option<Alert> {
    if !valid_rate(current) || !valid_rate(historical) {
        return None;
    }
    let change_percent = (current - historical) / historical * 100.0;
    let abs_change = change_percent.abs();
    let level = if abs_change >= period.critical {
        Level::Critical
    } else if abs_change >= period.warning {
        Level::Warning
    } else {
        return None;
    };
    let direction = if change_percent > 0.0 {
        Direction::Up
    } else {
        Direction::Down
    };
    Some(Alert {
        level,
        period,
        pair,
        rate_type,
        direction,
        change_percent,
        old_rate: historical,
        new_rate: current,
    })
}

/// A rate usable in a percentage comparison.
#[must_use]
pub fn valid_rate(rate: f64) -> bool {
    rate > 0.0 && rate.is_finite()
}

/// The five deduplication rules, in the Go order.
#[must_use]
pub fn should_send(candidate: &Alert, last: Option<&SentAlert>, period: &Period) -> bool {
    let Some(last) = last else {
        return true;
    };
    // A zero or non-finite rate_at_alert (imported or corrupted row) cannot
    // define the re-alert band: without the guard the division yields +Inf and
    // every candidate is sent. Treat such a row as absent.
    if !valid_rate(last.rate_at_alert) {
        return true;
    }
    if last.level == Level::Warning && candidate.level == Level::Critical {
        return true;
    }
    if last.direction != candidate.direction {
        return true;
    }
    let diff = ((candidate.new_rate - last.rate_at_alert) / last.rate_at_alert * 100.0).abs();
    diff >= period.re_alert
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::model::ExchangeRate;

    fn rates(current: f64) -> Rates {
        Rates {
            usd_to_kzt: ExchangeRate {
                buy: current,
                sell: current,
            },
            rub_to_kzt: ExchangeRate {
                buy: current,
                sell: current,
            },
        }
    }

    fn record(historical: f64) -> Record {
        let rate = ExchangeRate {
            buy: historical,
            sell: historical,
        };
        Record {
            timestamp: jiff::Timestamp::UNIX_EPOCH,
            rates: Rates {
                usd_to_kzt: rate,
                rub_to_kzt: rate,
            },
        }
    }

    #[test]
    fn inclusive_thresholds_and_invalid_rates() {
        let hour = Period::get("HOUR").expect("HOUR");
        let alert = evaluate(hour, Pair::UsdToKzt, RateType::Sell, 101.0, 100.0).expect("critical");
        assert_eq!(alert.level, Level::Critical);
        assert_eq!(alert.direction, Direction::Up);
        assert!(evaluate(hour, Pair::UsdToKzt, RateType::Sell, 100.1, 100.0).is_none());
        for historical in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(
                evaluate(hour, Pair::UsdToKzt, RateType::Sell, 100.0, historical).is_none(),
                "{historical}"
            );
        }
    }

    #[test]
    fn a_clear_is_planned_for_every_non_firing_slot() {
        let plan = plan(
            &rates(100.01),
            false,
            |period| (period.name == "HOUR").then_some(record(100.0)),
            |_| None,
        );
        assert!(plan.alerts.is_empty());
        assert_eq!(plan.clears.len(), 4);
    }

    #[test]
    fn a_failed_history_lookup_skips_the_period_and_its_clears() {
        let plan = plan(&rates(100.59), false, |_| None, |_| None);
        assert!(plan.alerts.is_empty());
        assert!(plan.clears.is_empty());
    }

    #[test]
    fn manual_trigger_skips_dedup_reads_and_clears() {
        let plan = plan(
            &rates(100.59),
            true,
            |period| (period.name == "HOUR").then_some(record(100.0)),
            |_| None,
        );
        assert_eq!(plan.alerts.len(), 4);
        assert!(plan.clears.is_empty());
    }
}
