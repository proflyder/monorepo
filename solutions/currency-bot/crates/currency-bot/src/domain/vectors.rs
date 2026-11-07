//! Replay of `solutions/currency-bot/testdata/vectors/*.json`.
//!
//! The files are the frozen contract emitted by the Go implementation. The
//! Rust domain must reproduce them byte for byte; the JSON is read from the
//! repository (or Bazel runfiles), never copied into the crate.

use serde_json::Value;

use super::alerts::{evaluate, plan, should_send};
use super::message::format;
use super::model::{
    Alert, Direction, ExchangeRate, Level, Pair, Period, RateType, Rates, Record, SentAlert,
};
use super::schedule::{next_hourly, next_weekly};
use super::texts;
use super::time::rfc3339_zoned;
use crate::testdata;

fn read_json(name: &str) -> Value {
    let path = testdata::dir().join("vectors").join(name);
    let bytes =
        std::fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("decode {}: {error}", path.display()))
}

fn special_float(value: &Value) -> f64 {
    match value {
        Value::Number(number) => number.as_f64().expect("number"),
        Value::String(text) => match text.as_str() {
            "NaN" => f64::NAN,
            "+Inf" => f64::INFINITY,
            "-Inf" => f64::NEG_INFINITY,
            other => other.parse().expect("vector number"),
        },
        other => panic!("unsupported vector number {other}"),
    }
}

fn period(name: &str) -> Period {
    Period::get(name).unwrap_or_else(|| panic!("unknown period {name}"))
}

fn pair(name: &str) -> Pair {
    Pair::parse(name).unwrap_or_else(|| panic!("unknown pair {name}"))
}

fn rate_type(name: &str) -> RateType {
    RateType::parse(name).unwrap_or_else(|| panic!("unknown rate type {name}"))
}

fn level(name: &str) -> Level {
    Level::parse(name).unwrap_or_else(|| panic!("unknown level {name}"))
}

fn direction(name: &str) -> Direction {
    Direction::parse(name).unwrap_or_else(|| panic!("unknown direction {name}"))
}

fn rates(value: &Value) -> Rates {
    let pair_value = |name: &str| ExchangeRate {
        buy: value[name]["buy"].as_f64().expect("buy"),
        sell: value[name]["sell"].as_f64().expect("sell"),
    };
    Rates {
        usd_to_kzt: pair_value("usd"),
        rub_to_kzt: pair_value("rub"),
    }
}

#[test]
fn rates_vectors() {
    let vectors = read_json("rates.json");

    for case in vectors["parse_cases"].as_array().expect("parse_cases") {
        let name = case["name"].as_str().expect("name");
        let html = case["html"].as_str().expect("html");
        let result = super::rates::parse(html);
        if case.get("error").and_then(Value::as_bool).unwrap_or(false) {
            assert!(result.is_err(), "{name} must fail");
            continue;
        }
        let got = result.unwrap_or_else(|error| panic!("{name}: {error}"));
        let want = rates(&case["rates"]);
        assert_eq!(got, want, "{name}");
        if name == "live_fixture" {
            let fixture =
                std::fs::read_to_string(testdata::dir().join("rates").join("kurskz.html"))
                    .expect("kurskz.html");
            assert_eq!(super::rates::parse(&fixture).expect("fixture"), want);
        }
    }

    for case in vectors["outlier_cases"].as_array().expect("outlier_cases") {
        let name = case["name"].as_str().expect("name");
        let input: Vec<f64> = case["input"]
            .as_array()
            .expect("input")
            .iter()
            .map(|value| value.as_f64().expect("number"))
            .collect();
        let want: Vec<f64> = case["want"]
            .as_array()
            .expect("want")
            .iter()
            .map(|value| value.as_f64().expect("number"))
            .collect();
        assert_eq!(super::rates::remove_outliers(&input), want, "{name}");
    }

    for case in vectors["best_average_cases"]
        .as_array()
        .expect("best_average_cases")
    {
        let name = case["name"].as_str().expect("name");
        let values: Vec<f64> = case["values"]
            .as_array()
            .expect("values")
            .iter()
            .map(|value| value.as_f64().expect("number"))
            .collect();
        let ascending = case["ascending"].as_bool().expect("ascending");
        let want = case["want"].as_f64().expect("want");
        assert_eq!(
            super::rates::best_average(&values, ascending),
            want,
            "{name}"
        );
    }
}

#[test]
fn alerts_vectors() {
    let vectors = read_json("alerts.json");

    for case in vectors["thresholds"].as_array().expect("thresholds") {
        let name = case["name"].as_str().expect("name");
        let current = special_float(&case["current"]);
        let historical = special_float(&case["historical"]);
        let alert = evaluate(
            period(case["period"].as_str().expect("period")),
            Pair::UsdToKzt,
            RateType::Sell,
            current,
            historical,
        );
        match case.get("want") {
            None => assert!(alert.is_none(), "{name} must not alert"),
            Some(want) => {
                let alert = alert.unwrap_or_else(|| panic!("{name} must alert"));
                assert_eq!(
                    alert.level.as_str(),
                    want["level"].as_str().expect("level"),
                    "{name}"
                );
                assert_eq!(
                    alert.direction.as_str(),
                    want["direction"].as_str().expect("direction"),
                    "{name}"
                );
                assert_eq!(
                    alert.change_percent,
                    want["change_percent"].as_f64().expect("change_percent"),
                    "{name}"
                );
            }
        }
    }

    for case in vectors["dedup"].as_array().expect("dedup") {
        let name = case["name"].as_str().expect("name");
        let candidate_input = &case["candidate"];
        let candidate = Alert {
            level: level(candidate_input["level"].as_str().expect("level")),
            period: period(case["period"].as_str().expect("period")),
            pair: Pair::UsdToKzt,
            rate_type: RateType::Sell,
            direction: direction(candidate_input["direction"].as_str().expect("direction")),
            change_percent: 1.0,
            old_rate: 100.0,
            new_rate: candidate_input["rate"].as_f64().expect("rate"),
        };
        let last = case.get("last").map(|last| SentAlert {
            key: candidate.key(),
            level: level(last["level"].as_str().expect("level")),
            direction: direction(last["direction"].as_str().expect("direction")),
            rate_at_alert: last["rate"].as_f64().expect("rate"),
            change_percent: 0.0,
            sent_at: None,
        });
        let send = should_send(&candidate, last.as_ref(), &candidate.period);
        assert_eq!(send, case["send"].as_bool().expect("send"), "{name}");
    }

    for case in vectors["checks"].as_array().expect("checks") {
        let name = case["name"].as_str().expect("name");
        let current = case["current"].as_f64().expect("current");
        let historical = case["historical"].as_f64().expect("historical");
        let rate = ExchangeRate {
            buy: current,
            sell: current,
        };
        let current_rates = Rates {
            usd_to_kzt: rate,
            rub_to_kzt: rate,
        };
        let old = ExchangeRate {
            buy: historical,
            sell: historical,
        };
        let record = Record {
            timestamp: jiff::Timestamp::UNIX_EPOCH,
            rates: Rates {
                usd_to_kzt: old,
                rub_to_kzt: old,
            },
        };
        let result = plan(
            &current_rates,
            false,
            |period| (period.name == "HOUR").then_some(record),
            |_| None,
        );
        assert_eq!(
            result.alerts.len(),
            usize::try_from(case["alerts"].as_u64().expect("alerts")).expect("count"),
            "{name}"
        );
        assert_eq!(
            result.clears.len(),
            usize::try_from(case["clears"].as_u64().expect("clears")).expect("count"),
            "{name}"
        );
    }
}

#[test]
fn format_vectors() {
    let vectors = read_json("format.json");
    for case in vectors["cases"].as_array().expect("cases") {
        let name = case["name"].as_str().expect("name");
        let alerts: Vec<Alert> = case["alerts"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .map(|item| Alert {
                        level: level(item["level"].as_str().expect("level")),
                        period: period(item["period"].as_str().expect("period")),
                        pair: pair(item["pair"].as_str().expect("pair")),
                        rate_type: rate_type(item["rate_type"].as_str().expect("rate_type")),
                        direction: direction(item["direction"].as_str().expect("direction")),
                        change_percent: item["change_percent"].as_f64().expect("change_percent"),
                        old_rate: item["old_rate"].as_f64().expect("old_rate"),
                        new_rate: item["new_rate"].as_f64().expect("new_rate"),
                    })
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(
            format(&rates(&case["rates"]), &alerts),
            case["text"].as_str().expect("text"),
            "{name}"
        );
    }
}

#[test]
fn messages_vectors() {
    let vectors = read_json("messages.json");
    assert_eq!(texts::ROOT_BODY, vectors["root"].as_str().expect("root"));
    assert_eq!(
        texts::START_MESSAGE,
        vectors["start"].as_str().expect("start")
    );
    assert_eq!(texts::HELP_MESSAGE, vectors["help"].as_str().expect("help"));
    assert_eq!(
        texts::trigger_failure("upstream down"),
        vectors["trigger_failure"]
            .as_str()
            .expect("trigger_failure")
    );
}

#[test]
fn schedule_vectors() {
    let vectors = read_json("schedule.json");
    for case in vectors["hourly"].as_array().expect("hourly") {
        let name = case["name"].as_str().expect("name");
        let now = case["now"]
            .as_str()
            .expect("now")
            .parse::<jiff::Timestamp>()
            .expect("now");
        let zone = jiff::tz::TimeZone::get(case["tz"].as_str().expect("tz")).expect("tz");
        let minute = u8::try_from(case["minute"].as_u64().expect("minute")).expect("minute");
        let result = next_hourly(now, &zone, minute).unwrap_or_else(|| panic!("{name}: no next"));
        assert_eq!(
            rfc3339_zoned(&result),
            case["next"].as_str().expect("next"),
            "{name}"
        );
    }
    for case in vectors["weekly"].as_array().expect("weekly") {
        let name = case["name"].as_str().expect("name");
        let now = case["now"]
            .as_str()
            .expect("now")
            .parse::<jiff::Timestamp>()
            .expect("now");
        let zone = jiff::tz::TimeZone::get(case["tz"].as_str().expect("tz")).expect("tz");
        let weekday = match case["weekday"].as_str().expect("weekday") {
            "Sunday" => 0,
            "Monday" => 1,
            "Tuesday" => 2,
            "Wednesday" => 3,
            "Thursday" => 4,
            "Friday" => 5,
            "Saturday" => 6,
            other => panic!("unknown weekday {other}"),
        };
        let hour = u8::try_from(case["hour"].as_u64().expect("hour")).expect("hour");
        let result =
            next_weekly(now, &zone, weekday, hour).unwrap_or_else(|| panic!("{name}: no next"));
        assert_eq!(
            rfc3339_zoned(&result),
            case["next"].as_str().expect("next"),
            "{name}"
        );
    }
}
