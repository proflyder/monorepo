//! Extensible tool groups and the shared tool-call envelope.
//!
//! A group is a named bundle of MCP tools. The process enables groups from
//! `MCP_GROUPS`, so adding a domain means adding a [`ToolGroup`]
//! implementation and teaching the registry about it — not changing the MCP
//! transport or auth layer.
//!
//! Every tool answers with a single JSON text block
//! `{"speech": "...", "details": ...}` and the same object in
//! `structuredContent`. Backend failures are spoken, never surfaced as
//! protocol errors, exactly like the Go implementation.

pub mod backends;
pub mod bb;
pub mod infra_fix;
pub mod infra_read;
pub mod infra_report;

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::domain::alert;
use crate::domain::catalog::{Catalog, Service};
use crate::domain::clock::Clock;
use crate::domain::speech::speech as normalize_speech;
use crate::domain::state::{AgentMode, KeyedStore, PendingStore};
use crate::domain::status::CheckResult;
use crate::metrics::SharedMetrics;

/// A boxed async result used by the object-safe backend traits.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// One Prometheus sample.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sample {
    /// Series labels.
    pub labels: BTreeMap<String, String>,
    /// Parsed numeric value.
    pub value: f64,
    /// Raw string value.
    pub raw: String,
}

/// One Loki line.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LogLine {
    /// Line text.
    pub text: String,
}

/// Read-only instant PromQL.
pub trait PromQuerier: Send + Sync {
    /// Whether the backend is configured.
    fn configured(&self) -> bool {
        true
    }
    /// Run one instant query.
    fn query<'a>(
        &'a self,
        promql: &'a str,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<Sample>, String>>;
}

/// Active Alertmanager alerts.
pub trait AlertLister: Send + Sync {
    /// Whether the backend is configured.
    fn configured(&self) -> bool {
        true
    }
    /// List active, unsilenced, uninhibited alerts.
    fn active<'a>(&'a self, limit: usize) -> BoxFuture<'a, Result<Vec<alert::Alert>, String>>;
}

/// Loki range queries.
pub trait LogQuerier: Send + Sync {
    /// Whether the backend is configured.
    fn configured(&self) -> bool {
        true
    }
    /// Query one selector.
    fn query<'a>(
        &'a self,
        selector: &'a str,
        since: Duration,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<LogLine>, String>>;
}

/// The Kubernetes subset used by the tools.
pub trait KubeApi: Send + Sync {
    /// Whether the backend is configured.
    fn configured(&self) -> bool {
        true
    }
    /// One workload.
    fn get_workload<'a>(
        &'a self,
        namespace: &'a str,
        kind: &'a str,
        name: &'a str,
    ) -> BoxFuture<'a, Result<crate::kubernetes::Workload, String>>;
    /// Pods, optionally filtered by labels.
    fn list_pods<'a>(
        &'a self,
        namespace: &'a str,
        labels: &'a BTreeMap<String, String>,
    ) -> BoxFuture<'a, Result<Vec<crate::kubernetes::Pod>, String>>;
    /// Recent events.
    fn list_events<'a>(
        &'a self,
        namespace: &'a str,
        object_name: &'a str,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<crate::kubernetes::Event>, String>>;
    /// One container's log tail.
    fn pod_logs<'a>(
        &'a self,
        namespace: &'a str,
        pod: &'a str,
        container: &'a str,
        tail_lines: usize,
        since: Duration,
    ) -> BoxFuture<'a, Result<String, String>>;
    /// Patch the restart annotation.
    fn patch_restart<'a>(
        &'a self,
        namespace: &'a str,
        kind: &'a str,
        name: &'a str,
        at: Timestamp,
    ) -> BoxFuture<'a, Result<(), String>>;
    /// Every Deployment/StatefulSet/DaemonSet in the cluster (report
    /// coverage check).
    fn list_workloads<'a>(
        &'a self,
    ) -> BoxFuture<'a, Result<Vec<crate::kubernetes::WorkloadRef>, String>>;
    /// Flux Kustomizations and HelmReleases in every namespace.
    fn list_flux<'a>(
        &'a self,
    ) -> BoxFuture<'a, Result<Vec<crate::kubernetes::FluxResource>, String>>;
}

/// Executes the typed external probes of every catalog service.
pub trait CheckRunner: Send + Sync {
    /// Whether the backend is configured.
    fn configured(&self) -> bool {
        true
    }
    /// Run every check.
    fn run_all<'a>(&'a self, services: &'a [Service]) -> BoxFuture<'a, Vec<CheckResult>>;
}

/// Delivers a long report out of band.
pub trait ReportSender: Send + Sync {
    /// Whether the backend is configured.
    fn configured(&self) -> bool {
        true
    }
    /// Send one title + text report.
    fn send<'a>(&'a self, title: &'a str, text: &'a str) -> BoxFuture<'a, Result<(), String>>;
}

/// Defers work without blocking a tool call.
pub trait Scheduler: Send + Sync {
    /// Run `task` after `delay`.
    fn after_func(&self, delay: Duration, task: Box<dyn FnOnce() + Send + 'static>);
}

/// The process timers.
#[derive(Debug, Clone, Copy, Default)]
pub struct TimerScheduler;

impl Scheduler for TimerScheduler {
    fn after_func(&self, delay: Duration, task: Box<dyn FnOnce() + Send + 'static>) {
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            task();
        });
    }
}

/// Tool annotations exposed in `tools/list`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Annotations {
    /// The tool does not modify the environment.
    pub read_only: bool,
    /// The tool may perform destructive updates.
    pub destructive: Option<bool>,
    /// The tool may interact with an open world of entities.
    pub open_world: Option<bool>,
}

impl Annotations {
    /// A read-only tool with a closed world.
    #[must_use]
    pub const fn read_only() -> Self {
        Self {
            read_only: true,
            destructive: None,
            open_world: Some(false),
        }
    }

    /// A destructive mutating tool with a closed world.
    #[must_use]
    pub const fn destructive() -> Self {
        Self {
            read_only: false,
            destructive: Some(true),
            open_world: Some(false),
        }
    }
}

/// One tool as the model sees it.
#[derive(Debug, Clone)]
pub struct ToolSpec {
    /// Tool name.
    pub name: &'static str,
    /// Tool description.
    pub description: &'static str,
    /// Hand-written input schema (`serde_json::json!`, never `schemars`).
    pub input_schema: Value,
    /// Behaviour hints.
    pub annotations: Annotations,
}

/// The single tool answer: a short spoken line and structured details.
///
/// Details are serialized once, as pretty JSON, at construction time. Keeping
/// the rendered text (instead of a `serde_json::Value`) preserves the struct
/// field order in the `tools/call` text block: the frozen fixtures compare that
/// block byte-for-byte, and `Value` maps are alphabetically ordered.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutcome {
    /// Normalized spoken line.
    pub speech: String,
    /// Pretty-printed detail text, when present.
    details: Option<String>,
}

impl ToolOutcome {
    /// Build an answer from any serializable detail value.
    #[must_use]
    pub fn new<T: Serialize>(speech: impl Into<String>, details: Option<T>) -> Self {
        Self {
            speech: speech.into(),
            details: details.map(|value| {
                serde_json::to_string_pretty(&value).expect("serializing tool details")
            }),
        }
    }

    /// An answer without details.
    #[must_use]
    pub fn speech_only(speech: impl Into<String>) -> Self {
        Self {
            speech: speech.into(),
            details: None,
        }
    }

    /// Serializes the envelope as the text content of `tools/call`.
    ///
    /// The Go implementation uses `json.MarshalIndent`, which HTML-escapes
    /// `<`, `>`, `&`, U+2028 and U+2029 inside strings. Reproducing that keeps
    /// the frozen tool-call fixtures byte-identical.
    #[must_use]
    pub fn envelope(&self) -> String {
        let speech = serde_json::to_string(&self.speech).expect("serializing speech");
        let text = self.details.as_ref().map_or_else(
            || format!("{{\n  \"speech\": {speech}\n}}"),
            |details| {
                format!(
                    "{{\n  \"speech\": {speech},\n  \"details\": {}\n}}",
                    indent(details, 2)
                )
            },
        );
        go_json_escape(&text)
    }

    /// The same object as `structuredContent`.
    #[must_use]
    pub fn structured(&self) -> Value {
        let mut map = Map::new();
        map.insert("speech".to_owned(), Value::String(self.speech.clone()));
        if let Some(details) = &self.details
            && let Ok(value) = serde_json::from_str::<Value>(details)
        {
            map.insert("details".to_owned(), value);
        }
        Value::Object(map)
    }
}

fn indent(text: &str, spaces: usize) -> String {
    let pad = " ".repeat(spaces);
    let mut out = String::with_capacity(text.len() + spaces * 8);
    for (index, line) in text.lines().enumerate() {
        if index > 0 {
            out.push('\n');
            out.push_str(&pad);
        }
        out.push_str(line);
    }
    out
}

fn go_json_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            other => out.push(other),
        }
    }
    out
}

/// Build a `{"speech", "details"}` outcome with a normalized spoken line.
#[must_use]
pub fn respond<T: Serialize>(speech_text: impl Into<String>, details: Option<T>) -> ToolOutcome {
    ToolOutcome::new(normalize_speech(&speech_text.into()), details)
}

/// A backend failure that is spoken instead of surfaced as a protocol error.
#[must_use]
pub fn failure<T: Serialize>(speech_text: impl Into<String>, details: Option<T>) -> ToolOutcome {
    respond(speech_text, details)
}

/// One named bundle of MCP tools.
pub trait ToolGroup: Send + Sync {
    /// Env-facing group name, for example `infra.read`.
    fn name(&self) -> &'static str;
    /// Every tool in the group.
    fn tools(&self) -> Vec<ToolSpec>;
    /// Execute one tool.
    fn call<'a>(
        &'a self,
        name: &'a str,
        arguments: Value,
    ) -> BoxFuture<'a, Result<ToolOutcome, String>>;
}

/// Everything the tool groups need. Optional backends are `None` when not
/// configured; the tools then answer "not configured" instead of panicking.
pub struct Deps {
    /// Validated catalog.
    pub catalog: Option<Arc<Catalog>>,
    /// Prometheus.
    pub prom: Option<Arc<dyn PromQuerier>>,
    /// Alertmanager.
    pub alerts: Option<Arc<dyn AlertLister>>,
    /// Loki.
    pub loki: Option<Arc<dyn LogQuerier>>,
    /// Kubernetes.
    pub kube: Option<Arc<dyn KubeApi>>,
    /// Telegram.
    pub report: Option<Arc<dyn ReportSender>>,
    /// External checks.
    pub checks: Option<Arc<dyn CheckRunner>>,
    /// Pending restart.
    pub pending: Arc<PendingStore>,
    /// Deferred work.
    pub timers: Arc<dyn Scheduler>,
    /// Metrics.
    pub metrics: SharedMetrics,
    /// Clock.
    pub clock: Arc<dyn Clock>,
    /// Report timezone.
    pub location: TimeZone,
    /// Restart cooldown.
    pub cooldown: Duration,
    /// Delay before a self-disrupting restart.
    pub self_restart_delay: Duration,
    /// Per-backend query timeout.
    pub query_timeout: Duration,
    /// Whole `infra_status` budget.
    pub status_timeout: Duration,
    /// Whole `infra_report` budget.
    pub report_timeout: Duration,
    /// Returned log payload cap.
    pub log_byte_cap: Arc<AtomicUsize>,

    /// bb home-dispatcher bridge.
    pub bb: Option<Arc<dyn crate::bb::BbApi>>,
    /// Personal allow-list.
    pub bb_targets: Option<Arc<crate::bb::BbResolver>>,
    /// Pending agent spawns.
    pub bb_pending: Arc<KeyedStore<crate::bb::SpawnAction>>,
    /// Shared agent mode switch.
    pub bb_mode: Arc<AgentMode>,
}

impl Deps {
    /// Current instant from the injected clock.
    #[must_use]
    pub fn now(&self) -> Timestamp {
        self.clock.now()
    }

    /// The configured Prometheus backend.
    #[must_use]
    pub fn prom(&self) -> Option<&dyn PromQuerier> {
        self.prom.as_deref().filter(|backend| backend.configured())
    }

    /// The configured Alertmanager backend.
    #[must_use]
    pub fn alerts(&self) -> Option<&dyn AlertLister> {
        self.alerts
            .as_deref()
            .filter(|backend| backend.configured())
    }

    /// The configured Loki backend.
    #[must_use]
    pub fn loki(&self) -> Option<&dyn LogQuerier> {
        self.loki.as_deref().filter(|backend| backend.configured())
    }

    /// The configured Kubernetes backend.
    #[must_use]
    pub fn kube(&self) -> Option<&dyn KubeApi> {
        self.kube.as_deref().filter(|backend| backend.configured())
    }

    /// The configured external check runner.
    #[must_use]
    pub fn checks(&self) -> Option<&dyn CheckRunner> {
        self.checks
            .as_deref()
            .filter(|backend| backend.configured())
    }

    /// The configured Telegram sender.
    #[must_use]
    pub fn report(&self) -> Option<&dyn ReportSender> {
        self.report
            .as_deref()
            .filter(|backend| backend.configured())
    }

    /// The configured bb bridge.
    #[must_use]
    pub fn bb(&self) -> Option<&dyn crate::bb::BbApi> {
        self.bb.as_deref().filter(|backend| backend.configured())
    }

    /// Register an action metric.
    pub fn audit(&self, action: &str, target: &str, outcome: &str) {
        self.metrics.observe_action(action, target, outcome);
    }

    /// Resolve a spoken service name.
    pub fn resolve(&self, name: &str) -> Result<&Service, crate::domain::catalog::ResolveError> {
        let catalog = self
            .catalog
            .as_ref()
            .ok_or(crate::domain::catalog::ResolveError::Empty)?;
        catalog.resolve(name)
    }
}

/// Resolves configured group names.
pub struct Registry {
    groups: BTreeMap<&'static str, Arc<dyn ToolGroup>>,
}

impl Registry {
    /// Build a registry; duplicate names are a programming error.
    ///
    /// # Panics
    ///
    /// Panics on an empty or duplicate group name.
    #[must_use]
    pub fn new(groups: Vec<Arc<dyn ToolGroup>>) -> Self {
        let mut by_name = BTreeMap::new();
        for group in groups {
            let name = group.name().trim();
            assert!(!name.is_empty(), "group: empty group name");
            assert!(!by_name.contains_key(name), "group: duplicate group {name}");
            by_name.insert(name, group);
        }
        Self { groups: by_name }
    }

    /// Every known group name, sorted.
    #[must_use]
    pub fn names(&self) -> Vec<&'static str> {
        self.groups.keys().copied().collect()
    }

    /// Resolve enabled group names.
    pub fn select(&self, names: &[String]) -> Result<Vec<Arc<dyn ToolGroup>>, String> {
        if names.is_empty() {
            return Err(format!(
                "no tool groups enabled; set MCP_GROUPS to one or more of {}",
                self.names().join(", ")
            ));
        }
        let mut seen = BTreeMap::new();
        let mut selected = Vec::new();
        for raw in names {
            let name = raw.trim();
            if name.is_empty() || seen.contains_key(name) {
                continue;
            }
            let group = self.groups.get(name).ok_or_else(|| {
                format!(
                    "unknown tool group {name:?}; known groups: {}",
                    self.names().join(", ")
                )
            })?;
            seen.insert(name, ());
            selected.push(Arc::clone(group));
        }
        if selected.is_empty() {
            return Err(format!(
                "no tool groups enabled; set MCP_GROUPS to one or more of {}",
                self.names().join(", ")
            ));
        }
        Ok(selected)
    }
}

/// A `{}` schema for tools without arguments.
#[must_use]
pub fn empty_schema() -> Value {
    json!({"type": "object", "additionalProperties": false})
}

/// Build a schema with a single required string property.
#[must_use]
pub fn required_string(description: &str) -> Value {
    let mut properties = Map::new();
    properties.insert(
        "name".to_owned(),
        json!({"type": "string", "description": description}),
    );
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": properties,
        "required": ["name"],
    })
}
