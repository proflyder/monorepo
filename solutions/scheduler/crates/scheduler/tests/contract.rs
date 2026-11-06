//! Replays the frozen golden HTTP fixtures against the Rust Connect server.
//!
//! The fixtures live in `solutions/scheduler/testdata/contract`; they were
//! captured from the former Go implementation and are now frozen data.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use home_contracttest::Options;
use scheduler::api::{self, AlarmFacade, EngineStatus, Service};
use scheduler::domain::Clock;
use scheduler::store::{Config, Store};

const TOKEN: &str = "contract-test-token";
const BASE: i64 = 1_767_268_800;

#[derive(Debug, Clone)]
struct TestClock(Arc<Mutex<i64>>);

impl Clock for TestClock {
    fn now_unix(&self) -> i64 {
        *self.0.lock().expect("clock lock")
    }
}

struct Env {
    store: Arc<Store>,
    clock: Arc<Mutex<i64>>,
}

static ENVS: OnceLock<Mutex<HashMap<String, Arc<Env>>>> = OnceLock::new();

fn envs() -> &'static Mutex<HashMap<String, Arc<Env>>> {
    ENVS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn fixtures() -> PathBuf {
    std::env::var_os("CARGO_MANIFEST_DIR").map_or_else(
        || PathBuf::from("solutions/scheduler/testdata/contract"),
        |manifest| PathBuf::from(manifest).join("../../testdata/contract"),
    )
}

#[tokio::test]
async fn golden_contract_fixtures() {
    let passed = home_contracttest::run(
        fixtures(),
        new_router,
        Options::default().with_before_step(before_step),
    )
    .await
    .expect("contract fixtures");
    assert_eq!(passed, 21);
}

fn new_router(scenario: &str) -> axum::Router {
    let store = Arc::new(
        Store::open_in_memory(Config {
            max_attempts: 2,
            ..Config::default()
        })
        .expect("open store"),
    );
    let clock = Arc::new(Mutex::new(BASE));
    envs().lock().expect("env lock").insert(
        scenario.to_owned(),
        Arc::new(Env {
            store: Arc::clone(&store),
            clock: Arc::clone(&clock),
        }),
    );
    seed(scenario, &store);
    let allowed = if scenario == "auth_and_limits" {
        vec!["home-1".to_owned()]
    } else {
        Vec::new()
    };
    let engine = Arc::new(|| EngineStatus {
        ready: true,
        ..EngineStatus::default()
    });
    let service = Service::new(store, Arc::new(TestClock(clock)), "test", engine, allowed);
    api::router(service.clone(), AlarmFacade::new(service), TOKEN)
}

fn seed(scenario: &str, store: &Store) {
    if scenario != "timer_limit" {
        return;
    }
    for index in 0..64 {
        store
            .start_timer(&format!("seed-{index:02}"), 600, "", BASE)
            .expect("seed timer");
    }
}

fn before_step(scenario: &str, step: usize) {
    let env = envs()
        .lock()
        .expect("env lock")
        .get(scenario)
        .cloned()
        .expect("contract environment");
    let set = |unix: i64| {
        *env.clock.lock().expect("clock lock") = unix;
    };
    let tick = |unix: i64| {
        set(unix);
        env.store.tick(unix).expect("tick");
    };
    match scenario {
        "tick_occurrence_fire" => match step {
            2 => tick(1_767_339_000),
            4 => {
                let now = *env.clock.lock().expect("clock lock");
                tick(now);
            }
            _ => {}
        },
        "tick_missed_grace" => {
            if step == 2 {
                tick(1_767_339_601);
            }
        }
        "ring_window_expiry_stop" => match step {
            2 => tick(1_767_339_000),
            3 | 4 => tick(1_767_339_901),
            _ => {}
        },
        "snooze_flow" => match step {
            2 => tick(1_767_339_000),
            7 => tick(1_767_425_400),
            _ => {}
        },
        "stop_after_delivered" | "update_during_ring" => {
            if step == 2 {
                tick(1_767_339_000);
            }
        }
        "timer_lifecycle" => match step {
            4 => set(1_767_268_900),
            5 => set(1_767_268_950),
            _ => {}
        },
        "task_lifecycle" => {
            if step == 9 {
                tick(1_767_268_861);
            }
        }
        "outbox_lease_expiry" => match step {
            3 => set(1_767_268_831),
            4 => set(1_767_268_862),
            _ => {}
        },
        _ => {}
    }
}
