//! Replay of `solutions/currency-bot/testdata/contract/*.json`.
//!
//! Each scenario gets a fresh in-memory store, a fake rates source and sender;
//! the fixture files are the frozen Go HTTP contract and are not modified.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use axum::Router;
use home_contracttest::{Options as ContractOptions, run};
use home_secret::{Secret, Token};
use jiff::Timestamp;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::api::{
    AlwaysReady, ApiState, NeverReady, StoreReadiness, UpdateDedup, api_router, webhook_router,
};
use crate::domain::clock::FixedClock;
use crate::domain::model::{ExchangeRate, Rates, Record};
use crate::metrics::Metrics;
use crate::pipeline::{BoxFuture, MessageSender, Pipeline, RatesSource};
use crate::store::Store;
use crate::testdata;

struct FakeSource {
    result: Result<Rates, String>,
    calls: AtomicUsize,
}

impl RatesSource for FakeSource {
    fn fetch(&self) -> BoxFuture<'_, Result<Rates, String>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let result = self.result.clone();
        Box::pin(async move { result })
    }
}

#[derive(Default)]
struct FakeSender {
    #[allow(dead_code)]
    sent: Mutex<Vec<(String, String)>>,
}

impl MessageSender for FakeSender {
    fn send<'a>(&'a self, chat_id: &'a str, text: &'a str) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            self.sent
                .lock()
                .expect("lock")
                .push((chat_id.to_owned(), text.to_owned()));
            Ok(())
        })
    }
}

fn rate(usd_buy: f64, usd_sell: f64, rub_buy: f64, rub_sell: f64) -> Rates {
    Rates {
        usd_to_kzt: ExchangeRate {
            buy: usd_buy,
            sell: usd_sell,
        },
        rub_to_kzt: ExchangeRate {
            buy: rub_buy,
            sell: rub_sell,
        },
    }
}

fn contract_records() -> Vec<Record> {
    vec![
        Record {
            timestamp: "2026-10-31T12:00:00Z".parse().expect("time"),
            rates: rate(500.1, 501.2, 5.05, 5.15),
        },
        Record {
            timestamp: "2026-10-31T11:00:00Z".parse().expect("time"),
            rates: rate(499.5, 500.25, 4.99, 5.09),
        },
    ]
}

fn filter_records() -> Vec<Record> {
    vec![
        Record {
            timestamp: "2026-03-19T00:00:00Z".parse().expect("time"),
            rates: rate(503.0, 504.0, 5.1, 5.2),
        },
        Record {
            timestamp: "2026-03-18T12:00:00Z".parse().expect("time"),
            rates: rate(502.0, 503.0, 5.0, 5.1),
        },
        Record {
            timestamp: "2026-03-18T00:00:00Z".parse().expect("time"),
            rates: rate(501.0, 502.0, 4.9, 5.0),
        },
        Record {
            timestamp: "2026-03-17T23:59:59.999Z".parse().expect("time"),
            rates: rate(500.0, 501.0, 4.8, 4.9),
        },
    ]
}

fn new_router(scenario: &str) -> Router {
    let store = Arc::new(Store::open_in_memory().expect("contract store"));
    match scenario {
        "api_history_shape" => {
            for record in contract_records() {
                store.save_record(&record).expect("seed");
            }
        }
        "api_history_filters" => {
            for record in filter_records() {
                store.save_record(&record).expect("seed");
            }
        }
        "api_latest" => {
            store
                .save_record(&Record {
                    timestamp: "2026-10-31T12:00:00Z".parse().expect("time"),
                    rates: rate(509.8, 512.4, 6.21, 6.35),
                })
                .expect("seed");
        }
        "api_delete_history" => {
            for offset in 0..3 {
                store
                    .save_record(&Record {
                        timestamp: Timestamp::from_second(1_793_448_000 + offset).expect("time"),
                        rates: rate(500.0, 501.0, 5.0, 5.1),
                    })
                    .expect("seed");
            }
        }
        _ => {}
    }

    let source: Result<Rates, String> = if scenario == "api_trigger_failure" {
        Err("parse exploded".to_owned())
    } else {
        Ok(rate(485.5, 487.2, 4.85, 4.92))
    };
    let sender = Arc::new(FakeSender::default());
    let sender_trait: Arc<dyn MessageSender> = sender;
    let metrics = Arc::new(Metrics::new());
    let clock: Arc<dyn crate::domain::clock::Clock> =
        Arc::new(FixedClock("2026-10-31T12:00:00Z".parse().expect("now")));
    let pipeline = Arc::new(Pipeline {
        source: Arc::new(FakeSource {
            result: source,
            calls: AtomicUsize::new(0),
        }),
        store: Arc::clone(&store),
        sender: Arc::clone(&sender_trait),
        metrics: Arc::clone(&metrics),
        clock: Arc::clone(&clock),
    });
    let readiness: Arc<dyn crate::api::Readiness> = if scenario == "api_ready_unavailable" {
        Arc::new(NeverReady)
    } else if scenario == "api_health" {
        Arc::new(AlwaysReady)
    } else {
        Arc::new(StoreReadiness::new(Arc::clone(&store)))
    };

    let state = Arc::new(ApiState {
        token: Token::new("test-api-key"),
        webhook_secret: Secret::new("test-secret"),
        allowed_chats: vec!["123".to_owned(), "-100999".to_owned()],
        default_chat: "123".to_owned(),
        pipeline,
        store: Arc::clone(&store),
        sender: Arc::clone(&sender_trait),
        metrics,
        clock,
        readiness,
        tasks: TaskTracker::new(),
        cancel: CancellationToken::new(),
        dedup: Mutex::new(UpdateDedup::default()),
        dispatch_inline: true,
    });

    if scenario.starts_with("webhook_") {
        webhook_router(state)
    } else {
        api_router(state)
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn contract_fixtures() {
    // The fixture directory is repository-relative; Bazel exports TEST_SRCDIR
    // and cargo runs from the package directory.
    let dir = if std::env::var_os("TEST_SRCDIR").is_some() {
        "solutions/currency-bot/testdata/contract".to_owned()
    } else {
        testdata::dir().join("contract").display().to_string()
    };
    let count = run(dir, new_router, ContractOptions::default())
        .await
        .expect("all currency-bot contract fixtures pass");
    assert_eq!(count, 14);
}
