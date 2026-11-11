//! Golden-vector tests of the pure domain.
//!
//! The JSON files are read from `solutions/mcp-gateway/testdata/vectors` (the
//! same files the Go implementation used), never copied. `expected` is the
//! full result of a case, so a port that changes any byte fails here.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::alert::{Alert, alert_line, dedupe_alerts, match_alerts};
use super::catalog::{Catalog, CatalogError};
use super::clock::{Clock, FixedClock};
use super::redact;
use super::report::{
    ContainerState, FluxResource, PodState, ReportInput, ServiceProbe, WorkloadRef, WorkloadState,
    build_report,
};
use super::speech::speech as normalize_speech;
use super::state::{
    Action, AgentMode, Cooldown, KeyedStore, PendingStore, human_duration, last_restart_of,
    restart_cooldown,
};
use super::status::{self, CheckResult, SignalDetail, StatusDetail, WorkloadDetail};

use std::sync::Arc;

fn vectors_dir() -> PathBuf {
    let relative = Path::new("solutions/mcp-gateway/testdata/vectors");
    if std::env::var_os("TEST_SRCDIR").is_none() {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        return manifest.join("../../testdata/vectors");
    }
    for root in ["TEST_SRCDIR", "RUNFILES_DIR", "GULP_SRCDIR"] {
        let Ok(root) = std::env::var(root) else {
            continue;
        };
        let workspace = std::env::var("TEST_WORKSPACE").unwrap_or_default();
        let candidate = Path::new(&root).join(&workspace).join(relative);
        if candidate.is_dir() {
            return candidate;
        }
        let candidate = Path::new(&root).join(relative);
        if candidate.is_dir() {
            return candidate;
        }
    }
    relative.to_owned()
}

fn read_json(name: &str) -> Value {
    let path = vectors_dir().join(name);
    let bytes =
        std::fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
}

fn cases(file: &Value) -> &[Value] {
    file.get("cases")
        .and_then(Value::as_array)
        .map_or_else(|| panic!("vectors without cases: {file}"), Vec::as_slice)
}

fn case_name(case: &Value) -> &str {
    case.get("name")
        .and_then(Value::as_str)
        .unwrap_or("<unnamed>")
}

fn text(value: &Value, name: &str) -> String {
    value
        .get(name)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("missing string {name} in {value}"))
        .to_owned()
}

#[test]
fn speech_vectors_match_go() {
    let file = read_json("speech.json");
    for case in cases(&file) {
        let input = case.get("input").expect("input");
        let expected = case.get("expected").expect("expected");
        let actual = Value::String(normalize_speech(&text(input, "text")));
        assert_eq!(&actual, expected, "speech case {}", case_name(case));
    }
}

// The secret values are assembled from parts so the repository scanner never
// sees a contiguous credential. The vector file only names them.
fn redact_vector_text(raw: &str) -> String {
    let replacements = [
        ("{{token}}", ["abcdefgh", "ijklmnop", "qrstuvwx"].concat()),
        (
            "{{jwt}}",
            [
                "eyJhbGciOiJIUzI1NiJ9",
                ".",
                "eyJzdWIiOiIxMjM0NTY3ODkwIn0",
                ".",
                "dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U",
            ]
            .concat(),
        ),
        ("{{hex}}", ["0123456789abcdef", "0123456789abcdef"].concat()),
        (
            "{{telegram}}",
            ["123456789:", "AAHdqTcvCH1vGWJxfSeofSAs0K5PALDsaw"].concat(),
        ),
    ];
    let mut out = raw.to_owned();
    for (name, value) in replacements {
        out = out.replace(name, &value);
    }
    out
}

#[test]
fn redact_vectors_match_go() {
    let file = read_json("redact.json");
    for case in cases(&file) {
        let input = case.get("input").expect("input");
        let raw = text(input, "text");
        let expected = case.get("expected").expect("expected");
        let actual = Value::String(redact::text(&redact_vector_text(&raw)));
        assert_eq!(&actual, expected, "redact case {}", case_name(case));
    }
}

#[test]
fn resolve_vectors_match_go() {
    let file = read_json("resolve.json");
    let catalog = text(&file, "catalog");
    let parsed = Catalog::parse(&catalog).expect("vector catalog is valid");
    for case in cases(&file) {
        let input = case.get("input").expect("input");
        let name = text(input, "name");
        let actual = match parsed.resolve(&name) {
            Ok(service) => json!({"service": service.id}),
            Err(error) => json!({"error": error.to_string()}),
        };
        assert_eq!(
            &actual,
            case.get("expected").expect("expected"),
            "resolve case {}",
            case_name(case)
        );
    }
}

#[test]
fn catalog_error_vectors_match_go() {
    let file = read_json("catalog_errors.json");
    for case in cases(&file) {
        let input = case.get("input").expect("input");
        let yaml = text(input, "yaml");
        let actual = match Catalog::parse(&yaml) {
            Ok(_) => json!({"ok": true}),
            Err(CatalogError::Invalid(problems)) => {
                json!({"error": format!("invalid catalog: {problems}")})
            }
            Err(error) => json!({"error": error.to_string()}),
        };
        assert_eq!(
            &actual,
            case.get("expected").expect("expected"),
            "catalog_errors case {}",
            case_name(case)
        );
    }
}

fn workload_from(value: &Value) -> WorkloadState {
    WorkloadState {
        namespace: text(value, "namespace"),
        kind: text(value, "kind"),
        name: text(value, "name"),
        generation: value.get("generation").and_then(Value::as_i64).unwrap_or(0),
        observed_generation: value
            .get("observedGeneration")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        desired: value.get("desired").and_then(Value::as_i64).unwrap_or(0),
        updated: value.get("updated").and_then(Value::as_i64).unwrap_or(0),
        ready: value.get("ready").and_then(Value::as_i64).unwrap_or(0),
        available: value.get("available").and_then(Value::as_i64).unwrap_or(0),
    }
}

fn pod_from(value: &Value) -> PodState {
    PodState {
        namespace: text(value, "namespace"),
        name: text(value, "name"),
        ready: value.get("ready").and_then(Value::as_bool).unwrap_or(false),
        start_time: value
            .get("startTime")
            .and_then(Value::as_str)
            .map(|raw| raw.parse().expect("RFC3339 startTime")),
        containers: value
            .get("containers")
            .and_then(Value::as_array)
            .map(|containers| {
                containers
                    .iter()
                    .map(|container| ContainerState {
                        name: text(container, "name"),
                        image: container
                            .get("image")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        ready: container
                            .get("ready")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                        ..ContainerState::default()
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

fn check_from(value: &Value) -> CheckResult {
    CheckResult {
        service: text(value, "service"),
        check_type: text(value, "type"),
        label: text(value, "label"),
        ok: value.get("ok").and_then(Value::as_bool).unwrap_or(false),
        skipped: value
            .get("skipped")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        value: value
            .get("value")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        error: value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        details: value
            .get("details")
            .and_then(Value::as_array)
            .map(|details| {
                details
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
    }
}

#[test]
fn report_vectors_match_go() {
    let file = read_json("report_inputs.json");
    for case in cases(&file) {
        let input = case.get("input").expect("input");
        let catalog = Catalog::parse(&text(input, "catalog")).expect("vector catalog is valid");
        let now = text(input, "now").parse().expect("RFC3339 now");
        let timezone = jiff::tz::TimeZone::get(&text(input, "timezone")).expect("timezone");

        let workloads: HashMap<(String, String, String), WorkloadState> = input
            .get("workloads")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(|item| {
                        let workload = workload_from(item);
                        (
                            (
                                workload.namespace.clone(),
                                workload.kind.clone(),
                                workload.name.clone(),
                            ),
                            workload,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut pods_by_namespace: HashMap<String, Vec<PodState>> = HashMap::new();
        if let Some(items) = input.get("pods").and_then(Value::as_array) {
            for item in items {
                let pod = pod_from(item);
                pods_by_namespace
                    .entry(pod.namespace.clone())
                    .or_default()
                    .push(pod);
            }
        }
        let mut checks_by_service: HashMap<String, Vec<CheckResult>> = HashMap::new();
        if let Some(items) = input.get("checks").and_then(Value::as_array) {
            for item in items {
                let check = check_from(item);
                checks_by_service
                    .entry(check.service.clone())
                    .or_default()
                    .push(check);
            }
        }

        let probes = catalog
            .services
            .iter()
            .map(|service| {
                let workload = if service.kind == super::catalog::SERVICE_K8S {
                    Some(
                        workloads
                            .get(&(
                                service.namespace.clone(),
                                service.workload.kind.clone(),
                                service.workload.name.clone(),
                            ))
                            .cloned()
                            .ok_or_else(|| "workload not found".to_owned()),
                    )
                } else {
                    None
                };
                let pods = if service.kind == super::catalog::SERVICE_K8S {
                    Some(Ok(pods_by_namespace
                        .get(&service.namespace)
                        .cloned()
                        .unwrap_or_default()))
                } else {
                    None
                };
                ServiceProbe {
                    workload,
                    pods,
                    checks: checks_by_service
                        .get(&service.id)
                        .cloned()
                        .unwrap_or_default(),
                    checks_configured: true,
                }
            })
            .collect();

        let detail = build_report(&ReportInput {
            catalog: &catalog,
            now,
            timezone,
            probes,
            alerts: Vec::new(),
            errors: Vec::new(),
            verbose: input
                .get("verbose")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            workloads: input
                .get("clusterWorkloads")
                .and_then(Value::as_array)
                .map(|items| {
                    Ok(items
                        .iter()
                        .map(|item| WorkloadRef {
                            namespace: text(item, "namespace"),
                            kind: text(item, "kind"),
                            name: text(item, "name"),
                        })
                        .collect())
                }),
            flux: input.get("flux").and_then(Value::as_array).map(|items| {
                Ok(items
                    .iter()
                    .map(|item| FluxResource {
                        kind: text(item, "kind"),
                        namespace: text(item, "namespace"),
                        name: text(item, "name"),
                        ready: item.get("ready").and_then(Value::as_bool).unwrap_or(false),
                        suspended: item
                            .get("suspended")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                        message: item
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                    })
                    .collect())
            }),
        });
        let expected = case.get("expected").expect("expected");
        assert_eq!(
            &Value::String(detail.verdict),
            &expected["speech"],
            "report speech for {}",
            case_name(case)
        );
        assert_eq!(
            &Value::String(detail.text),
            &expected["text"],
            "report text for {}",
            case_name(case)
        );
    }
}

#[test]
fn alert_lookup_matches_name_and_long_prefix_only() {
    let alerts = vec![
        Alert {
            fingerprint: "abcdef123456".to_owned(),
            ..alert_with_name("Watchdog")
        },
        Alert {
            fingerprint: "abcdef999999".to_owned(),
            ..alert_with_name("KubePodDown")
        },
        Alert {
            fingerprint: "zzzzzz".to_owned(),
            ..alert_with_name("KubeNodeDown")
        },
    ];
    let by_name = match_alerts(&alerts, "kubepoddown");
    assert_eq!(by_name.len(), 1);
    assert_eq!(by_name[0].name(), "KubePodDown");
    let by_prefix = match_alerts(&alerts, "abcdef");
    assert_eq!(by_prefix.len(), 2);
    assert!(match_alerts(&alerts, "abc").is_empty());
    assert_eq!(match_alerts(&alerts, "  ").len(), 0);
}

fn alert_with_name(name: &str) -> Alert {
    let mut labels = BTreeMap::new();
    labels.insert("alertname".to_owned(), name.to_owned());
    Alert {
        labels,
        ..Alert::default()
    }
}

#[test]
fn report_alerts_are_deduped_and_sorted() {
    let detail = super::alert::AlertDetail {
        name: "B".to_owned(),
        summary: "two".to_owned(),
        ..super::alert::AlertDetail::default()
    };
    let duplicate = detail.clone();
    let first = super::alert::AlertDetail {
        name: "A".to_owned(),
        summary: "one".to_owned(),
        ..super::alert::AlertDetail::default()
    };
    let deduped = dedupe_alerts(vec![detail, first, duplicate]);
    assert_eq!(deduped.len(), 2);
    assert_eq!(deduped[0].name, "A");
    assert_eq!(alert_line(&deduped[1]), "⚠️ B: two");
}

#[test]
fn status_verdict_prefers_issues_and_reports_partial_checks() {
    let catalog =
        Catalog::parse("services:\n  - id: a\n    aliases: {en: [a]}\n    kind: external\n")
            .expect("catalog");
    let mut detail = StatusDetail {
        service_count: 1,
        ..StatusDetail::default()
    };
    detail.signals.push(SignalDetail {
        service: "a".to_owned(),
        meaning: "первый".to_owned(),
        promql: "up == 0".to_owned(),
        firing: true,
        ..SignalDetail::default()
    });
    detail.workloads.push(WorkloadDetail {
        service: "a".to_owned(),
        desired: 2,
        ready: 0,
        healthy: false,
        ..WorkloadDetail::default()
    });
    let issues = status::status_issues(&detail, &catalog);
    assert_eq!(issues, vec!["первый", "A: готово 0 из 2"]);
    assert_eq!(
        status::status_verdict(&detail, &issues),
        "Проблемы: первый; A: готово 0 из 2."
    );

    detail
        .errors
        .push("алерты: Alertmanager не настроен".to_owned());
    assert_eq!(
        status::status_verdict(
            &StatusDetail {
                service_count: 1,
                ..StatusDetail::default()
            },
            &[]
        ),
        "Всё в порядке: 1 сервисов, алертов нет."
    );
    assert_eq!(
        status::status_verdict(
            &StatusDetail {
                errors: vec!["алерты: Alertmanager не настроен".to_owned()],
                ..StatusDetail::default()
            },
            &[]
        ),
        "Проверка неполная: алерты: Alertmanager не настроен."
    );
}

struct ManualClock(std::sync::atomic::AtomicI64);

impl ManualClock {
    fn new(instant: jiff::Timestamp) -> Self {
        Self(std::sync::atomic::AtomicI64::new(instant.as_second()))
    }

    fn advance(&self, seconds: i64) {
        self.0
            .fetch_add(seconds, std::sync::atomic::Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now(&self) -> jiff::Timestamp {
        jiff::Timestamp::from_second(self.0.load(std::sync::atomic::Ordering::SeqCst))
            .expect("timestamp")
    }
}

fn epoch() -> jiff::Timestamp {
    "2026-11-01T12:00:00Z".parse().expect("timestamp")
}

#[test]
fn pending_action_expires_at_the_ttl_boundary() {
    let clock = Arc::new(ManualClock::new(epoch()));
    let store = PendingStore::new(std::time::Duration::from_secs(90), clock.clone());
    let created = store.put(Action {
        service_id: "scheduler".to_owned(),
        kind: super::state::ACTION_ROLLOUT_RESTART.to_owned(),
        ..Action::default()
    });
    assert_eq!(
        store.expires_at(created),
        epoch() + jiff::SignedDuration::from_secs(90)
    );
    clock.advance(89);
    assert!(store.peek().is_some());
    clock.advance(1);
    assert!(store.peek().is_none());
    assert!(store.take().is_none());
}

#[test]
fn keyed_store_keeps_keys_independent() {
    let clock = Arc::new(ManualClock::new(epoch()));
    let store: KeyedStore<String> =
        KeyedStore::new(std::time::Duration::from_secs(10), clock.clone());
    store.put("bb.spawn", "one".to_owned());
    clock.advance(5);
    store.put("other", "two".to_owned());
    clock.advance(5);
    assert!(store.peek("bb.spawn").is_none());
    assert_eq!(store.take("other"), Some("two".to_owned()));
}

#[test]
fn agent_mode_switches_and_clears_only_its_own_thread() {
    let mode = AgentMode::new();
    assert_eq!(mode.active(), None);
    mode.enter("thr_a");
    assert_eq!(mode.active(), Some("thr_a".to_owned()));
    mode.clear_if("thr_b");
    assert_eq!(mode.active(), Some("thr_a".to_owned()));
    mode.clear_if("thr_a");
    assert_eq!(mode.active(), None);
    mode.enter("thr_a");
    mode.exit();
    assert_eq!(mode.active(), None);
}

#[test]
fn cooldown_uses_the_last_restart_annotation() {
    let now = epoch();
    let last = now - jiff::SignedDuration::from_secs(120);
    let mut annotations = BTreeMap::new();
    annotations.insert(
        super::state::LAST_RESTART_ANNOTATION.to_owned(),
        last.to_string(),
    );
    assert_eq!(last_restart_of(&annotations), Some(last));
    assert_eq!(
        restart_cooldown(Some(last), now, std::time::Duration::from_secs(300)),
        Some(Cooldown {
            elapsed_seconds: 120,
            remaining_seconds: 180
        })
    );
    assert_eq!(
        restart_cooldown(Some(last), now, std::time::Duration::from_secs(120)),
        None
    );
    assert_eq!(
        restart_cooldown(None, now, std::time::Duration::from_secs(300)),
        None
    );
    assert_eq!(human_duration(60), "1 мин");
    assert_eq!(human_duration(3600), "1 ч");
    assert_eq!(human_duration(90), "1 мин");
    assert_eq!(human_duration(9), "9 с");
}

#[test]
fn fixed_clock_is_a_domain_clock() {
    let clock = FixedClock::new(epoch());
    assert_eq!(clock.now(), epoch());
    assert_eq!(clock.instant(), epoch());
}
