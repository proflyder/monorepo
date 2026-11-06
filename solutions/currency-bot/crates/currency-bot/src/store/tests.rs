//! Store replay against `testdata/vectors/store.json` and the Go schema dump.

use serde_json::Value;

use super::Store;
use crate::domain::model::{
    AlertKey, Direction, ExchangeRate, Level, Pair, RateType, Rates, Record, SentAlert,
};
use crate::domain::time::rfc3339_nano;
use crate::testdata;

fn read_json(name: &str) -> Value {
    let path = testdata::dir().join("vectors").join(name);
    let bytes =
        std::fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("decode {}: {error}", path.display()))
}

fn timestamp(text: &str) -> jiff::Timestamp {
    text.parse().unwrap_or_else(|_| panic!("timestamp {text}"))
}

fn record(input: &Value) -> Record {
    Record {
        timestamp: timestamp(input["timestamp"].as_str().expect("timestamp")),
        rates: Rates {
            usd_to_kzt: ExchangeRate {
                buy: input["usd_buy"].as_f64().expect("usd_buy"),
                sell: input["usd_sell"].as_f64().expect("usd_sell"),
            },
            rub_to_kzt: ExchangeRate {
                buy: input["rub_buy"].as_f64().expect("rub_buy"),
                sell: input["rub_sell"].as_f64().expect("rub_sell"),
            },
        },
    }
}

fn key(pair: &str, period: &str, rate_type: &str) -> AlertKey {
    AlertKey::new(
        Pair::parse(pair).expect("pair"),
        period,
        RateType::parse(rate_type).expect("rate_type"),
    )
}

fn seeded(vectors: &Value) -> Store {
    let store = Store::open_in_memory().expect("store");
    for input in vectors["records"].as_array().expect("records") {
        store.save_record(&record(input)).expect("save record");
    }
    for input in vectors["sent_alerts"].as_array().expect("sent_alerts") {
        let alert = SentAlert {
            key: key(
                input["pair"].as_str().expect("pair"),
                input["period"].as_str().expect("period"),
                input["rate_type"].as_str().expect("rate_type"),
            ),
            level: Level::parse(input["level"].as_str().expect("level")).expect("level"),
            direction: Direction::parse(input["direction"].as_str().expect("direction"))
                .expect("direction"),
            rate_at_alert: input["rate_at_alert"].as_f64().expect("rate_at_alert"),
            change_percent: input["change_percent"].as_f64().expect("change_percent"),
            sent_at: None,
        };
        store.record_sent_alert(&alert).expect("sent alert");
    }
    store
}

fn timestamps(records: &[Record]) -> Vec<String> {
    records
        .iter()
        .map(|record| rfc3339_nano(record.timestamp))
        .collect()
}

#[test]
fn store_vectors() {
    let vectors = read_json("store.json");

    let store = seeded(&vectors);
    for case in vectors["record_before"].as_array().expect("record_before") {
        let name = case["name"].as_str().expect("name");
        let result = store.record_before(timestamp(case["cutoff"].as_str().expect("cutoff")));
        if case["found"].as_bool().expect("found") {
            let record = result.unwrap_or_else(|error| panic!("{name}: {error}"));
            assert_eq!(
                rfc3339_nano(record.timestamp),
                case["timestamp"].as_str().expect("timestamp"),
                "{name}"
            );
        } else {
            assert!(matches!(result, Err(super::StoreError::NotFound)), "{name}");
        }
    }

    for case in vectors["ranges"].as_array().expect("ranges") {
        let name = case["name"].as_str().expect("name");
        let records = store
            .records_by_date_range(
                timestamp(case["from"].as_str().expect("from")),
                timestamp(case["to"].as_str().expect("to")),
            )
            .expect("range");
        let want: Vec<String> = case["timestamps"]
            .as_array()
            .expect("timestamps")
            .iter()
            .map(|value| value.as_str().expect("timestamp").to_owned())
            .collect();
        assert_eq!(timestamps(&records), want, "{name}");
    }

    let latest = store.latest_record().expect("latest");
    assert_eq!(
        rfc3339_nano(latest.timestamp),
        vectors["latest"].as_str().expect("latest")
    );

    let cleanup_store = seeded(&vectors);
    let cleanup = &vectors["cleanup"];
    let retention = std::time::Duration::from_secs_f64(
        cleanup["retention_hours"]
            .as_f64()
            .expect("retention_hours")
            * 3_600.0,
    );
    let removed = cleanup_store
        .clean_old_records(retention, timestamp(cleanup["now"].as_str().expect("now")))
        .expect("cleanup");
    assert_eq!(
        removed,
        usize::try_from(cleanup["removed"].as_u64().expect("removed")).expect("count")
    );
    let remaining = cleanup_store.all_records().expect("remaining");
    let want: Vec<String> = cleanup["remaining"]
        .as_array()
        .expect("remaining")
        .iter()
        .map(|value| value.as_str().expect("timestamp").to_owned())
        .collect();
    assert_eq!(timestamps(&remaining), want);

    let delete_store = seeded(&vectors);
    assert_eq!(
        delete_store.delete_all().expect("delete"),
        usize::try_from(vectors["delete_count"].as_u64().expect("delete_count")).expect("count")
    );

    let sent_store = seeded(&vectors);
    let sent_key = key("USD_TO_KZT", "HOUR", "SELL");
    let upsert = &vectors["sent_upsert"];
    sent_store
        .record_sent_alert(&SentAlert {
            key: sent_key.clone(),
            level: Level::Critical,
            direction: Direction::Up,
            rate_at_alert: upsert["rate"].as_f64().expect("rate"),
            change_percent: 1.2,
            sent_at: None,
        })
        .expect("upsert");
    let got = sent_store
        .get_sent_alert(&sent_key)
        .expect("get")
        .expect("row");
    assert_eq!(got.level.as_str(), upsert["level"].as_str().expect("level"));
    assert_eq!(got.rate_at_alert, upsert["rate"].as_f64().expect("rate"));
    assert_eq!(
        sent_store.count_sent_alerts().expect("count"),
        usize::try_from(upsert["count"].as_u64().expect("count")).expect("count")
    );
    sent_store.clear_sent_alert(&sent_key).expect("clear");
    assert!(sent_store.get_sent_alert(&sent_key).expect("get").is_none());
    assert!(upsert["cleared"].as_bool().expect("cleared"));
    let other = key("RUB_TO_KZT", "DAY", "BUY");
    assert!(sent_store.get_sent_alert(&other).expect("get").is_some());
    assert!(upsert["other_survives"].as_bool().expect("other_survives"));
}

#[test]
fn a_go_schema_v1_database_opens_in_place() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join("currency-bot.db");
    let schema = std::fs::read_to_string(testdata::dir().join("store").join("schema_v1.sql"))
        .expect("schema_v1.sql");

    {
        // Exactly what the Go binary leaves behind: tables, indexes and rows,
        // without `schema_meta`.
        let connection = rusqlite::Connection::open(&path).expect("create");
        connection.execute_batch(&schema).expect("apply Go schema");
    }

    let store = Store::open(&path).expect("open Go database");
    assert_eq!(store.count_records().expect("records"), 3);
    assert_eq!(store.count_sent_alerts().expect("sent"), 2);
    let records = store.all_records().expect("all");
    assert_eq!(records[0].timestamp.as_millisecond(), 1_793_448_000_000);
    assert_eq!(records[0].rates.usd_to_kzt.buy, 500.1);
    let version = store
        .call(|connection| {
            connection.query_row("SELECT version FROM schema_meta LIMIT 1", [], |row| {
                row.get::<_, i64>(0)
            })
        })
        .expect("version");
    assert_eq!(version, super::SCHEMA_VERSION);

    // The Rust store can write to the same file, and the Go rows survive.
    store
        .save_record(&Record {
            timestamp: timestamp("2026-11-01T00:00:00Z"),
            rates: Rates {
                usd_to_kzt: ExchangeRate {
                    buy: 510.0,
                    sell: 511.0,
                },
                rub_to_kzt: ExchangeRate {
                    buy: 6.0,
                    sell: 6.1,
                },
            },
        })
        .expect("save");
    assert_eq!(store.count_records().expect("records"), 4);
    store.shutdown().expect("shutdown");

    let connection = rusqlite::Connection::open(&path).expect("reopen");
    let rows: i64 = connection
        .query_row("SELECT COUNT(*) FROM currency_history", [], |row| {
            row.get(0)
        })
        .expect("count");
    assert_eq!(rows, 4);
}
