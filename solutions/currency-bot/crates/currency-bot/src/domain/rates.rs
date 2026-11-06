//! Pure kurs.kz extraction: the `var punkts` literal, the intentional IQR
//! filter and the best-five average.
//!
//! This is a direct port of the Go `internal/rates` functions. Only USD and
//! RUB are used; EUR and every other currency is ignored.

use regex::Regex;
use serde_json::Value;
use std::sync::LazyLock;

use super::model::{ExchangeRate, Rates};

/// The Kotlin regex, lazy match included: the first `];` after the array
/// opening bracket terminates the literal.
static PUNKTS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)var punkts = (\[.*?\]);").expect("valid punkts regex"));

/// Why a document could not be turned into rates.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RatesError {
    #[error("could not find 'var punkts' in HTML: kurs.kz parse error")]
    MissingMarker,
    #[error("decode punkts: {0}")]
    Decode(String),
    #[error("no USD rates found: kurs.kz parse error")]
    NoUsd,
    #[error("no RUB rates found: kurs.kz parse error")]
    NoRub,
}

/// One exchange office's accepted USD or RUB quote.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub buy: f64,
    pub sell: f64,
}

/// Extract and aggregate the rates from a kurs.kz HTML document.
pub fn parse(html: &str) -> Result<Rates, RatesError> {
    let Some(captures) = PUNKTS.captures(html) else {
        return Err(RatesError::MissingMarker);
    };
    let literal = captures.get(1).map_or("", |capture| capture.as_str());
    let points: Vec<Value> =
        serde_json::from_str(literal).map_err(|error| RatesError::Decode(error.to_string()))?;

    let mut usd = Vec::new();
    let mut rub = Vec::new();
    for point in &points {
        let Value::Object(object) = point else {
            continue;
        };
        let Some(data) = object.get("data") else {
            continue;
        };
        let Value::Object(data) = data else {
            // Missing or not an object: skip silently, like Kotlin.
            continue;
        };
        if let Some(quote) = parse_point(data.get("USD")) {
            usd.push(quote);
        }
        if let Some(quote) = parse_point(data.get("RUB")) {
            rub.push(quote);
        }
    }
    if usd.is_empty() {
        return Err(RatesError::NoUsd);
    }
    if rub.is_empty() {
        return Err(RatesError::NoRub);
    }

    let usd_buy = best_average(&remove_outliers(&pluck(&usd, |point| point.buy)), false);
    let usd_sell = best_average(&remove_outliers(&pluck(&usd, |point| point.sell)), true);
    let rub_buy = best_average(&remove_outliers(&pluck(&rub, |point| point.buy)), false);
    let rub_sell = best_average(&remove_outliers(&pluck(&rub, |point| point.sell)), true);

    Ok(Rates {
        usd_to_kzt: ExchangeRate {
            buy: usd_buy,
            sell: usd_sell,
        },
        rub_to_kzt: ExchangeRate {
            buy: rub_buy,
            sell: rub_sell,
        },
    })
}

fn parse_point(raw: Option<&Value>) -> Option<Point> {
    let Value::Array(items) = raw? else {
        return None;
    };
    if items.len() < 2 {
        return None;
    }
    let buy = parse_number(&items[0])?;
    if buy <= 0.0 {
        return None;
    }
    let sell = parse_number(&items[1])?;
    if sell <= 0.0 {
        return None;
    }
    Some(Point { buy, sell })
}

/// Accept JSON numbers and numeric strings, matching kotlinx's
/// `JsonPrimitive.doubleOrNull`. Empty strings, null, booleans, arrays,
/// objects and non-finite values fail.
fn parse_number(raw: &Value) -> Option<f64> {
    let value = match raw {
        Value::Number(number) => number.as_f64()?,
        Value::String(text) => text.trim().parse::<f64>().ok()?,
        _ => return None,
    };
    if !value.is_finite() {
        return None;
    }
    Some(value)
}

fn pluck(points: &[Point], field: impl Fn(&Point) -> f64) -> Vec<f64> {
    points.iter().map(field).collect()
}

/// The intentional non-textbook IQR filter from Kotlin: quartile indexes are
/// `size/4` and `size*3/4` with no interpolation, and an empty filtered list
/// falls back to the original values. The original order is preserved.
#[must_use]
pub fn remove_outliers(values: &[f64]) -> Vec<f64> {
    if values.len() < 4 {
        return values.to_vec();
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable_by(f64::total_cmp);
    let q1 = sorted[sorted.len() / 4];
    let q3 = sorted[sorted.len() * 3 / 4];
    let iqr = q3 - q1;
    let lower = q1 - 1.5 * iqr;
    let upper = q3 + 1.5 * iqr;

    let filtered: Vec<f64> = values
        .iter()
        .copied()
        .filter(|value| *value >= lower && *value <= upper)
        .collect();
    if filtered.is_empty() {
        values.to_vec()
    } else {
        filtered
    }
}

/// Average the best five quotes: ascending for sell (cheapest rate when buying
/// currency), descending for buy (highest rate when selling).
#[must_use]
pub fn best_average(values: &[f64], ascending: bool) -> f64 {
    let mut sorted = values.to_vec();
    if ascending {
        sorted.sort_unstable_by(f64::total_cmp);
    } else {
        sorted.sort_unstable_by(|left, right| right.total_cmp(left));
    }
    let n = sorted.len().min(5);
    let mut sum = 0.0;
    for value in &sorted[..n] {
        sum += value;
    }
    sum / n as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_marker_is_an_error() {
        assert_eq!(parse("<html></html>"), Err(RatesError::MissingMarker));
    }

    #[test]
    fn zero_and_malformed_quotes_are_dropped() {
        let html = r#"<script>var punkts = [
            {"data":{"USD":[0,0],"RUB":[0,0]}},
            {"data":{"USD":[10,"11"],"RUB":[1,2]}},
            {"data":{"USD":[10],"RUB":[1,2]}},
            {"name":"x","data":{"USD":[11,"12"],"RUB":[2]}}
        ];</script>"#;
        let rates = parse(html).expect("rates");
        assert_eq!(rates.usd_to_kzt.buy, 10.5);
        assert_eq!(rates.usd_to_kzt.sell, 11.5);
        assert_eq!(rates.rub_to_kzt.buy, 1.0);
        assert_eq!(rates.rub_to_kzt.sell, 2.0);
    }

    #[test]
    fn non_finite_strings_are_rejected() {
        for text in ["NaN", "+Inf", "-Inf", "infinity", ""] {
            let html = format!(
                r#"<script>var punkts = [{{"data":{{"USD":["{text}",11],"RUB":[1,2]}}}}];</script>"#
            );
            assert_eq!(parse(&html), Err(RatesError::NoUsd), "{text}");
        }
    }
}
