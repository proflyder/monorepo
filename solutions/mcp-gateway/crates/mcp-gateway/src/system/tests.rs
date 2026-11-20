use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt as _;
use serde_json::{Value, json};
use tower::ServiceExt as _;

use super::build::{self, Results};
use super::model::{Level, Sections};
use super::{Dashboard, Sources, summary_level};
use crate::domain::alert::Alert;
use crate::push::{Delivery, Device, PushApi};
use crate::tools::{AlertLister, BoxFuture, PromQuerier, Sample};

const HOSTS: &str = include_str!("testdata/hosts.json");
const GUESTS: &str = include_str!("testdata/guests.json");
const K8S: &str = include_str!("testdata/k8s.json");
const STATION: &str = include_str!("testdata/station.json");

fn parse_prometheus(answer: &Value) -> Vec<Sample> {
    assert_eq!(answer["status"], "success");
    answer["data"]["result"]
        .as_array()
        .expect("result array")
        .iter()
        .map(|item| {
            let labels: BTreeMap<String, String> =
                serde_json::from_value(item["metric"].clone()).expect("metric labels");
            let raw = item["value"][1].as_str().expect("value").to_owned();
            Sample {
                labels,
                value: raw.parse().unwrap_or(0.0),
                raw,
            }
        })
        .collect()
}

fn fixture_results(fixture: &str, table: &'static [(&'static str, &'static str)]) -> Results {
    let answers: HashMap<String, Value> = serde_json::from_str(fixture).expect("fixture");
    let mut results = Results::default();
    for (key, _) in table {
        if let Some(answer) = answers.get(*key) {
            results.insert(key, parse_prometheus(answer));
        }
    }
    results
}

struct FixtureProm {
    by_promql: HashMap<&'static str, Vec<Sample>>,
    calls: AtomicUsize,
    fail: bool,
}

impl FixtureProm {
    fn new() -> Self {
        let mut by_promql = HashMap::new();
        for (fixture, table) in [
            (HOSTS, build::HOST_QUERIES),
            (GUESTS, build::GUEST_QUERIES),
            (K8S, build::K8S_QUERIES),
            (STATION, build::STATION_QUERIES),
        ] {
            let answers: HashMap<String, Value> = serde_json::from_str(fixture).expect("fixture");
            for (key, promql) in table {
                if let Some(answer) = answers.get(*key) {
                    by_promql.insert(*promql, parse_prometheus(answer));
                }
            }
        }
        Self {
            by_promql,
            calls: AtomicUsize::new(0),
            fail: false,
        }
    }
}

impl PromQuerier for FixtureProm {
    fn query<'a>(
        &'a self,
        promql: &'a str,
        _limit: usize,
    ) -> BoxFuture<'a, Result<Vec<Sample>, String>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let answer = if self.fail {
            Err("prometheus down".to_owned())
        } else {
            Ok(self.by_promql.get(promql).cloned().unwrap_or_default())
        };
        Box::pin(async move { answer })
    }
}

struct FixtureAlerts;

impl AlertLister for FixtureAlerts {
    fn active<'a>(&'a self, _limit: usize) -> BoxFuture<'a, Result<Vec<Alert>, String>> {
        let alert = |name: &str, severity: &str, summary: &str, start: i64| Alert {
            fingerprint: name.to_owned(),
            labels: BTreeMap::from([
                ("alertname".to_owned(), name.to_owned()),
                ("severity".to_owned(), severity.to_owned()),
            ]),
            annotations: BTreeMap::from([("summary".to_owned(), summary.to_owned())]),
            starts_at: jiff::Timestamp::from_second(start).ok(),
        };
        let alerts = vec![
            alert("Watchdog", "none", "always firing", 1),
            alert("PinsStale", "warning", "pins are stale", 1_795_000_000),
            alert("StationDown", "critical", "station is down", 1_795_100_000),
        ];
        Box::pin(async move { Ok(alerts) })
    }
}

struct FixturePush;

impl PushApi for FixturePush {
    fn devices<'a>(&'a self) -> BoxFuture<'a, Result<Vec<Device>, String>> {
        Box::pin(async {
            Ok(vec![Device {
                id: "pixel".to_owned(),
                name: "Pixel".to_owned(),
                platform: "android".to_owned(),
            }])
        })
    }

    fn find<'a>(
        &'a self,
        _: &'a str,
        _: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Delivery, String>> {
        Box::pin(async { Err("unused".to_owned()) })
    }

    fn notify<'a>(
        &'a self,
        _: &'a str,
        _: &'a str,
        _: Option<&'a str>,
        _: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Delivery, String>> {
        Box::pin(async { Err("unused".to_owned()) })
    }
}

fn dashboard(prom: Arc<FixtureProm>, ttl: Duration) -> Arc<Dashboard> {
    Arc::new(Dashboard::with_ttl(
        Sources {
            prom: Some(prom),
            alerts: Some(Arc::new(FixtureAlerts)),
            push: Some(Arc::new(FixturePush)),
        },
        ttl,
    ))
}

async fn get(app: &Router, path: &str) -> Value {
    let response = app
        .clone()
        .oneshot(Request::get(path).body(Body::empty()).expect("request"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK, "{path}");
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    serde_json::from_slice(&bytes).expect("json")
}

#[test]
fn host_fixture_maps_to_the_contract() {
    let hosts = build::hosts(
        &fixture_results(HOSTS, build::HOST_QUERIES),
        &fixture_results(GUESTS, build::GUEST_QUERIES),
    );
    let value = serde_json::to_value(&hosts).expect("json");
    assert_eq!(
        value["host"],
        json!({
            "name": "pve",
            "up": true,
            "cpuRatio": 0.234,
            "memUsedBytes": 10e9,
            "memTotalBytes": 16e9,
            "diskUsedBytes": 60e9,
            "diskTotalBytes": 100e9,
            "tempCelsius": 61.0,
            "uptimeSeconds": 1_491_534.0
        })
    );
    assert_eq!(value["status"], "ok");
    assert_eq!(value["guestsStatus"], "critical");
    assert_eq!(value["guestsAvailable"], true);
    let order: Vec<&str> = value["guests"]
        .as_array()
        .expect("guests")
        .iter()
        .map(|guest| guest["name"].as_str().expect("name"))
        .collect();
    assert_eq!(order, ["vpn", "haos", "lab-k3s", "bb", "sandbox"]);
    assert_eq!(
        value["guests"][0],
        json!({
            "vmid": 102, "name": "vpn", "kind": "ct", "running": false, "onboot": true,
            "status": "critical", "memTotalBytes": 0.5e9, "diskUsedBytes": 1.2e9,
            "diskTotalBytes": 8e9
        })
    );
    assert_eq!(value["guests"][1]["kind"], "vm");
    assert!(value["guests"][1].get("diskUsedBytes").is_none());
    assert_eq!(value["guests"][4]["status"], "ok");
}

#[test]
fn host_without_exporters_is_unknown() {
    let hosts = build::hosts(&Results::default(), &Results::default());
    let value = serde_json::to_value(&hosts).expect("json");
    assert_eq!(
        value,
        json!({"status": "unknown", "guestsStatus": "unknown", "guestsAvailable": false, "guests": []})
    );
}

#[test]
fn hot_or_full_host_is_degraded() {
    let mut results = fixture_results(HOSTS, build::HOST_QUERIES);
    let sample = |value: f64| Sample {
        labels: BTreeMap::new(),
        value,
        raw: value.to_string(),
    };
    results.insert("temp", vec![sample(90.0)]);
    assert_eq!(build::host(&results).0, Level::Critical);
    results.insert("temp", vec![sample(50.0)]);
    results.insert("disk_available", vec![sample(5e9)]);
    assert_eq!(build::host(&results).0, Level::Warning);
    results.insert("up", vec![sample(0.0)]);
    assert_eq!(build::host(&results).0, Level::Critical);
}

#[test]
fn pods_are_grouped_with_problems_first() {
    let k8s = build::kubernetes(&fixture_results(K8S, build::K8S_QUERIES));
    let value = serde_json::to_value(&k8s).expect("json");
    assert_eq!(value["status"], "critical");
    assert_eq!(value["podCount"], 4);
    assert_eq!(value["problemCount"], 1);
    let names: Vec<&str> = value["namespaces"]
        .as_array()
        .expect("namespaces")
        .iter()
        .map(|namespace| namespace["name"].as_str().expect("name"))
        .collect();
    assert_eq!(names, ["currency-bot", "home-assistant", "monitoring"]);
    assert_eq!(
        value["namespaces"][0]["pods"][0],
        json!({
            "name": "currency-bot-6c", "phase": "Running", "ready": 0, "containers": 1,
            "restarts": 9, "recentRestarts": 3, "reason": "CrashLoopBackOff",
            "status": "critical", "ageSeconds": 600.0
        })
    );
    assert_eq!(value["namespaces"][1]["pods"][0]["name"], "mcp-gateway-5f");
    assert_eq!(value["namespaces"][2]["pods"][0]["memBytes"], 180e6);
}

#[test]
fn kubernetes_without_kube_state_metrics_is_unknown() {
    let value = serde_json::to_value(build::kubernetes(&Results::default())).expect("json");
    assert_eq!(
        value,
        json!({"status": "unknown", "available": false, "podCount": 0, "problemCount": 0, "namespaces": []})
    );
}

#[test]
fn station_and_phones_map_to_the_contract() {
    let devices = build::devices(
        &fixture_results(STATION, build::STATION_QUERIES),
        Some(vec![Device {
            id: "b".to_owned(),
            name: "Pixel".to_owned(),
            platform: "android".to_owned(),
        }]),
    );
    let value = serde_json::to_value(&devices).expect("json");
    assert_eq!(
        value,
        json!({
            "status": "ok",
            "station": {
                "up": true, "status": "ok", "version": "v0.5.1020",
                "model": "Yandex Station Midi", "latencyMs": 32.0,
                "tempCelsius": 58.5, "uptimeSeconds": 4000.0
            },
            "phonesAvailable": true,
            "phones": [{"id": "b", "name": "Pixel", "platform": "android"}]
        })
    );
}

#[test]
fn summary_level_ignores_missing_sources_but_not_alerts() {
    let sections = |level| Sections {
        host: level,
        guests: Level::Unknown,
        k8s: Level::Ok,
        devices: Level::Ok,
    };
    assert_eq!(summary_level(sections(Level::Ok), &[]), Level::Ok);
    assert_eq!(summary_level(sections(Level::Warning), &[]), Level::Warning);
    let all_unknown = Sections {
        host: Level::Unknown,
        guests: Level::Unknown,
        k8s: Level::Unknown,
        devices: Level::Unknown,
    };
    assert_eq!(summary_level(all_unknown, &[]), Level::Unknown);
    let alerts = build::alerts(&[Alert {
        fingerprint: String::new(),
        labels: BTreeMap::from([("severity".to_owned(), "critical".to_owned())]),
        annotations: BTreeMap::new(),
        starts_at: None,
    }]);
    assert_eq!(summary_level(all_unknown, &alerts), Level::Critical);
}

#[tokio::test]
async fn routes_serve_every_section() {
    let prom = Arc::new(FixtureProm::new());
    let app: Router = super::router(dashboard(Arc::clone(&prom), Duration::from_secs(60)));
    let summary = get(&app, "/v1/system/summary").await;
    assert_eq!(summary["status"], "critical");
    assert_eq!(
        summary["sections"],
        json!({"host": "ok", "guests": "critical", "k8s": "critical", "devices": "ok"})
    );
    assert_eq!(summary["alertsAvailable"], true);
    assert_eq!(
        summary["alerts"],
        json!([
            {"name": "StationDown", "severity": "critical", "summary": "station is down", "startedAt": 1_795_100_000},
            {"name": "PinsStale", "severity": "warning", "summary": "pins are stale", "startedAt": 1_795_000_000}
        ])
    );
    assert!(summary["generatedAt"].as_i64().expect("time") > 0);
    assert_eq!(get(&app, "/v1/system/hosts").await["host"]["name"], "pve");
    assert_eq!(get(&app, "/v1/system/k8s").await["podCount"], 4);
    assert_eq!(
        get(&app, "/v1/system/devices").await["phones"][0]["id"],
        "pixel"
    );
}

#[tokio::test]
async fn sections_are_cached_within_the_ttl() {
    let prom = Arc::new(FixtureProm::new());
    let app: Router = super::router(dashboard(Arc::clone(&prom), Duration::from_secs(60)));
    get(&app, "/v1/system/k8s").await;
    let after_first = prom.calls.load(Ordering::SeqCst);
    get(&app, "/v1/system/k8s").await;
    assert_eq!(prom.calls.load(Ordering::SeqCst), after_first);

    let uncached = Arc::new(FixtureProm::new());
    let app: Router = super::router(dashboard(Arc::clone(&uncached), Duration::ZERO));
    get(&app, "/v1/system/k8s").await;
    get(&app, "/v1/system/k8s").await;
    assert_eq!(
        uncached.calls.load(Ordering::SeqCst),
        2 * build::K8S_QUERIES.len()
    );
}

#[tokio::test]
async fn a_failing_prometheus_degrades_to_unknown() {
    let prom = Arc::new(FixtureProm {
        fail: true,
        ..FixtureProm::new()
    });
    let app: Router = super::router(Arc::new(Dashboard::with_ttl(
        Sources {
            prom: Some(prom),
            alerts: None,
            push: None,
        },
        Duration::ZERO,
    )));
    let summary = get(&app, "/v1/system/summary").await;
    assert_eq!(summary["status"], "unknown");
    assert_eq!(summary["alertsAvailable"], false);
    let devices = get(&app, "/v1/system/devices").await;
    assert_eq!(
        devices,
        json!({"status": "unknown", "phonesAvailable": false, "phones": []})
    );
}

#[tokio::test]
async fn server_requires_the_app_bearer() {
    let prom = Arc::new(FixtureProm::new());
    let app = crate::server::router(crate::server::Options {
        auth_token: "mcp".to_owned(),
        voice_token: None,
        voice: None,
        system: Some(("app".to_owned(), dashboard(prom, Duration::from_secs(60)))),
        chat: None,
        groups: Vec::new(),
        deps: Arc::new(empty_deps()),
        metrics: Arc::new(crate::metrics::Metrics::new()),
        version: "test".to_owned(),
        ready: true,
    });
    let status = |token: &'static str| {
        let app = app.clone();
        async move {
            let mut request = Request::get("/v1/system/summary");
            if !token.is_empty() {
                request = request.header("authorization", format!("Bearer {token}"));
            }
            app.oneshot(request.body(Body::empty()).expect("request"))
                .await
                .expect("response")
                .status()
        }
    };
    assert_eq!(status("").await, StatusCode::UNAUTHORIZED);
    assert_eq!(status("mcp").await, StatusCode::UNAUTHORIZED);
    assert_eq!(status("app").await, StatusCode::OK);
}

fn empty_deps() -> crate::tools::Deps {
    use crate::domain::clock::SystemClock;
    use crate::domain::state::{AgentMode, KeyedStore, PendingStore};
    let clock = Arc::new(SystemClock);
    crate::tools::Deps {
        catalog: None,
        prom: None,
        alerts: None,
        loki: None,
        kube: None,
        report: None,
        checks: None,
        pending: Arc::new(PendingStore::new(Duration::from_secs(60), clock.clone())),
        timers: Arc::new(crate::tools::TimerScheduler),
        metrics: Arc::new(crate::metrics::Metrics::new()),
        clock: clock.clone(),
        location: jiff::tz::TimeZone::UTC,
        timezone_name: "UTC".to_owned(),
        cooldown: Duration::from_secs(1),
        self_restart_delay: Duration::ZERO,
        query_timeout: Duration::from_secs(1),
        status_timeout: Duration::from_secs(1),
        report_timeout: Duration::from_secs(1),
        log_byte_cap: Arc::new(AtomicUsize::new(1)),
        bb: None,
        bb_targets: None,
        bb_pending: Arc::new(KeyedStore::new(Duration::from_secs(60), clock)),
        bb_mode: Arc::new(AgentMode::new()),
        scheduler: None,
        push: None,
    }
}

fn golden(name: &str, body: &Value) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/system/testdata/contract")
        .join(format!("{name}.json"));
    let rendered = format!("{}\n", serde_json::to_string_pretty(body).expect("json"));
    if std::env::var_os("UPDATE_SYSTEM_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().expect("dir")).expect("mkdir");
        std::fs::write(&path, &rendered).expect("write golden");
    }
    let expected = match name {
        "summary" => include_str!("testdata/contract/summary.json"),
        "hosts" => include_str!("testdata/contract/hosts.json"),
        "k8s" => include_str!("testdata/contract/k8s.json"),
        _ => include_str!("testdata/contract/devices.json"),
    };
    assert_eq!(rendered, expected, "{name}");
}

#[tokio::test]
async fn responses_match_the_app_contract_fixtures() {
    let prom = Arc::new(FixtureProm::new());
    let app: Router = super::router(dashboard(prom, Duration::from_secs(60)));
    let mut summary = get(&app, "/v1/system/summary").await;
    summary["generatedAt"] = json!(1_795_129_665);
    golden("summary", &summary);
    golden("hosts", &get(&app, "/v1/system/hosts").await);
    golden("k8s", &get(&app, "/v1/system/k8s").await);
    golden("devices", &get(&app, "/v1/system/devices").await);
}
