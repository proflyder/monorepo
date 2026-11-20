pub mod backends;
pub mod bb;
pub mod infra_fix;
pub mod infra_read;
pub mod infra_report;
pub mod phone;
pub mod time;

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

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sample {
    pub labels: BTreeMap<String, String>,
    pub value: f64,
    pub raw: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LogLine {
    pub text: String,
}

pub trait PromQuerier: Send + Sync {
    fn configured(&self) -> bool {
        true
    }
    fn query<'a>(
        &'a self,
        promql: &'a str,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<Sample>, String>>;
}

pub trait AlertLister: Send + Sync {
    fn configured(&self) -> bool {
        true
    }
    fn active<'a>(&'a self, limit: usize) -> BoxFuture<'a, Result<Vec<alert::Alert>, String>>;
}

pub trait LogQuerier: Send + Sync {
    fn configured(&self) -> bool {
        true
    }
    fn query<'a>(
        &'a self,
        selector: &'a str,
        since: Duration,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<LogLine>, String>>;
}

pub trait KubeApi: Send + Sync {
    fn configured(&self) -> bool {
        true
    }
    fn get_workload<'a>(
        &'a self,
        namespace: &'a str,
        kind: &'a str,
        name: &'a str,
    ) -> BoxFuture<'a, Result<crate::kubernetes::Workload, String>>;
    fn list_pods<'a>(
        &'a self,
        namespace: &'a str,
        labels: &'a BTreeMap<String, String>,
    ) -> BoxFuture<'a, Result<Vec<crate::kubernetes::Pod>, String>>;
    fn list_events<'a>(
        &'a self,
        namespace: &'a str,
        object_name: &'a str,
        limit: usize,
    ) -> BoxFuture<'a, Result<Vec<crate::kubernetes::Event>, String>>;
    fn pod_logs<'a>(
        &'a self,
        namespace: &'a str,
        pod: &'a str,
        container: &'a str,
        tail_lines: usize,
        since: Duration,
    ) -> BoxFuture<'a, Result<String, String>>;
    fn patch_restart<'a>(
        &'a self,
        namespace: &'a str,
        kind: &'a str,
        name: &'a str,
        at: Timestamp,
    ) -> BoxFuture<'a, Result<(), String>>;
    fn list_workloads<'a>(
        &'a self,
    ) -> BoxFuture<'a, Result<Vec<crate::kubernetes::WorkloadRef>, String>>;
    fn list_flux<'a>(
        &'a self,
    ) -> BoxFuture<'a, Result<Vec<crate::kubernetes::FluxResource>, String>>;
}

pub trait CheckRunner: Send + Sync {
    fn configured(&self) -> bool {
        true
    }
    fn run_all<'a>(&'a self, services: &'a [Service]) -> BoxFuture<'a, Vec<CheckResult>>;
}

pub trait ReportSender: Send + Sync {
    fn configured(&self) -> bool {
        true
    }
    fn send<'a>(&'a self, title: &'a str, text: &'a str) -> BoxFuture<'a, Result<(), String>>;
}

pub trait Scheduler: Send + Sync {
    fn after_func(&self, delay: Duration, task: Box<dyn FnOnce() + Send + 'static>);
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Annotations {
    pub read_only: bool,
    pub destructive: Option<bool>,
    pub open_world: Option<bool>,
}

impl Annotations {
    #[must_use]
    pub const fn read_only() -> Self {
        Self {
            read_only: true,
            destructive: None,
            open_world: Some(false),
        }
    }

    #[must_use]
    pub const fn destructive() -> Self {
        Self {
            read_only: false,
            destructive: Some(true),
            open_world: Some(false),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
    pub annotations: Annotations,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutcome {
    pub speech: String,
    details: Option<String>,
}

impl ToolOutcome {
    #[must_use]
    pub fn new<T: Serialize>(speech: impl Into<String>, details: Option<T>) -> Self {
        Self {
            speech: speech.into(),
            details: details.map(|value| {
                serde_json::to_string_pretty(&value).expect("serializing tool details")
            }),
        }
    }

    #[must_use]
    pub fn speech_only(speech: impl Into<String>) -> Self {
        Self {
            speech: speech.into(),
            details: None,
        }
    }

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

#[must_use]
pub fn respond<T: Serialize>(speech_text: impl Into<String>, details: Option<T>) -> ToolOutcome {
    ToolOutcome::new(normalize_speech(&speech_text.into()), details)
}

#[must_use]
pub fn failure<T: Serialize>(speech_text: impl Into<String>, details: Option<T>) -> ToolOutcome {
    respond(speech_text, details)
}

pub trait ToolGroup: Send + Sync {
    fn name(&self) -> &'static str;
    fn tools(&self) -> Vec<ToolSpec>;
    fn call<'a>(
        &'a self,
        name: &'a str,
        arguments: Value,
    ) -> BoxFuture<'a, Result<ToolOutcome, String>>;
}

pub struct Deps {
    pub catalog: Option<Arc<Catalog>>,
    pub prom: Option<Arc<dyn PromQuerier>>,
    pub alerts: Option<Arc<dyn AlertLister>>,
    pub loki: Option<Arc<dyn LogQuerier>>,
    pub kube: Option<Arc<dyn KubeApi>>,
    pub report: Option<Arc<dyn ReportSender>>,
    pub checks: Option<Arc<dyn CheckRunner>>,
    pub pending: Arc<PendingStore>,
    pub timers: Arc<dyn Scheduler>,
    pub metrics: SharedMetrics,
    pub clock: Arc<dyn Clock>,
    pub location: TimeZone,
    pub timezone_name: String,
    pub scheduler: Option<Arc<dyn crate::scheduler::SchedulerApi>>,
    pub push: Option<Arc<dyn crate::push::PushApi>>,
    pub cooldown: Duration,
    pub self_restart_delay: Duration,
    pub query_timeout: Duration,
    pub status_timeout: Duration,
    pub report_timeout: Duration,
    pub log_byte_cap: Arc<AtomicUsize>,

    pub bb: Option<Arc<dyn crate::bb::BbApi>>,
    pub bb_targets: Option<Arc<crate::bb::BbResolver>>,
    pub bb_pending: Arc<KeyedStore<crate::bb::SpawnAction>>,
    pub bb_mode: Arc<AgentMode>,
}

impl Deps {
    #[must_use]
    pub fn now(&self) -> Timestamp {
        self.clock.now()
    }

    #[must_use]
    pub fn prom(&self) -> Option<&dyn PromQuerier> {
        self.prom.as_deref().filter(|backend| backend.configured())
    }

    #[must_use]
    pub fn alerts(&self) -> Option<&dyn AlertLister> {
        self.alerts
            .as_deref()
            .filter(|backend| backend.configured())
    }

    #[must_use]
    pub fn loki(&self) -> Option<&dyn LogQuerier> {
        self.loki.as_deref().filter(|backend| backend.configured())
    }

    #[must_use]
    pub fn kube(&self) -> Option<&dyn KubeApi> {
        self.kube.as_deref().filter(|backend| backend.configured())
    }

    #[must_use]
    pub fn checks(&self) -> Option<&dyn CheckRunner> {
        self.checks
            .as_deref()
            .filter(|backend| backend.configured())
    }

    #[must_use]
    pub fn report(&self) -> Option<&dyn ReportSender> {
        self.report
            .as_deref()
            .filter(|backend| backend.configured())
    }

    #[must_use]
    pub fn bb(&self) -> Option<&dyn crate::bb::BbApi> {
        self.bb.as_deref().filter(|backend| backend.configured())
    }

    #[must_use]
    pub fn scheduler(&self) -> Option<&dyn crate::scheduler::SchedulerApi> {
        self.scheduler
            .as_deref()
            .filter(|backend| backend.configured())
    }

    #[must_use]
    pub fn push(&self) -> Option<&dyn crate::push::PushApi> {
        self.push.as_deref().filter(|backend| backend.configured())
    }

    pub fn audit(&self, action: &str, target: &str, outcome: &str) {
        self.metrics.observe_action(action, target, outcome);
    }

    pub fn resolve(&self, name: &str) -> Result<&Service, crate::domain::catalog::ResolveError> {
        let catalog = self
            .catalog
            .as_ref()
            .ok_or(crate::domain::catalog::ResolveError::Empty)?;
        catalog.resolve(name)
    }
}

pub struct Registry {
    groups: BTreeMap<&'static str, Arc<dyn ToolGroup>>,
}

impl Registry {
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

    #[must_use]
    pub fn names(&self) -> Vec<&'static str> {
        self.groups.keys().copied().collect()
    }

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

#[must_use]
pub fn empty_schema() -> Value {
    json!({"type": "object", "additionalProperties": false})
}

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
