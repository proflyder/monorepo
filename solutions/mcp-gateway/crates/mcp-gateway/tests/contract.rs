//! Golden contract replay for the MCP surface and the infra tool groups.
//!
//! The format is `packages/rust/home-contracttest`; the frozen
//! `testdata/contract/{server,tools}/*.json` files captured from the former Go
//! implementation are replayed against this server.
//!
//! The `bb_*` calls run against a fake dispatcher; the voice fixtures are
//! replayed in `tests/voice.rs` through the same fake.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use axum::Router;
use home_contracttest::Options;
use jiff::Timestamp;
use mcp_gateway::bb::{
    BbApi, BbFuture, BbResolver, Error as BbError, SendRequest, SendResult, SpawnAction,
    SpawnRequest, SpawnResult, StatusResult, Targets, Thread, TurnResultRequest, TurnResultResult,
    UpdatesRequest, UpdatesResult,
};
use mcp_gateway::domain::alert::Alert;
use mcp_gateway::domain::catalog::{Catalog, Service};
use mcp_gateway::domain::clock::Clock;
use mcp_gateway::domain::state::{AgentMode, KeyedStore, PendingStore};
use mcp_gateway::kubernetes;
use mcp_gateway::metrics::Metrics;
use mcp_gateway::server;
use mcp_gateway::tools::infra_fix::FixGroup;
use mcp_gateway::tools::infra_read::ReadGroup;
use mcp_gateway::tools::infra_report::ReportGroup;
use mcp_gateway::tools::{
    AlertLister, BoxFuture, CheckRunner, Deps, KubeApi, LogLine, LogQuerier, PromQuerier,
    ReportSender, Sample, Scheduler, ToolGroup,
};

const SERVER_CATALOG: &str = r#"
overview: |
  ## Contract topology
  One fake service.
services:
  - id: scheduler
    aliases:
      ru: [планировщик]
      en: [scheduler]
    kind: k8s
    namespace: home
    workload: {kind: Deployment, name: scheduler}
    containers: [scheduler]
    restart:
      allowed: true
      warning: Contract restart warning.
"#;

const CONTRACT_CATALOG: &str = r#"
overview: |
  ## Фикстурная топология
  Один кластер и один внешний узел.
ignore_alerts: [Watchdog]
group_order: [Голос, Дом, Мониторинг]
services:
  - id: scheduler
    title: Планировщик
    group: Мониторинг
    aliases:
      ru: [планировщик, будильники]
      en: [scheduler]
    kind: k8s
    namespace: home
    workload: {kind: Deployment, name: scheduler}
    containers: [scheduler]
    signals:
      - promql: 'scheduler_up{endpoint="scheduler"} == 0'
        meaning: планировщик недоступен
        status: true
    logs: {source: pods, containers: [scheduler]}
    restart:
      allowed: true
      warning: Будильники в ближайшие минуты могут не сработать.
  - id: hermes
    title: Гермес
    group: Голос
    aliases:
      ru: [гермес, ассистент]
      en: [hermes]
    kind: k8s
    namespace: home
    workload: {kind: Deployment, name: hermes}
    containers: [hermes, voice-proxy]
    signals:
      - promql: 'hermes_up{endpoint="hermes"} == 0'
        meaning: гермес недоступен
    logs: {source: loki, selector: '{namespace="home"}'}
    restart:
      allowed: true
      self_disrupting: true
      warning: Текущий разговор прервётся.
  - id: ha
    title: Home Assistant
    group: Дом
    host_title: HAOS VM
    aliases:
      ru: [дом, ха]
      en: [ha]
    kind: external
    signals:
      - promql: 'ha_up{endpoint="ha"} == 0'
        meaning: home assistant недоступен
        status: true
    checks:
      - {type: tcp, label: HA API, host: 192.0.2.28, port: 8123}
      - {type: ha_template, label: Pulsar, template: "{{ states('sensor.pulsar_version') }}"}
    restart: {allowed: false}
  - id: readonly
    title: Readonly
    group: Мониторинг
    aliases:
      en: [readonly]
    kind: k8s
    namespace: home
    workload: {kind: StatefulSet, name: readonly}
    restart: {allowed: false}
"#;

const CONTRACT_CLOCK: &str = "2026-11-01T12:00:00Z";

struct FakeClock {
    now: Mutex<Timestamp>,
}

impl FakeClock {
    fn new() -> Self {
        Self {
            now: Mutex::new(CONTRACT_CLOCK.parse().expect("clock")),
        }
    }

    fn advance(&self, seconds: i64) {
        let mut now = self.now.lock().expect("clock");
        *now = Timestamp::from_second(now.as_second() + seconds).expect("clock");
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Timestamp {
        *self.now.lock().expect("clock")
    }
}

#[derive(Default)]
struct FakeProm {
    results: Mutex<BTreeMap<String, Vec<Sample>>>,
    errs: Mutex<BTreeMap<String, String>>,
    configured: Mutex<bool>,
}

impl FakeProm {
    fn new() -> Self {
        Self {
            configured: Mutex::new(true),
            ..Self::default()
        }
    }

    fn set_result(&self, promql: &str, samples: Vec<Sample>) {
        self.results
            .lock()
            .expect("prom")
            .insert(promql.to_owned(), samples);
    }

    fn clear(&self) {
        self.results.lock().expect("prom").clear();
    }

    fn set_error(&self, promql: &str, error: &str) {
        self.errs
            .lock()
            .expect("prom")
            .insert(promql.to_owned(), error.to_owned());
    }

    fn set_configured(&self, value: bool) {
        *self.configured.lock().expect("prom") = value;
    }
}

impl PromQuerier for FakeProm {
    fn configured(&self) -> bool {
        *self.configured.lock().expect("prom")
    }

    fn query<'a>(
        &'a self,
        promql: &'a str,
        _limit: usize,
    ) -> BoxFuture<'a, Result<Vec<Sample>, String>> {
        Box::pin(async move {
            if let Some(error) = self.errs.lock().expect("prom").get(promql) {
                return Err(error.clone());
            }
            Ok(self
                .results
                .lock()
                .expect("prom")
                .get(promql)
                .cloned()
                .unwrap_or_default())
        })
    }
}

#[derive(Default)]
struct FakeAlerts {
    alerts: Mutex<Vec<Alert>>,
    error: Mutex<Option<String>>,
    configured: Mutex<bool>,
}

impl FakeAlerts {
    fn new() -> Self {
        Self {
            configured: Mutex::new(true),
            ..Self::default()
        }
    }

    fn set(&self, alerts: Vec<Alert>) {
        *self.alerts.lock().expect("alerts") = alerts;
    }

    fn set_configured(&self, value: bool) {
        *self.configured.lock().expect("alerts") = value;
    }
}

impl AlertLister for FakeAlerts {
    fn configured(&self) -> bool {
        *self.configured.lock().expect("alerts")
    }

    fn active<'a>(&'a self, _limit: usize) -> BoxFuture<'a, Result<Vec<Alert>, String>> {
        Box::pin(async move {
            if let Some(error) = self.error.lock().expect("alerts").clone() {
                return Err(error);
            }
            Ok(self.alerts.lock().expect("alerts").clone())
        })
    }
}

#[derive(Default)]
struct FakeLoki {
    lines: Mutex<Vec<LogLine>>,
    error: Mutex<Option<String>>,
}

impl LogQuerier for FakeLoki {
    fn query<'a>(
        &'a self,
        _selector: &'a str,
        _since: std::time::Duration,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<LogLine>, String>> {
        Box::pin(async move {
            if let Some(error) = self.error.lock().expect("loki").clone() {
                return Err(error);
            }
            let mut lines = self.lines.lock().expect("loki").clone();
            if limit > 0 && lines.len() > limit {
                lines.truncate(limit);
            }
            Ok(lines)
        })
    }
}

#[derive(Default)]
struct FakeKube {
    workloads: Mutex<BTreeMap<String, kubernetes::Workload>>,
    pods: Mutex<BTreeMap<String, Vec<kubernetes::Pod>>>,
    events: Mutex<BTreeMap<String, Vec<kubernetes::Event>>>,
    logs: Mutex<BTreeMap<String, String>>,
    errors: Mutex<BTreeMap<String, String>>,
    patches: Mutex<Vec<(String, String, String)>>,
}

fn kube_key(namespace: &str, kind: &str, name: &str) -> String {
    format!("{namespace}/{kind}/{name}")
}

impl FakeKube {
    fn workload(&self, namespace: &str, kind: &str, name: &str) -> Option<kubernetes::Workload> {
        self.workloads
            .lock()
            .expect("kube")
            .get(&kube_key(namespace, kind, name))
            .cloned()
    }

    fn set_workload(&self, workload: kubernetes::Workload) {
        self.workloads.lock().expect("kube").insert(
            kube_key(&workload.namespace, &workload.kind, &workload.name),
            workload,
        );
    }

    fn set_pods(&self, namespace: &str, pods: Vec<kubernetes::Pod>) {
        self.pods
            .lock()
            .expect("kube")
            .insert(namespace.to_owned(), pods);
    }
}

impl KubeApi for FakeKube {
    fn get_workload<'a>(
        &'a self,
        namespace: &'a str,
        kind: &'a str,
        name: &'a str,
    ) -> BoxFuture<'a, Result<kubernetes::Workload, String>> {
        Box::pin(async move {
            let key = kube_key(namespace, kind, name);
            if let Some(error) = self.errors.lock().expect("kube").get(&key) {
                return Err(error.clone());
            }
            self.workload(namespace, kind, name)
                .ok_or_else(|| "workload not found".to_owned())
        })
    }

    fn list_pods<'a>(
        &'a self,
        namespace: &'a str,
        _labels: &'a BTreeMap<String, String>,
    ) -> BoxFuture<'a, Result<Vec<kubernetes::Pod>, String>> {
        Box::pin(async move {
            if let Some(error) = self
                .errors
                .lock()
                .expect("kube")
                .get(&format!("pods/{namespace}"))
            {
                return Err(error.clone());
            }
            Ok(self
                .pods
                .lock()
                .expect("kube")
                .get(namespace)
                .cloned()
                .unwrap_or_default())
        })
    }

    fn list_events<'a>(
        &'a self,
        namespace: &'a str,
        object_name: &'a str,
        _limit: usize,
    ) -> BoxFuture<'a, Result<Vec<kubernetes::Event>, String>> {
        Box::pin(async move {
            Ok(self
                .events
                .lock()
                .expect("kube")
                .get(&format!("{namespace}/{object_name}"))
                .cloned()
                .unwrap_or_default())
        })
    }

    fn pod_logs<'a>(
        &'a self,
        namespace: &'a str,
        pod: &'a str,
        container: &'a str,
        _tail_lines: usize,
        _since: std::time::Duration,
    ) -> BoxFuture<'a, Result<String, String>> {
        Box::pin(async move {
            Ok(self
                .logs
                .lock()
                .expect("kube")
                .get(&kube_key(namespace, pod, container))
                .cloned()
                .unwrap_or_default())
        })
    }

    fn patch_restart<'a>(
        &'a self,
        namespace: &'a str,
        kind: &'a str,
        name: &'a str,
        _at: Timestamp,
    ) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            if let Some(error) = self.errors.lock().expect("kube").get("patch") {
                return Err(error.clone());
            }
            self.patches.lock().expect("kube").push((
                namespace.to_owned(),
                kind.to_owned(),
                name.to_owned(),
            ));
            Ok(())
        })
    }

    fn list_workloads<'a>(&'a self) -> BoxFuture<'a, Result<Vec<kubernetes::WorkloadRef>, String>> {
        // The contract scenarios pin per-service behaviour; an empty cluster
        // listing keeps the report coverage entry out of their totals.
        Box::pin(async move { Ok(Vec::new()) })
    }

    fn list_flux<'a>(&'a self) -> BoxFuture<'a, Result<Vec<kubernetes::FluxResource>, String>> {
        Box::pin(async move { Ok(Vec::new()) })
    }
}

#[derive(Default)]
struct FakeChecks {
    by_service: Mutex<BTreeMap<String, Vec<mcp_gateway::domain::status::CheckResult>>>,
}

impl CheckRunner for FakeChecks {
    fn run_all<'a>(
        &'a self,
        services: &'a [Service],
    ) -> BoxFuture<'a, Vec<mcp_gateway::domain::status::CheckResult>> {
        Box::pin(async move {
            let by_service = self.by_service.lock().expect("checks");
            let mut out = Vec::new();
            for service in services {
                if let Some(results) = by_service.get(&service.id) {
                    out.extend(results.iter().cloned());
                }
            }
            out
        })
    }
}

#[derive(Default)]
struct FakeReport {
    title: Mutex<String>,
    text: Mutex<String>,
    error: Mutex<Option<String>>,
}

impl ReportSender for FakeReport {
    fn send<'a>(&'a self, title: &'a str, text: &'a str) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            if let Some(error) = self.error.lock().expect("report").clone() {
                return Err(error);
            }
            title.clone_into(&mut self.title.lock().expect("report"));
            text.clone_into(&mut self.text.lock().expect("report"));
            Ok(())
        })
    }
}

struct FakeTimers;

impl Scheduler for FakeTimers {
    fn after_func(&self, _delay: std::time::Duration, _task: Box<dyn FnOnce() + Send + 'static>) {}
}

const CONTRACT_TARGETS: &str = r#"
targets:
  - project: proj_personal
    aliases: ["стан лэб", "station lab"]
    provider: claude-code
    title_prefix: "Голос"
  - project: proj_work
    aliases: ["работа"]
"#;

#[derive(Default)]
struct ContractBb {
    targets: Mutex<Option<Targets>>,
    status: Mutex<StatusResult>,
    status_error: Mutex<Option<BbError>>,
    validate_error: Mutex<Option<BbError>>,
    spawn_error: Mutex<Option<BbError>>,
    spawn_result: Mutex<SpawnResult>,
    configured: std::sync::atomic::AtomicBool,
}

impl ContractBb {
    fn new() -> Self {
        Self {
            configured: std::sync::atomic::AtomicBool::new(true),
            spawn_result: Mutex::new(SpawnResult::default()),
            ..Self::default()
        }
    }

    fn live_targets() -> Targets {
        Targets {
            projects: vec![
                mcp_gateway::bb::Target {
                    project_id: "proj_personal".to_owned(),
                    name: "Station Lab".to_owned(),
                    profile: "personal".to_owned(),
                    providers: vec![mcp_gateway::bb::Provider {
                        provider_id: "claude-code".to_owned(),
                        models: vec![mcp_gateway::bb::Model {
                            model: "sonnet".to_owned(),
                            default_reasoning_level: "medium".to_owned(),
                            reasoning_levels: vec![
                                "low".to_owned(),
                                "medium".to_owned(),
                                "high".to_owned(),
                            ],
                            ..mcp_gateway::bb::Model::default()
                        }],
                        ..mcp_gateway::bb::Provider::default()
                    }],
                    ..mcp_gateway::bb::Target::default()
                },
                mcp_gateway::bb::Target {
                    project_id: "proj_work".to_owned(),
                    name: "Work".to_owned(),
                    profile: "work".to_owned(),
                    providers: vec![mcp_gateway::bb::Provider {
                        provider_id: "codex".to_owned(),
                        models: vec![mcp_gateway::bb::Model {
                            model: "gpt".to_owned(),
                            default_reasoning_level: "low".to_owned(),
                            ..mcp_gateway::bb::Model::default()
                        }],
                        ..mcp_gateway::bb::Provider::default()
                    }],
                    ..mcp_gateway::bb::Target::default()
                },
            ],
        }
    }
}

impl BbApi for ContractBb {
    fn configured(&self) -> bool {
        self.configured.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn targets(&self) -> BbFuture<'_, Result<Targets, BbError>> {
        Box::pin(async move {
            Ok(self
                .targets
                .lock()
                .expect("bb")
                .clone()
                .unwrap_or_else(Self::live_targets))
        })
    }

    fn status(&self, _limit: usize) -> BbFuture<'_, Result<StatusResult, BbError>> {
        Box::pin(async move {
            if let Some(error) = self.status_error.lock().expect("bb").clone() {
                return Err(error);
            }
            Ok(self.status.lock().expect("bb").clone())
        })
    }

    fn validate_spawn(&self, _request: SpawnRequest) -> BbFuture<'_, Result<(), BbError>> {
        Box::pin(async move {
            if let Some(error) = self.validate_error.lock().expect("bb").clone() {
                return Err(error);
            }
            Ok(())
        })
    }

    fn spawn(&self, _request: SpawnRequest) -> BbFuture<'_, Result<SpawnResult, BbError>> {
        Box::pin(async move {
            if let Some(error) = self.spawn_error.lock().expect("bb").clone() {
                return Err(error);
            }
            Ok(self.spawn_result.lock().expect("bb").clone())
        })
    }

    fn send(&self, request: SendRequest) -> BbFuture<'_, Result<SendResult, BbError>> {
        Box::pin(async move {
            Ok(SendResult {
                thread_id: request.thread_id,
                ..SendResult::default()
            })
        })
    }

    fn turn_result(
        &self,
        _request: TurnResultRequest,
    ) -> BbFuture<'_, Result<TurnResultResult, BbError>> {
        Box::pin(async move {
            Ok(TurnResultResult {
                status: "idle".to_owned(),
                ..TurnResultResult::default()
            })
        })
    }

    fn updates(&self, _request: UpdatesRequest) -> BbFuture<'_, Result<UpdatesResult, BbError>> {
        Box::pin(async move { Ok(UpdatesResult::default()) })
    }
}

struct World {
    prom: Arc<FakeProm>,
    alerts: Arc<FakeAlerts>,
    loki: Arc<FakeLoki>,
    kube: Arc<FakeKube>,
    checks: Arc<FakeChecks>,
    clock: Arc<FakeClock>,
    log_byte_cap: Arc<std::sync::atomic::AtomicUsize>,
    bb: Arc<ContractBb>,
}

fn ready_workload(namespace: &str, kind: &str, name: &str) -> kubernetes::Workload {
    kubernetes::Workload {
        namespace: namespace.to_owned(),
        kind: kind.to_owned(),
        name: name.to_owned(),
        desired: 1,
        ready: 1,
        available: 1,
        updated: 1,
        generation: 2,
        observed_generation: 2,
        selector: BTreeMap::from([("app.kubernetes.io/name".to_owned(), name.to_owned())]),
        ..kubernetes::Workload::default()
    }
}

fn seed(scenario: &str, world: &World) {
    let kube = &world.kube;
    kube.set_workload(ready_workload("home", "Deployment", "scheduler"));
    kube.set_workload(ready_workload("home", "Deployment", "hermes"));
    kube.set_workload(ready_workload("home", "StatefulSet", "readonly"));
    kube.set_pods(
        "home",
        vec![
            kubernetes::Pod {
                namespace: "home".to_owned(),
                name: "scheduler-1".to_owned(),
                ready: true,
                start_time: Some(
                    time::OffsetDateTime::parse(
                        "2026-10-30T12:00:00Z",
                        &time::format_description::well_known::Rfc3339,
                    )
                    .expect("time"),
                ),
                containers: vec![kubernetes::ContainerStatus {
                    name: "scheduler".to_owned(),
                    image: "ghcr.io/proflyder/scheduler:1.2.3".to_owned(),
                    ready: true,
                    ..kubernetes::ContainerStatus::default()
                }],
                ..kubernetes::Pod::default()
            },
            kubernetes::Pod {
                namespace: "home".to_owned(),
                name: "hermes-7d9f8c6b5-x2x4z".to_owned(),
                ready: true,
                start_time: Some(
                    time::OffsetDateTime::parse(
                        "2026-10-31T12:00:00Z",
                        &time::format_description::well_known::Rfc3339,
                    )
                    .expect("time"),
                ),
                containers: vec![
                    kubernetes::ContainerStatus {
                        name: "hermes".to_owned(),
                        image: "ghcr.io/proflyder/hermes:2.0.0".to_owned(),
                        ready: true,
                        ..kubernetes::ContainerStatus::default()
                    },
                    kubernetes::ContainerStatus {
                        name: "voice-proxy".to_owned(),
                        image: "ghcr.io/proflyder/voice-proxy:1.1.0".to_owned(),
                        ready: true,
                        ..kubernetes::ContainerStatus::default()
                    },
                ],
                ..kubernetes::Pod::default()
            },
        ],
    );
    kube.logs.lock().expect("kube").insert(
        kube_key("home", "scheduler-1", "scheduler"),
        "tick ok\nERROR alarm not fired\ntick ok\n".to_owned(),
    );
    kube.events.lock().expect("kube").insert(
        "home/scheduler".to_owned(),
        vec![kubernetes::Event {
            reason: "Pulled".to_owned(),
            message: "Image pulled".to_owned(),
            last_seen: Some(
                time::OffsetDateTime::parse(
                    "2026-11-01T11:00:00Z",
                    &time::format_description::well_known::Rfc3339,
                )
                .expect("time"),
            ),
            ..kubernetes::Event::default()
        }],
    );
    let ha_checks = vec![
        mcp_gateway::domain::status::CheckResult {
            service: "ha".to_owned(),
            check_type: "tcp".to_owned(),
            label: "HA API".to_owned(),
            ok: true,
            value: "192.0.2.28:8123".to_owned(),
            ..mcp_gateway::domain::status::CheckResult::default()
        },
        mcp_gateway::domain::status::CheckResult {
            service: "ha".to_owned(),
            check_type: "ha_template".to_owned(),
            label: "Pulsar".to_owned(),
            ok: true,
            value: "1.2.3".to_owned(),
            ..mcp_gateway::domain::status::CheckResult::default()
        },
    ];
    world
        .checks
        .by_service
        .lock()
        .expect("checks")
        .insert("ha".to_owned(), ha_checks);
    *world.loki.lines.lock().expect("loki") = vec![
        LogLine {
            text: "hermes started".to_owned(),
        },
        LogLine {
            text: "Authorization: Bearer supersecrettoken123".to_owned(),
        },
    ];
    match scenario {
        "tools_call_bb_status" => {
            *world.bb.status.lock().expect("bb") = StatusResult {
                threads: vec![
                    Thread {
                        id: "thr_personal".to_owned(),
                        title: "Личный тред".to_owned(),
                        project: "Station Lab".to_owned(),
                        project_id: "proj_personal".to_owned(),
                        profile: "personal".to_owned(),
                        status: "running".to_owned(),
                        ..Thread::default()
                    },
                    Thread {
                        id: "thr_work".to_owned(),
                        title: "Рабочий тред".to_owned(),
                        project: "Work".to_owned(),
                        project_id: "proj_work".to_owned(),
                        profile: "work".to_owned(),
                        status: "running".to_owned(),
                        ..Thread::default()
                    },
                ],
            };
        }
        scenario if scenario.starts_with("tools_call_bb_confirm") => {
            *world.bb.spawn_result.lock().expect("bb") = SpawnResult {
                thread_id: "thr_new".to_owned(),
                idempotent: false,
            };
        }
        _ => {}
    }
}

fn contract_alerts() -> Vec<Alert> {
    let starts = |offset: i64| {
        Some(
            Timestamp::from_second(
                CONTRACT_CLOCK
                    .parse::<Timestamp>()
                    .expect("clock")
                    .as_second()
                    - offset,
            )
            .expect("clock"),
        )
    };
    vec![
        Alert {
            fingerprint: "abcdef123456".to_owned(),
            labels: BTreeMap::from([
                ("alertname".to_owned(), "SchedulerDown".to_owned()),
                ("severity".to_owned(), "critical".to_owned()),
                ("deployment".to_owned(), "scheduler".to_owned()),
                ("namespace".to_owned(), "home".to_owned()),
            ]),
            annotations: BTreeMap::from([
                ("summary".to_owned(), "Планировщик недоступен".to_owned()),
                ("description".to_owned(), "Нет метрик 5 минут".to_owned()),
            ]),
            starts_at: starts(600),
        },
        Alert {
            fingerprint: "abcdef999999".to_owned(),
            labels: BTreeMap::from([
                ("alertname".to_owned(), "SchedulerDown".to_owned()),
                ("severity".to_owned(), "warning".to_owned()),
                ("deployment".to_owned(), "scheduler".to_owned()),
                ("namespace".to_owned(), "home".to_owned()),
            ]),
            annotations: BTreeMap::from([(
                "summary".to_owned(),
                "Планировщик недоступен".to_owned(),
            )]),
            starts_at: starts(300),
        },
        Alert {
            fingerprint: "fedcba654321".to_owned(),
            labels: BTreeMap::from([
                ("alertname".to_owned(), "HermesHighLatency".to_owned()),
                ("severity".to_owned(), "warning".to_owned()),
                ("job".to_owned(), "hermes".to_owned()),
                ("namespace".to_owned(), "home".to_owned()),
            ]),
            annotations: BTreeMap::from([("summary".to_owned(), "Гермес тормозит".to_owned())]),
            starts_at: starts(180),
        },
    ]
}

fn mutate(scenario: &str, step: usize, world: &World) {
    let kube = &world.kube;
    match scenario {
        "tools_call_infra_status" => match step {
            4 => world.alerts.set(vec![
                Alert {
                    fingerprint: "abcdef123456".to_owned(),
                    labels: BTreeMap::from([
                        ("alertname".to_owned(), "SchedulerDown".to_owned()),
                        ("severity".to_owned(), "critical".to_owned()),
                        ("deployment".to_owned(), "scheduler".to_owned()),
                    ]),
                    annotations: BTreeMap::from([(
                        "summary".to_owned(),
                        "Планировщик недоступен".to_owned(),
                    )]),
                    starts_at: Some(
                        Timestamp::from_second(
                            CONTRACT_CLOCK.parse::<Timestamp>().unwrap().as_second() - 600,
                        )
                        .unwrap(),
                    ),
                },
                Alert {
                    fingerprint: "watchdog01".to_owned(),
                    labels: BTreeMap::from([
                        ("alertname".to_owned(), "Watchdog".to_owned()),
                        ("severity".to_owned(), "none".to_owned()),
                    ]),
                    annotations: BTreeMap::from([("summary".to_owned(), "Watchdog".to_owned())]),
                    starts_at: Some(
                        Timestamp::from_second(
                            CONTRACT_CLOCK.parse::<Timestamp>().unwrap().as_second() - 3600,
                        )
                        .unwrap(),
                    ),
                },
            ]),
            5 => {
                world.alerts.set(Vec::new());
                world.prom.set_result(
                    r#"scheduler_up{endpoint="scheduler"} == 0"#,
                    vec![Sample {
                        value: 1.0,
                        ..Sample::default()
                    }],
                );
            }
            6 => {
                world.prom.clear();
                kube.set_workload(kubernetes::Workload {
                    namespace: "home".to_owned(),
                    kind: "Deployment".to_owned(),
                    name: "scheduler".to_owned(),
                    desired: 1,
                    ready: 0,
                    available: 0,
                    updated: 1,
                    generation: 3,
                    observed_generation: 2,
                    ..kubernetes::Workload::default()
                });
                kube.set_pods(
                    "home",
                    vec![kubernetes::Pod {
                        namespace: "home".to_owned(),
                        name: "scheduler-2".to_owned(),
                        ready: false,
                        containers: vec![kubernetes::ContainerStatus {
                            name: "scheduler".to_owned(),
                            restart_count: 2,
                            waiting_reason: "CrashLoopBackOff".to_owned(),
                            last_terminated_at: Some(
                                time::OffsetDateTime::parse(
                                    "2026-11-01T11:55:00Z",
                                    &time::format_description::well_known::Rfc3339,
                                )
                                .unwrap(),
                            ),
                            ..kubernetes::ContainerStatus::default()
                        }],
                        ..kubernetes::Pod::default()
                    }],
                );
            }
            7 => {
                world.prom.set_configured(false);
                world.alerts.set_configured(false);
                kube.set_workload(ready_workload("home", "Deployment", "scheduler"));
                kube.set_pods("home", Vec::new());
            }
            8 => {
                world.checks.by_service.lock().expect("checks").insert(
                    "ha".to_owned(),
                    vec![
                        mcp_gateway::domain::status::CheckResult {
                            service: "ha".to_owned(),
                            check_type: "tcp".to_owned(),
                            label: "HA API".to_owned(),
                            ok: false,
                            error: "connection refused".to_owned(),
                            ..mcp_gateway::domain::status::CheckResult::default()
                        },
                        mcp_gateway::domain::status::CheckResult {
                            service: "ha".to_owned(),
                            check_type: "ha_template".to_owned(),
                            label: "Pulsar".to_owned(),
                            ok: true,
                            skipped: true,
                            value: "Home Assistant не настроен".to_owned(),
                            ..mcp_gateway::domain::status::CheckResult::default()
                        },
                    ],
                );
            }
            _ => {}
        },
        "tools_call_infra_service" => {
            if step == 7 {
                kube.errors.lock().expect("kube").insert(
                    kube_key("home", "Deployment", "scheduler"),
                    "unexpected EOF".to_owned(),
                );
            }
        }
        "tools_call_infra_alert" => {
            if step == 3 {
                world.alerts.set(contract_alerts());
            }
        }
        "tools_call_infra_logs" => match step {
            5 => {
                world
                    .log_byte_cap
                    .store(9, std::sync::atomic::Ordering::Relaxed);
                *world.loki.lines.lock().expect("loki") = vec![LogLine {
                    text: "абвгдеж".to_owned(),
                }];
            }
            6 => {
                world
                    .log_byte_cap
                    .store(64 << 10, std::sync::atomic::Ordering::Relaxed);
                *world.loki.lines.lock().expect("loki") = vec![
                    LogLine {
                        text: "hermes started".to_owned(),
                    },
                    LogLine {
                        text: "Authorization: Bearer supersecrettoken123".to_owned(),
                    },
                ];
            }
            7 => kube.set_pods("home", Vec::new()),
            8 => {
                *world.loki.error.lock().expect("loki") = Some("unexpected EOF".to_owned());
            }
            _ => {}
        },
        "tools_call_infra_query" => match step {
            4 => world.prom.clear(),
            5 => world.prom.set_error("up", "unexpected EOF"),
            _ => {}
        },
        "tools_call_infra_report" => match step {
            4 => {
                kube.set_workload(kubernetes::Workload {
                    namespace: "home".to_owned(),
                    kind: "Deployment".to_owned(),
                    name: "scheduler".to_owned(),
                    desired: 1,
                    ready: 0,
                    available: 0,
                    updated: 1,
                    generation: 3,
                    observed_generation: 2,
                    ..kubernetes::Workload::default()
                });
                world.checks.by_service.lock().expect("checks").insert(
                    "ha".to_owned(),
                    vec![mcp_gateway::domain::status::CheckResult {
                        service: "ha".to_owned(),
                        check_type: "tcp".to_owned(),
                        label: "HA API".to_owned(),
                        ok: false,
                        error: "connection refused".to_owned(),
                        ..mcp_gateway::domain::status::CheckResult::default()
                    }],
                );
            }
            5 => {
                kube.set_workload(ready_workload("home", "Deployment", "scheduler"));
                world.checks.by_service.lock().expect("checks").insert(
                    "ha".to_owned(),
                    vec![
                        mcp_gateway::domain::status::CheckResult {
                            service: "ha".to_owned(),
                            check_type: "tcp".to_owned(),
                            label: "HA API".to_owned(),
                            ok: true,
                            value: "192.0.2.28:8123".to_owned(),
                            ..mcp_gateway::domain::status::CheckResult::default()
                        },
                        mcp_gateway::domain::status::CheckResult {
                            service: "ha".to_owned(),
                            check_type: "ha_template".to_owned(),
                            label: "Pulsar".to_owned(),
                            ok: true,
                            value: "1.2.3".to_owned(),
                            ..mcp_gateway::domain::status::CheckResult::default()
                        },
                    ],
                );
            }
            6 => world.alerts.set_configured(false),
            _ => {}
        },
        "tools_call_infra_prepare_restart" => match step {
            5 => kube.set_workload(kubernetes::Workload {
                namespace: "home".to_owned(),
                kind: "Deployment".to_owned(),
                name: "scheduler".to_owned(),
                desired: 1,
                ready: 1,
                available: 1,
                updated: 1,
                generation: 3,
                observed_generation: 2,
                ..kubernetes::Workload::default()
            }),
            6 => kube.set_workload(kubernetes::Workload {
                annotations: BTreeMap::from([(
                    "mcp-gateway/last-restart".to_owned(),
                    "2026-11-01T11:59:00Z".to_owned(),
                )]),
                ..ready_workload("home", "Deployment", "scheduler")
            }),
            7 => kube.set_workload(ready_workload("home", "Deployment", "scheduler")),
            _ => {}
        },
        "tools_call_infra_confirm" if step == 10 => {
            world.clock.advance(180);
        }
        "tools_call_bb_status" => match step {
            4 => *world.bb.status.lock().expect("bb") = StatusResult::default(),
            5 => {
                *world.bb.status_error.lock().expect("bb") =
                    Some(BbError::new("bridge_error", "Сервер агентов недоступен."));
            }
            _ => {}
        },
        "tools_call_bb_prepare_spawn" => match step {
            9 => {
                *world.bb.validate_error.lock().expect("bb") =
                    Some(BbError::new("thread_limit", "Лимит тредов."));
            }
            10 => {
                world
                    .bb
                    .configured
                    .store(false, std::sync::atomic::Ordering::Relaxed);
            }
            _ => {}
        },
        _ => {}
    }
}

fn contract_groups(scenario: &str) -> Vec<String> {
    match scenario {
        "mcp_tools_list_read" => vec!["infra.read".to_owned()],
        "mcp_tools_list_bb_read" => vec!["bb.read".to_owned()],
        "mcp_tools_list_all" => vec![
            "infra.read".to_owned(),
            "infra.fix".to_owned(),
            "infra.report".to_owned(),
            "bb.read".to_owned(),
            "bb.act".to_owned(),
        ],
        other if other.starts_with("tools_call_bb_") => vec![
            "infra.read".to_owned(),
            "bb.read".to_owned(),
            "bb.act".to_owned(),
        ],
        _ => vec![
            "infra.read".to_owned(),
            "infra.fix".to_owned(),
            "infra.report".to_owned(),
        ],
    }
}

type Worlds = Arc<Mutex<BTreeMap<String, Arc<World>>>>;

fn build_deps(catalog: Arc<Catalog>, world: &Arc<World>) -> Arc<Deps> {
    let clock = world.clock.clone();
    Arc::new(Deps {
        catalog: Some(catalog),
        prom: Some(world.prom.clone()),
        alerts: Some(world.alerts.clone()),
        loki: Some(world.loki.clone()),
        kube: Some(world.kube.clone()),
        report: Some(Arc::new(FakeReport::default())),
        checks: Some(world.checks.clone()),
        pending: Arc::new(PendingStore::new(
            std::time::Duration::from_secs(90),
            clock.clone(),
        )),
        timers: Arc::new(FakeTimers),
        metrics: Arc::new(Metrics::new()),
        clock: clock.clone(),
        location: jiff::tz::TimeZone::get("Asia/Almaty").expect("timezone"),
        cooldown: std::time::Duration::from_secs(300),
        self_restart_delay: std::time::Duration::from_secs(20),
        query_timeout: std::time::Duration::from_secs(1),
        status_timeout: std::time::Duration::from_secs(2),
        report_timeout: std::time::Duration::from_secs(2),
        log_byte_cap: world.log_byte_cap.clone(),
        bb: Some(world.bb.clone()),
        bb_targets: Some(Arc::new(
            BbResolver::parse(CONTRACT_TARGETS.as_bytes()).expect("targets"),
        )),
        bb_pending: Arc::new(KeyedStore::<SpawnAction>::new(
            std::time::Duration::from_secs(90),
            clock,
        )),
        bb_mode: Arc::new(AgentMode::new()),
    })
}

fn tools_router(scenario: &str, worlds: &Worlds) -> Router {
    let catalog = Arc::new(Catalog::parse(CONTRACT_CATALOG).expect("catalog"));
    let world = Arc::new(World {
        prom: Arc::new(FakeProm::new()),
        alerts: Arc::new(FakeAlerts::new()),
        loki: Arc::new(FakeLoki::default()),
        kube: Arc::new(FakeKube::default()),
        checks: Arc::new(FakeChecks::default()),
        clock: Arc::new(FakeClock::new()),
        log_byte_cap: Arc::new(std::sync::atomic::AtomicUsize::new(64 << 10)),
        bb: Arc::new(ContractBb::new()),
    });
    seed(scenario, &world);
    worlds
        .lock()
        .expect("worlds")
        .insert(scenario.to_owned(), Arc::clone(&world));

    let deps = build_deps(catalog, &world);
    let all: Vec<Arc<dyn ToolGroup>> = vec![
        Arc::new(ReadGroup::new(Arc::clone(&deps))),
        Arc::new(FixGroup::new(Arc::clone(&deps))),
        Arc::new(ReportGroup::new(Arc::clone(&deps))),
        Arc::new(mcp_gateway::tools::bb::BbReadGroup::new(Arc::clone(&deps))),
        Arc::new(mcp_gateway::tools::bb::BbActGroup::new(Arc::clone(&deps))),
    ];
    let names = contract_groups(scenario);
    let groups: Vec<Arc<dyn ToolGroup>> = all
        .into_iter()
        .filter(|group| names.iter().any(|name| name == group.name()))
        .collect();
    server::router(server::Options {
        auth_token: "mcp-contract-token".to_owned(),
        voice_token: Some("voice-contract-token".to_owned()),
        voice: None,
        groups,
        deps,
        metrics: Arc::new(Metrics::new()),
        version: "contract".to_owned(),
        ready: true,
    })
}

fn server_router(_scenario: &str, _worlds: &Worlds) -> Router {
    let catalog = Arc::new(Catalog::parse(SERVER_CATALOG).expect("catalog"));
    let clock = Arc::new(FakeClock::new());
    let metrics = Arc::new(Metrics::new());
    let deps = Arc::new(Deps {
        catalog: Some(catalog),
        prom: None,
        alerts: None,
        loki: None,
        kube: None,
        report: None,
        checks: None,
        pending: Arc::new(PendingStore::new(
            std::time::Duration::from_secs(90),
            clock.clone(),
        )),
        timers: Arc::new(FakeTimers),
        metrics: Arc::clone(&metrics),
        clock: clock.clone(),
        location: jiff::tz::TimeZone::UTC,
        cooldown: std::time::Duration::from_secs(300),
        self_restart_delay: std::time::Duration::from_secs(20),
        query_timeout: std::time::Duration::from_secs(1),
        status_timeout: std::time::Duration::from_secs(2),
        report_timeout: std::time::Duration::from_secs(2),
        log_byte_cap: Arc::new(std::sync::atomic::AtomicUsize::new(64 << 10)),
        bb: None,
        bb_targets: None,
        bb_pending: Arc::new(KeyedStore::<SpawnAction>::new(
            std::time::Duration::from_secs(90),
            clock,
        )),
        bb_mode: Arc::new(AgentMode::new()),
    });
    let groups: Vec<Arc<dyn ToolGroup>> = vec![Arc::new(ReadGroup::new(deps.clone()))];
    server::router(server::Options {
        auth_token: "mcp-contract-token".to_owned(),
        voice_token: Some("voice-contract-token".to_owned()),
        voice: None,
        groups,
        deps,
        metrics,
        version: "contract".to_owned(),
        ready: true,
    })
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
async fn server_contract_fixtures() {
    let worlds: Worlds = Arc::new(Mutex::new(BTreeMap::new()));
    let factory_worlds = Arc::clone(&worlds);
    let scenarios = home_contracttest::run(
        fixture_dir("testdata/contract/server"),
        move |scenario| server_router(scenario, &factory_worlds),
        Options::default(),
    )
    .await
    .expect("server contract fixtures");
    assert!(scenarios >= 4, "replayed {scenarios} scenarios");
}

#[tokio::test]
async fn tool_contract_fixtures() {
    let worlds: Worlds = Arc::new(Mutex::new(BTreeMap::new()));
    let factory_worlds = Arc::clone(&worlds);
    let hook_worlds = Arc::clone(&worlds);
    let scenarios = home_contracttest::run(
        fixture_dir("testdata/contract/tools"),
        move |scenario| tools_router(scenario, &factory_worlds),
        Options::default().with_before_step(move |scenario, step| {
            let world = hook_worlds.lock().expect("worlds").get(scenario).cloned();
            if let Some(world) = world {
                mutate(scenario, step, &world);
            }
        }),
    )
    .await
    .expect("tool contract fixtures");
    assert!(scenarios >= 17, "replayed {scenarios} scenarios");
}
