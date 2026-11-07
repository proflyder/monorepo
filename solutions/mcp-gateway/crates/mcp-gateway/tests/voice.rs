//! Golden replays of `/v1/voice/*` plus a real `voice-proxy` client.
//!
//! The eight files in `internal/voice/testdata/contract` are replayed against
//! this crate's voice router through `home-contracttest` with a fake bb
//! dispatcher and a deterministic clock. The final test drives the real
//! `voice_proxy::services::Gateway` over TCP and checks the `completedSeq`
//! idempotency key the proxy depends on.

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use jiff::Timestamp;
use mcp_gateway::bb::SpawnAction;
use mcp_gateway::bb::{
    BbApi, BbFuture, Error as BbError, SendRequest, SendResult, SpawnRequest, SpawnResult,
    StatusResult, Targets, TurnResultRequest, TurnResultResult, UpdatesRequest, UpdatesResult,
};
use mcp_gateway::domain::clock::{Clock, FixedClock};
use mcp_gateway::domain::state::{AgentMode, KeyedStore, PendingStore};
use mcp_gateway::metrics::Metrics;
use mcp_gateway::server;
use mcp_gateway::tools::{Deps, TimerScheduler, ToolGroup};
use mcp_gateway::voice;
use voice_contract::Route;
use voice_proxy::domain::profile::Profile;
use voice_proxy::services::Gateway;

/// Starts at the fixture clock and advances one second per read.
struct StepClock {
    nanos: AtomicI64,
}

impl StepClock {
    fn new() -> Self {
        Self {
            nanos: AtomicI64::new(
                "2026-11-01T12:00:00Z"
                    .parse::<Timestamp>()
                    .expect("clock")
                    .as_nanosecond() as i64,
            ),
        }
    }
}

impl Clock for StepClock {
    fn now(&self) -> Timestamp {
        let previous = self.nanos.fetch_add(1_000_000_000, Ordering::SeqCst);
        Timestamp::from_nanosecond(i128::from(previous) + 1_000_000_000).expect("clock")
    }
}

struct OnceLimiter {
    remaining: AtomicUsize,
}

impl voice::Limiter for OnceLimiter {
    fn allow(&self) -> bool {
        self.remaining
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
                value.checked_sub(1)
            })
            .is_ok()
    }
}

#[derive(Default)]
struct FakeVoice {
    sends: Mutex<Vec<SendRequest>>,
    send_error: Mutex<Option<BbError>>,
    turns: Mutex<VecDeque<TurnResultResult>>,
    turn_error: Mutex<Option<BbError>>,
    updates: Mutex<VecDeque<UpdatesResult>>,
    updates_error: Mutex<Option<BbError>>,
}

impl BbApi for FakeVoice {
    fn targets(&self) -> BbFuture<'_, Result<Targets, BbError>> {
        Box::pin(async move { Ok(Targets::default()) })
    }

    fn status(&self, _limit: usize) -> BbFuture<'_, Result<StatusResult, BbError>> {
        Box::pin(async move { Ok(StatusResult::default()) })
    }

    fn validate_spawn(&self, _request: SpawnRequest) -> BbFuture<'_, Result<(), BbError>> {
        Box::pin(async move { Ok(()) })
    }

    fn spawn(&self, _request: SpawnRequest) -> BbFuture<'_, Result<SpawnResult, BbError>> {
        Box::pin(async move { Ok(SpawnResult::default()) })
    }

    fn send(&self, request: SendRequest) -> BbFuture<'_, Result<SendResult, BbError>> {
        Box::pin(async move {
            self.sends.lock().expect("send").push(request);
            if let Some(error) = self.send_error.lock().expect("send").clone() {
                return Err(error);
            }
            Ok(SendResult {
                thread_id: self
                    .sends
                    .lock()
                    .expect("send")
                    .last()
                    .map(|request| request.thread_id.clone())
                    .unwrap_or_default(),
                after_seq: 42,
                ..SendResult::default()
            })
        })
    }

    fn turn_result(
        &self,
        _request: TurnResultRequest,
    ) -> BbFuture<'_, Result<TurnResultResult, BbError>> {
        Box::pin(async move {
            if let Some(error) = self.turn_error.lock().expect("turns").clone() {
                return Err(error);
            }
            let mut turns = self.turns.lock().expect("turns");
            Ok(turns.pop_front().unwrap_or(TurnResultResult {
                status: "running".to_owned(),
                ..TurnResultResult::default()
            }))
        })
    }

    fn updates(&self, _request: UpdatesRequest) -> BbFuture<'_, Result<UpdatesResult, BbError>> {
        Box::pin(async move {
            if let Some(error) = self.updates_error.lock().expect("updates").clone() {
                return Err(error);
            }
            Ok(self
                .updates
                .lock()
                .expect("updates")
                .pop_front()
                .unwrap_or(UpdatesResult {
                    cursor: 0,
                    items: Vec::new(),
                }))
        })
    }
}

struct VoiceWorld {
    fake: Arc<FakeVoice>,
    mode: Arc<AgentMode>,
}

type Worlds = Arc<Mutex<BTreeMap<String, Arc<VoiceWorld>>>>;

fn base_deps(clock: Arc<dyn Clock>) -> Arc<Deps> {
    Arc::new(Deps {
        catalog: None,
        prom: None,
        alerts: None,
        loki: None,
        kube: None,
        report: None,
        checks: None,
        pending: Arc::new(PendingStore::new(Duration::from_secs(90), clock.clone())),
        timers: Arc::new(TimerScheduler),
        metrics: Arc::new(Metrics::new()),
        clock: clock.clone(),
        location: jiff::tz::TimeZone::UTC,
        cooldown: Duration::from_secs(300),
        self_restart_delay: Duration::from_secs(20),
        query_timeout: Duration::from_secs(1),
        status_timeout: Duration::from_secs(2),
        report_timeout: Duration::from_secs(2),
        log_byte_cap: Arc::new(AtomicUsize::new(64 << 10)),
        bb: None,
        bb_targets: None,
        bb_pending: Arc::new(KeyedStore::<SpawnAction>::new(
            Duration::from_secs(90),
            clock,
        )),
        bb_mode: Arc::new(AgentMode::new()),
    })
}

fn voice_options(
    fake: Arc<FakeVoice>,
    mode: Arc<AgentMode>,
    work_thread: &str,
    limiter: Arc<dyn voice::Limiter>,
    clock: Arc<dyn Clock>,
) -> voice::Options {
    let dispatcher: Arc<dyn voice::Dispatcher> = fake;
    voice::Options {
        dispatcher,
        work_thread_id: work_thread.to_owned(),
        mode,
        metrics: Arc::new(Metrics::new()),
        clock,
        limiter,
        poll_interval: Duration::from_millis(1),
        max_wait: Duration::from_secs(1),
        updates_wait: Duration::from_secs(1),
    }
}

fn scenario_router(scenario: &str, worlds: &Worlds) -> Router {
    let fake = Arc::new(FakeVoice::default());
    let mode = Arc::new(AgentMode::new());
    let work_thread = if scenario == "voice_work_missing" {
        ""
    } else {
        "thr_work"
    };
    let limiter: Arc<dyn voice::Limiter> = if scenario == "voice_rate_limit" {
        Arc::new(OnceLimiter {
            remaining: AtomicUsize::new(1),
        })
    } else {
        Arc::new(voice::AllowAll)
    };
    match scenario {
        "voice_result" => {
            let mut turns = fake.turns.lock().expect("turns");
            turns.push_back(TurnResultResult {
                status: "idle".to_owned(),
                spoken: Some("Готово, тесты починены.".to_owned()),
                text: "details".to_owned(),
                completed_seq: Some(77),
                ..TurnResultResult::default()
            });
            turns.push_back(TurnResultResult {
                status: "waiting_input".to_owned(),
                ..TurnResultResult::default()
            });
            turns.push_back(TurnResultResult {
                status: "failed".to_owned(),
                ..TurnResultResult::default()
            });
        }
        "voice_result_errors" => {
            *fake.turn_error.lock().expect("turns") =
                Some(BbError::new("bridge_error", "Сервер агентов недоступен."));
        }
        "voice_updates" => {
            let mut updates = fake.updates.lock().expect("updates");
            updates.push_back(UpdatesResult {
                cursor: 12,
                items: Vec::new(),
            });
            updates.push_back(UpdatesResult {
                cursor: 12,
                items: Vec::new(),
            });
        }
        "voice_updates_errors" => {
            *fake.updates_error.lock().expect("updates") =
                Some(BbError::new("bridge_error", "Сервер агентов недоступен."));
        }
        _ => {}
    }
    worlds.lock().expect("worlds").insert(
        scenario.to_owned(),
        Arc::new(VoiceWorld {
            fake: Arc::clone(&fake),
            mode: Arc::clone(&mode),
        }),
    );

    let clock: Arc<dyn Clock> = Arc::new(StepClock::new());
    let deps = base_deps(Arc::new(FixedClock::new(
        "2026-11-01T12:00:00Z".parse().expect("clock"),
    )));
    let options = voice_options(fake, mode, work_thread, limiter, clock);
    let voice = voice::router(options);
    server::router(server::Options {
        auth_token: "mcp-contract-token".to_owned(),
        voice_token: Some("voice-contract-token".to_owned()),
        voice: Some(voice),
        groups: group_list(deps.clone()),
        deps,
        metrics: Arc::new(Metrics::new()),
        version: "contract".to_owned(),
        ready: true,
    })
}

fn before_step(scenario: &str, step: usize, worlds: &Worlds) {
    let world = worlds.lock().expect("worlds").get(scenario).cloned();
    let Some(world) = world else {
        return;
    };
    match (scenario, step) {
        ("voice_utterance_routes", 7) | ("voice_state", 3) => {
            world.mode.enter("thr_agent");
        }
        ("voice_utterance_routes", 10) => {
            *world.fake.send_error.lock().expect("send") =
                Some(BbError::new("bridge_error", "Сервер агентов недоступен."));
        }
        ("voice_updates", 3) => {
            world
                .fake
                .updates
                .lock()
                .expect("updates")
                .push_back(UpdatesResult {
                    cursor: 42,
                    items: vec![
                        mcp_gateway::bb::UpdateItem {
                            seq: 40,
                            spoken: "Первый ответ.".to_owned(),
                            completed_at: Some(1_700_000_000_000),
                        },
                        mcp_gateway::bb::UpdateItem {
                            seq: 41,
                            spoken: "Второй ответ.".to_owned(),
                            completed_at: None,
                        },
                    ],
                });
        }
        ("voice_state", 5) => world.mode.exit(),
        _ => {}
    }
}

fn fixture_dir(relative: &str) -> std::path::PathBuf {
    let candidate = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative);
    if candidate.is_dir() {
        return candidate;
    }
    std::path::Path::new("solutions/mcp-gateway").join(relative)
}

#[tokio::test]
async fn voice_contract_fixtures() {
    let worlds: Worlds = Arc::new(Mutex::new(BTreeMap::new()));
    let factory_worlds = Arc::clone(&worlds);
    let hook_worlds = Arc::clone(&worlds);
    let scenarios = home_contracttest::run(
        fixture_dir("internal/voice/testdata/contract"),
        move |scenario| scenario_router(scenario, &factory_worlds),
        home_contracttest::Options::default().with_before_step(move |scenario, step| {
            before_step(scenario, step, &hook_worlds);
        }),
    )
    .await
    .expect("voice contract fixtures");
    assert_eq!(scenarios, 8, "replayed {scenarios} scenarios");
}

#[tokio::test]
async fn the_real_voice_proxy_client_sees_completed_seq() {
    let fake = Arc::new(FakeVoice::default());
    fake.turns
        .lock()
        .expect("turns")
        .push_back(TurnResultResult {
            status: "idle".to_owned(),
            spoken: Some("Готово, тесты починены.".to_owned()),
            text: "details".to_owned(),
            completed_seq: Some(77),
            ..TurnResultResult::default()
        });
    let mode = Arc::new(AgentMode::new());
    let clock: Arc<dyn Clock> = Arc::new(FixedClock::new(
        "2026-11-01T12:00:00Z".parse().expect("clock"),
    ));
    let deps = base_deps(Arc::clone(&clock));
    let voice = voice::router(voice_options(
        Arc::clone(&fake),
        Arc::clone(&mode),
        "thr_work",
        Arc::new(voice::AllowAll),
        clock,
    ));
    let app = server::router(server::Options {
        auth_token: "mcp-contract-token".to_owned(),
        voice_token: Some("voice-contract-token".to_owned()),
        voice: Some(voice),
        groups: group_list(deps.clone()),
        deps,
        metrics: Arc::new(Metrics::new()),
        version: "contract".to_owned(),
        ready: true,
    });

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let address = listener.local_addr().expect("address");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });

    let gateway = Gateway::new(
        &format!("http://{address}"),
        Arc::new(|| "voice-contract-token".to_owned()),
        Duration::from_secs(2),
    )
    .expect("client");
    let utterance = gateway
        .utterance("почини тесты", Profile::Work)
        .await
        .expect("utterance");
    assert_eq!(utterance.route, Route::Bb);
    assert_eq!(utterance.thread_id.as_deref(), Some("thr_work"));
    assert_eq!(utterance.after_seq, Some(42));
    assert_eq!(utterance.ok, Some(true));

    let result = gateway.result("thr_work", 42, 1).await.expect("result");
    assert_eq!(result.status, "idle");
    assert_eq!(result.completed_seq, Some(77));
    assert_eq!(result.speech.as_deref(), Some("Готово, тесты починены."));
    assert_eq!(result.text.as_deref(), Some("details"));
}

fn group_list(deps: Arc<Deps>) -> Vec<Arc<dyn ToolGroup>> {
    vec![Arc::new(mcp_gateway::tools::infra_read::ReadGroup::new(
        deps,
    ))]
}
