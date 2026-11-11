//! `infra.read`: status, service, alert, logs, query, overview and report.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::sync::Arc;
use std::time::Duration;

use jiff::Timestamp;
use serde::Serialize;
use serde_json::{Value, json};

use super::{
    Annotations, BoxFuture, Deps, LogLine, Sample, ToolGroup, ToolOutcome, ToolSpec, empty_schema,
    failure, required_string, respond,
};
use crate::domain::alert::AlertDetail;
use crate::domain::catalog::{self, Catalog, Service};
use crate::domain::report::{
    self, ContainerState, PodState, ReportInput, ServiceProbe, WorkloadState,
};
use crate::domain::status::{
    self, CheckResult, PodDetail, SignalDetail, StatusDetail, WorkloadDetail,
};
use crate::domain::{alert, redact};
use crate::kubernetes;

/// The `infra.read` group.
pub struct ReadGroup {
    deps: Arc<Deps>,
}

impl ReadGroup {
    /// Build the group.
    #[must_use]
    pub fn new(deps: Arc<Deps>) -> Self {
        Self { deps }
    }
}

impl ToolGroup for ReadGroup {
    fn name(&self) -> &'static str {
        "infra.read"
    }

    fn tools(&self) -> Vec<ToolSpec> {
        vec![
            ToolSpec {
                name: "infra_status",
                description: "One aggregate infra health check: firing alerts, catalog health signals, Kubernetes workload availability and recent pod restarts. Returns a short Russian speech verdict plus structured details.",
                input_schema: empty_schema(),
                annotations: Annotations::read_only(),
            },
            ToolSpec {
                name: "infra_service",
                description: "Detail for one catalog service resolved by spoken name (case and ё-insensitive): workload status, pods, restarts, recent events, Prometheus signals and related alerts.",
                input_schema: required_string("spoken service name from the catalog"),
                annotations: Annotations::read_only(),
            },
            ToolSpec {
                name: "infra_alert",
                description: "Explain one active alert by Alertmanager fingerprint (full or a prefix of at least 6 characters) or by alert name. Returns its labels and annotations plus the matching catalog service detail, so a follow-up question about an alert can be answered.",
                input_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "ref": {
                            "type": "string",
                            "description": "Alertmanager fingerprint (full or a prefix of at least 6 characters) or the alert name",
                        }
                    },
                    "required": ["ref"],
                }),
                annotations: Annotations::read_only(),
            },
            ToolSpec {
                name: "infra_logs",
                description: "Recent logs for any catalog service: Loki when the service is ingested there, otherwise Kubernetes pods/log. Secret-like values are redacted before returning.",
                input_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "name": {"type": "string", "description": "spoken service name from the catalog"},
                        "since": {"type": "string", "description": "how far back to read, for example 15m or 1h (default 15m)"},
                        "grep": {"type": "string", "description": "case-insensitive substring filter"},
                        "limit": {"type": "integer", "description": "maximum number of log lines (default 50)"},
                    },
                    "required": ["name"],
                }),
                annotations: Annotations::read_only(),
            },
            ToolSpec {
                name: "infra_query",
                description: "Run one read-only instant PromQL query. Results are capped; this tool cannot change anything.",
                input_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "promql": {"type": "string", "description": "read-only instant PromQL expression"},
                    },
                    "required": ["promql"],
                }),
                annotations: Annotations::read_only(),
            },
            ToolSpec {
                name: "infra_overview",
                description: "Topology overview and the service catalog (spoken aliases, workloads, restart policy) for model context.",
                input_schema: empty_schema(),
                annotations: Annotations::read_only(),
            },
            ToolSpec {
                name: "infra_report",
                description: "Full infrastructure report across every catalog service plus two cluster-wide checks: Flux (every Kustomization and HelmRelease must be Ready=True; suspended resources are shown but are not problems) and catalog coverage (any Deployment/StatefulSet/DaemonSet in the cluster that is neither a catalog service nor in the catalog ignore_workloads is reported as a problem). Returns a short Russian speech verdict (with an offer to send details) and a ready-to-send plain-text Telegram message in details.text: header, emoji verdict, problems first, then healthy services grouped by the catalog group, then active alerts. Set verbose=true to append a compact versions section (image, age, versions). Images, age and versions always stay in the structured details. Read-only; use infra_send_report only when a separate delivery is requested.",
                input_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "verbose": {
                            "type": "boolean",
                            "description": "append a compact versions section (image, age, versions) to the text report",
                        }
                    },
                }),
                annotations: Annotations::read_only(),
            },
        ]
    }

    fn call<'a>(
        &'a self,
        name: &'a str,
        arguments: Value,
    ) -> BoxFuture<'a, Result<ToolOutcome, String>> {
        Box::pin(async move {
            let deps = &self.deps;
            match name {
                "infra_status" => Ok(infra_status(deps).await),
                "infra_service" => Ok(infra_service(deps, &arguments).await),
                "infra_alert" => Ok(infra_alert(deps, &arguments).await),
                "infra_logs" => Ok(infra_logs(deps, &arguments).await),
                "infra_query" => Ok(infra_query(deps, &arguments).await),
                "infra_overview" => Ok(infra_overview(deps)),
                "infra_report" => Ok(infra_report(deps, &arguments).await),
                other => Err(format!("unknown tool {other:?}")),
            }
        })
    }
}

// --- shared serializable details -----------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ServiceRef {
    id: String,
    kind: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    namespace: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    workload_kind: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    workload_name: String,
    restart_allowed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkloadJson {
    namespace: String,
    kind: String,
    name: String,
    generation: i64,
    observed_generation: i64,
    desired: i64,
    updated: i64,
    ready: i64,
    available: i64,
}

impl From<kubernetes::Workload> for WorkloadJson {
    fn from(workload: kubernetes::Workload) -> Self {
        Self {
            namespace: workload.namespace,
            kind: workload.kind,
            name: workload.name,
            generation: workload.generation,
            observed_generation: workload.observed_generation,
            desired: workload.desired,
            updated: workload.updated,
            ready: workload.ready,
            available: workload.available,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ContainerJson {
    name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    image: String,
    ready: bool,
    restart_count: i32,
    #[serde(skip_serializing_if = "String::is_empty")]
    waiting_reason: String,
    last_terminated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PodJson {
    namespace: String,
    name: String,
    phase: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    node: String,
    ready: bool,
    restarts: i32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    containers: Vec<ContainerJson>,
    start_time: String,
}

fn zero_time() -> String {
    "0001-01-01T00:00:00Z".to_owned()
}

fn rfc3339(value: Option<time::OffsetDateTime>) -> String {
    value.map_or_else(zero_time, |value| {
        value
            .to_offset(time::UtcOffset::UTC)
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| zero_time())
    })
}

impl From<kubernetes::Pod> for PodJson {
    fn from(pod: kubernetes::Pod) -> Self {
        Self {
            namespace: pod.namespace,
            name: pod.name,
            phase: pod.phase,
            node: pod.node,
            ready: pod.ready,
            restarts: pod.restarts,
            containers: pod
                .containers
                .into_iter()
                .map(|container| ContainerJson {
                    name: container.name,
                    image: container.image,
                    ready: container.ready,
                    restart_count: container.restart_count,
                    waiting_reason: container.waiting_reason,
                    last_terminated_at: rfc3339(container.last_terminated_at),
                })
                .collect(),
            start_time: rfc3339(pod.start_time),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct EventJson {
    #[serde(rename = "type")]
    event_type: String,
    reason: String,
    message: String,
    count: i32,
    last_seen: String,
}

impl From<kubernetes::Event> for EventJson {
    fn from(event: kubernetes::Event) -> Self {
        Self {
            event_type: event.event_type,
            reason: event.reason,
            message: event.message,
            count: event.count,
            last_seen: rfc3339(event.last_seen),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ServiceDetail {
    service: ServiceRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    workload: Option<WorkloadJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pods: Option<Vec<PodJson>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    events: Option<Vec<EventJson>>,
    signals: Vec<SignalDetail>,
    alerts: Vec<AlertDetail>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AlertQueryDetail {
    r#ref: String,
    count: usize,
    alerts: Vec<AlertDetail>,
    #[serde(skip_serializing_if = "Option::is_none")]
    services: Option<Vec<ServiceDetail>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LogsDetail {
    service: String,
    source: String,
    since: String,
    lines: usize,
    #[serde(skip_serializing_if = "String::is_empty")]
    grep: String,
    text: String,
    truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct QueryDetail {
    promql: String,
    samples: Vec<LabeledSample>,
    #[serde(skip_serializing_if = "String::is_empty")]
    error: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LabeledSample {
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    labels: BTreeMap<String, String>,
    #[serde(serialize_with = "crate::domain::status::serialize_go_f64")]
    value: f64,
    #[serde(skip_serializing_if = "String::is_empty")]
    raw: String,
}

// --- infra_status ---------------------------------------------------------

async fn infra_status(deps: &Deps) -> ToolOutcome {
    let Some(catalog) = deps.catalog.as_ref() else {
        return failure("Каталог сервисов не загружен.", None::<Value>);
    };
    let now = deps.now();
    let mut detail = StatusDetail {
        checked_at: now.to_string(),
        service_count: catalog.services.len(),
        alerts: Vec::new(),
        ignored_alerts: Vec::new(),
        signals: Vec::new(),
        workloads: Vec::new(),
        restarts: Vec::new(),
        external_checks: Vec::new(),
        ..StatusDetail::default()
    };
    let mut errors = Vec::new();

    match deps.alerts() {
        Some(alerts) => match alerts.active(20).await {
            Ok(active) => {
                let mut items = Vec::new();
                let mut ignored = Vec::new();
                for alert in active {
                    let item = alert::compact_alert(&alert);
                    if catalog.ignores_alert(&item.name) {
                        ignored.push(item);
                    } else {
                        items.push(item);
                    }
                }
                detail.alerts = items;
                detail.ignored_alerts = ignored;
            }
            Err(error) => errors.push(format!("алерты: {error}")),
        },
        None => errors.push("алерты: Alertmanager не настроен".to_owned()),
    }

    if let Some(checks) = deps.checks() {
        detail.external_checks = checks.run_all(&catalog.services).await;
    }

    if deps.prom().is_none() && status::has_status_signals(catalog) {
        errors.push("сигналы: Prometheus не настроен".to_owned());
    }

    for service in &catalog.services {
        for signal in service.signals.iter().filter(|signal| signal.status) {
            let mut finding = SignalDetail {
                service: service.id.clone(),
                meaning: signal.meaning.clone(),
                promql: signal.promql.clone(),
                ..SignalDetail::default()
            };
            match deps.prom() {
                None => "Prometheus не настроен".clone_into(&mut finding.error),
                Some(prom) => match prom.query(&signal.promql, 5).await {
                    Ok(samples) => {
                        if let Some(sample) = samples.first() {
                            finding.firing = true;
                            finding.value = sample.value;
                        }
                    }
                    Err(error) => {
                        error.clone_into(&mut finding.error);
                        errors.push(format!("сигнал {}: {error}", service.id));
                    }
                },
            }
            detail.signals.push(finding);
        }
        if service.kind != catalog::SERVICE_K8S {
            continue;
        }
        let mut item = WorkloadDetail {
            service: service.id.clone(),
            namespace: service.namespace.clone(),
            kind: service.workload.kind.clone(),
            name: service.workload.name.clone(),
            ..WorkloadDetail::default()
        };
        match deps.kube() {
            None => {
                "Kubernetes API не настроен".clone_into(&mut item.error);
                errors.push("кластер: Kubernetes API не настроен".to_owned());
            }
            Some(kube) => {
                match kube
                    .get_workload(
                        &service.namespace,
                        &service.workload.kind,
                        &service.workload.name,
                    )
                    .await
                {
                    Ok(workload) => {
                        item.desired = workload.desired;
                        item.ready = workload.ready;
                        item.available = workload.available;
                        item.healthy = workload.available_ok();
                    }
                    Err(error) => {
                        error.clone_into(&mut item.error);
                        errors.push(format!("нагрузка {}: {error}", service.id));
                    }
                }
            }
        }
        detail.workloads.push(item);
    }

    if let Some(kube) = deps.kube() {
        let namespaces = status::namespaces_of(catalog);
        let cutoff = now.as_second() - 3600;
        for namespace in namespaces {
            let labels = BTreeMap::new();
            match kube.list_pods(&namespace, &labels).await {
                Ok(pods) => {
                    for pod in pods {
                        for container in pod.containers {
                            let reason = if container.waiting_reason.contains("CrashLoopBackOff") {
                                container.waiting_reason.clone()
                            } else if container.restart_count > 0
                                && container
                                    .last_terminated_at
                                    .is_some_and(|at| at.unix_timestamp() > cutoff)
                            {
                                "перезапуск за последний час".to_owned()
                            } else {
                                continue;
                            };
                            detail.restarts.push(PodDetail {
                                namespace: namespace.clone(),
                                pod: pod.name.clone(),
                                container: container.name.clone(),
                                reason,
                                restarts: container.restart_count,
                                ..PodDetail::default()
                            });
                        }
                    }
                }
                Err(error) => errors.push(format!("поды {namespace}: {error}")),
            }
        }
    }

    detail
        .alerts
        .sort_by(|left, right| left.name.cmp(&right.name));
    detail
        .ignored_alerts
        .sort_by(|left, right| left.name.cmp(&right.name));
    detail.signals.sort_by(|left, right| {
        left.service
            .cmp(&right.service)
            .then_with(|| left.meaning.cmp(&right.meaning))
    });
    detail
        .workloads
        .sort_by(|left, right| left.service.cmp(&right.service));
    detail
        .restarts
        .sort_by(|left, right| left.pod.cmp(&right.pod));

    detail.errors = errors;
    let issues = status::status_issues(&detail, catalog);
    detail.degraded = !issues.is_empty();
    detail.partial = !detail.errors.is_empty();
    detail.verdict = status::status_verdict(&detail, &issues);
    respond(detail.verdict.clone(), Some(&detail))
}

// --- infra_service --------------------------------------------------------

async fn infra_service(deps: &Deps, arguments: &Value) -> ToolOutcome {
    let name = string_arg(arguments, "name");
    let service = match deps.resolve(&name) {
        Ok(service) => service.clone(),
        Err(error) => {
            return failure(
                format!("Не нашёл сервис {}.", name.trim()),
                Some(json!({"error": error.to_string()})),
            );
        }
    };
    let detail = collect_service_detail(deps, &service, None).await;
    respond(
        service_speech(&service.display_name(), &service, &detail),
        Some(&detail),
    )
}

async fn collect_service_detail(
    deps: &Deps,
    service: &Service,
    alerts: Option<Vec<alert::Alert>>,
) -> ServiceDetail {
    let mut detail = ServiceDetail {
        service: ServiceRef {
            id: service.id.clone(),
            kind: service.kind.clone(),
            namespace: service.namespace.clone(),
            workload_kind: service.workload.kind.clone(),
            workload_name: service.workload.name.clone(),
            restart_allowed: service.restart.allowed,
        },
        workload: None,
        pods: None,
        events: None,
        signals: Vec::new(),
        alerts: Vec::new(),
        errors: Vec::new(),
    };

    if service.kind == catalog::SERVICE_K8S {
        match deps.kube() {
            None => detail.errors.push("Kubernetes API не настроен".to_owned()),
            Some(kube) => {
                match kube
                    .get_workload(
                        &service.namespace,
                        &service.workload.kind,
                        &service.workload.name,
                    )
                    .await
                {
                    Ok(workload) => {
                        let selector = workload.selector.clone();
                        detail.workload = Some(WorkloadJson::from(workload));
                        match kube.list_pods(&service.namespace, &selector).await {
                            Ok(pods) => {
                                let pods: Vec<PodJson> =
                                    pods.into_iter().map(PodJson::from).collect();
                                detail.pods = (!pods.is_empty()).then_some(pods);
                            }
                            Err(error) => detail.errors.push(error),
                        }
                        match kube
                            .list_events(&service.namespace, &service.workload.name, 10)
                            .await
                        {
                            Ok(events) => {
                                let events: Vec<EventJson> =
                                    events.into_iter().map(EventJson::from).collect();
                                detail.events = (!events.is_empty()).then_some(events);
                            }
                            Err(error) => detail.errors.push(error),
                        }
                    }
                    Err(error) => detail.errors.push(error),
                }
            }
        }
    }

    for signal in &service.signals {
        let mut finding = SignalDetail {
            service: service.id.clone(),
            meaning: signal.meaning.clone(),
            promql: signal.promql.clone(),
            ..SignalDetail::default()
        };
        match deps.prom() {
            None => "Prometheus не настроен".clone_into(&mut finding.error),
            Some(prom) => match prom.query(&signal.promql, 5).await {
                Ok(samples) => {
                    if let Some(sample) = samples.first() {
                        finding.firing = true;
                        finding.value = sample.value;
                    }
                }
                Err(error) => finding.error = error,
            },
        }
        detail.signals.push(finding);
    }

    let mut active = alerts;
    if active.is_none()
        && let Some(lister) = deps.alerts()
    {
        match lister.active(50).await {
            Ok(alerts) => active = Some(alerts),
            Err(error) => detail.errors.push(error),
        }
    }
    if let Some(active) = active {
        for alert in active {
            if alert::alert_matches(&alert, service) {
                detail.alerts.push(alert::compact_alert(&alert));
            }
        }
    }
    detail
}

fn service_speech(display: &str, service: &Service, detail: &ServiceDetail) -> String {
    if let Some(error) = detail.errors.first() {
        return format!("{display}: не удалось получить состояние — {error}");
    }
    let mut issues: Vec<String> = detail
        .signals
        .iter()
        .filter(|signal| signal.firing)
        .map(|signal| signal.meaning.clone())
        .collect();
    issues.extend(detail.alerts.iter().map(|alert| alert.summary.clone()));
    if service.kind == catalog::SERVICE_K8S
        && let Some(workload) = detail.workload.as_ref()
    {
        let available_ok = workload.desired == 0
            || (workload.available >= workload.desired && workload.ready >= workload.desired);
        if !available_ok {
            issues.push(format!(
                "готовы {} из {} подов",
                workload.ready, workload.desired
            ));
        }
        let not_ready: Vec<String> = detail
            .pods
            .as_ref()
            .map(|pods| {
                pods.iter()
                    .filter(|pod| !pod.ready)
                    .map(|pod| pod.name.clone())
                    .collect()
            })
            .unwrap_or_default();
        if !not_ready.is_empty() && issues.is_empty() {
            issues.push(format!("не готовы поды: {}", not_ready.join(", ")));
        }
    }
    if issues.is_empty() {
        if service.kind == catalog::SERVICE_K8S
            && let Some(workload) = detail.workload.as_ref()
        {
            let pods = detail.pods.as_ref().map_or(0, Vec::len);
            return format!(
                "{display} работает: готовы {} из {}, подов {pods}.",
                workload.ready, workload.desired
            );
        }
        return format!("{display}: сигналы в норме.");
    }
    issues.truncate(2);
    format!("{display}: {}.", issues.join("; "))
}

// --- infra_alert ----------------------------------------------------------

async fn infra_alert(deps: &Deps, arguments: &Value) -> ToolOutcome {
    let reference = string_arg(arguments, "ref").trim().to_owned();
    if reference.is_empty() {
        return failure("Нужен отпечаток или имя алерта.", None::<Value>);
    }
    let Some(lister) = deps.alerts() else {
        return failure("Alertmanager не настроен.", None::<Value>);
    };
    let alerts = match lister.active(100).await {
        Ok(alerts) => alerts,
        Err(error) => {
            return failure("Не удалось получить алерты.", Some(json!({"error": error})));
        }
    };

    let matches = alert::match_alerts(&alerts, &reference);
    let mut detail = AlertQueryDetail {
        r#ref: reference.clone(),
        count: 0,
        alerts: Vec::new(),
        services: None,
    };
    if matches.is_empty() {
        return respond(format!("Алерт не найден: {reference}."), Some(&detail));
    }
    detail.count = matches.len();
    for alert in matches.iter().take(10) {
        detail.alerts.push(alert::full_alert(alert));
    }
    let services = services_for_alerts(deps, &matches, &alerts).await;
    if !services.is_empty() {
        detail.services = Some(services);
    }
    let speech = alert_speech(
        &reference,
        &matches,
        detail.services.as_deref().unwrap_or(&[]),
    );
    respond(speech, Some(&detail))
}

async fn services_for_alerts(
    deps: &Deps,
    matches: &[&alert::Alert],
    alerts: &[alert::Alert],
) -> Vec<ServiceDetail> {
    let Some(catalog) = deps.catalog.as_ref() else {
        return Vec::new();
    };
    let mut specific = Vec::new();
    let mut namespace_only = Vec::new();
    let mut seen = BTreeMap::new();
    for alert in matches {
        for service in &catalog.services {
            if seen.contains_key(&service.id) {
                continue;
            }
            let (matched, is_specific) = alert::alert_service_match(alert, service);
            if !matched {
                continue;
            }
            seen.insert(service.id.clone(), ());
            if is_specific {
                specific.push(service.clone());
            } else {
                namespace_only.push(service.clone());
            }
        }
    }
    specific.sort_by(|left, right| left.id.cmp(&right.id));
    namespace_only.sort_by(|left, right| left.id.cmp(&right.id));
    let ordered: Vec<Service> = specific.into_iter().chain(namespace_only).take(3).collect();
    let mut out = Vec::with_capacity(ordered.len());
    for service in ordered {
        out.push(collect_service_detail(deps, &service, Some(alerts.to_vec())).await);
    }
    out
}

fn alert_speech(reference: &str, matches: &[&alert::Alert], services: &[ServiceDetail]) -> String {
    let Some(first) = matches.first() else {
        return format!("Алерт не найден: {reference}.");
    };
    let mut summary = first.summary().trim().to_owned();
    if summary.is_empty() {
        reference.clone_into(&mut summary);
    }
    if matches.len() > 1 {
        return format!(
            "Нашёл {} алертов по {reference:?}; первый: {summary}.",
            matches.len()
        );
    }
    let mut text = format!("Алерт: {summary}");
    let severity = first.severity();
    if !severity.is_empty() {
        let _ = write!(text, ", {severity}");
    }
    match services.first() {
        Some(service) => {
            let _ = write!(text, ". Сервис {}", service.service.id);
            if let Some(workload) = service.workload.as_ref() {
                let _ = write!(text, ": готовы {} из {}", workload.ready, workload.desired);
            }
        }
        None => text.push_str(". Сервис не определён"),
    }
    text.push('.');
    text
}

// --- infra_logs -----------------------------------------------------------

async fn infra_logs(deps: &Deps, arguments: &Value) -> ToolOutcome {
    let name = string_arg(arguments, "name");
    let service = match deps.resolve(&name) {
        Ok(service) => service.clone(),
        Err(error) => {
            return failure(
                format!("Не нашёл сервис {}.", name.trim()),
                Some(json!({"error": error.to_string()})),
            );
        }
    };
    let since_raw = string_arg(arguments, "since");
    let mut since = Duration::from_secs(15 * 60);
    if !since_raw.trim().is_empty() {
        match home_config::parse_go_duration(since_raw.trim()) {
            Ok(parsed) if !parsed.is_zero() && parsed <= Duration::from_secs(24 * 3600) => {
                since = parsed;
            }
            _ => {
                return failure(
                    "Не понял период, используйте например 15m или 1h.",
                    Some(json!({"since": since_raw})),
                );
            }
        }
    }
    let mut limit = integer_arg(arguments, "limit").unwrap_or(0);
    if limit == 0 {
        limit = 50;
    }
    limit = limit.min(500);
    let grep = string_arg(arguments, "grep").trim().to_lowercase();
    let display = service.display_name();
    let mut detail = LogsDetail {
        service: service.id.clone(),
        source: String::new(),
        since: go_duration_string(since),
        lines: 0,
        grep: string_arg(arguments, "grep"),
        text: String::new(),
        truncated: false,
    };

    let text = match service.logs.source.as_str() {
        catalog::LOGS_LOKI => {
            let Some(loki) = deps.loki() else {
                return failure("Loki не настроен, логи недоступны.", Some(&detail));
            };
            catalog::LOGS_LOKI.clone_into(&mut detail.source);
            let lines = match loki.query(&service.logs.selector, since, limit).await {
                Ok(lines) => lines,
                Err(error) => {
                    return failure(
                        format!("Не удалось прочитать логи {display} из Loki."),
                        Some(json!({"error": error})),
                    );
                }
            };
            let mut body = String::new();
            for line in lines {
                if !grep.is_empty() && !line.text.to_lowercase().contains(&grep) {
                    continue;
                }
                body.push_str(&line.text);
                body.push('\n');
                detail.lines += 1;
            }
            body
        }
        catalog::LOGS_PODS => {
            let Some(kube) = deps.kube() else {
                return failure(
                    "Kubernetes API не настроен, логи недоступны.",
                    Some(&detail),
                );
            };
            catalog::LOGS_PODS.clone_into(&mut detail.source);
            match pod_logs(kube, &service, since, limit, &grep, &mut detail).await {
                Ok(text) => text,
                Err(error) => {
                    return failure(
                        format!("Не удалось прочитать логи {display} из подов."),
                        Some(json!({"error": error})),
                    );
                }
            }
        }
        _ => {
            catalog::LOGS_NONE.clone_into(&mut detail.source);
            return respond(format!("Логи для {display} не настроены."), Some(&detail));
        }
    };

    let mut redacted = redact::text(&text);
    let cap = deps.log_byte_cap.load(std::sync::atomic::Ordering::Relaxed);
    if redacted.len() > cap {
        redacted = truncate_utf8(&redacted, cap);
        detail.truncated = true;
    }
    detail.text = redacted;
    if detail.lines == 0 {
        let trimmed = detail.text.trim();
        detail.lines = if trimmed.is_empty() {
            0
        } else {
            trimmed.matches('\n').count() + 1
        };
    }
    respond(
        format!(
            "Логи {display} за {}: {} строк.",
            human_duration(since),
            detail.lines
        ),
        Some(&detail),
    )
}

async fn pod_logs(
    kube: &dyn super::KubeApi,
    service: &Service,
    since: Duration,
    limit: usize,
    grep: &str,
    detail: &mut LogsDetail,
) -> Result<String, String> {
    let mut selector: BTreeMap<String, String> = BTreeMap::from([(
        "app.kubernetes.io/name".to_owned(),
        service.workload.name.clone(),
    )]);
    let mut containers = service.logs.containers.clone();
    if let Ok(workload) = kube
        .get_workload(
            &service.namespace,
            &service.workload.kind,
            &service.workload.name,
        )
        .await
    {
        if !workload.selector.is_empty() {
            selector = workload.selector;
        }
        if containers.is_empty() {
            containers = workload.containers;
        }
    }
    if containers.is_empty() {
        containers = service.containers.clone();
    }
    if containers.is_empty() {
        containers.push(String::new());
    }
    let mut pods = kube.list_pods(&service.namespace, &selector).await?;
    pods.truncate(3);
    let mut body = String::new();
    let mut lines = 0usize;
    for pod in &pods {
        for container in &containers {
            if lines >= limit {
                break;
            }
            let Ok(log) = kube
                .pod_logs(&service.namespace, &pod.name, container, limit, since)
                .await
            else {
                continue;
            };
            let mut header = format!("== {}", pod.name);
            if !container.is_empty() {
                let _ = write!(header, "/{container}");
            }
            header.push_str(" ==");
            body.push_str(&header);
            body.push('\n');
            for line in log.trim_end_matches('\n').split('\n') {
                if line.is_empty() {
                    continue;
                }
                if !grep.is_empty() && !line.to_lowercase().contains(grep) {
                    continue;
                }
                body.push_str(line);
                body.push('\n');
                lines += 1;
                if lines >= limit {
                    break;
                }
            }
        }
    }
    if pods.is_empty() {
        return Err(format!("no pods found for {}", service.workload.name));
    }
    detail.lines = lines;
    Ok(body)
}

// --- infra_query ----------------------------------------------------------

async fn infra_query(deps: &Deps, arguments: &Value) -> ToolOutcome {
    let promql = string_arg(arguments, "promql").trim().to_owned();
    if promql.is_empty() {
        return failure("Пустой запрос PromQL.", None::<Value>);
    }
    if promql.len() > 2000 {
        return failure(
            "Запрос слишком длинный.",
            Some(json!({"length": promql.len()})),
        );
    }
    let Some(prom) = deps.prom() else {
        return failure("Prometheus не настроен.", None::<Value>);
    };
    let samples = match prom.query(&promql, 20).await {
        Ok(samples) => samples,
        Err(error) => {
            return failure(
                "Запрос не выполнен.",
                Some(json!({"error": error, "promql": promql})),
            );
        }
    };
    let detail = QueryDetail {
        promql,
        samples: samples
            .into_iter()
            .map(|sample| LabeledSample {
                labels: sample.labels,
                value: sample.value,
                raw: sample.raw,
            })
            .collect(),
        error: String::new(),
    };
    let speech = if detail.samples.is_empty() {
        "Запрос не вернул данных.".to_owned()
    } else {
        format!("Запрос вернул {} рядов.", detail.samples.len())
    };
    respond(speech, Some(&detail))
}

// --- infra_overview -------------------------------------------------------

fn infra_overview(deps: &Deps) -> ToolOutcome {
    let Some(catalog) = deps.catalog.as_ref() else {
        return failure("Каталог сервисов не загружен.", None::<Value>);
    };
    respond(
        format!(
            "Каталог: {} сервисов. Топология в деталях.",
            catalog.services.len()
        ),
        Some(json!({
            "overview": catalog.overview_text(),
            "catalog": catalog.catalog_text(),
        })),
    )
}

// --- infra_report ---------------------------------------------------------

async fn infra_report(deps: &Deps, arguments: &Value) -> ToolOutcome {
    let Some(catalog) = deps.catalog.as_ref() else {
        return failure("Каталог сервисов не загружен.", None::<Value>);
    };
    let verbose = arguments
        .get("verbose")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let now = deps.now();
    let mut errors = Vec::new();
    let mut alerts = Vec::new();
    if let Some(lister) = deps.alerts() {
        match lister.active(100).await {
            Ok(active) => alerts = active,
            Err(error) => errors.push(format!("алерты: {error}")),
        }
    } else {
        errors.push("алерты: Alertmanager не настроен".to_owned());
    }

    // The three cluster-wide reads are independent: run them together so the
    // report budget is not spent on serial round trips.
    let (workloads, flux, checks) = tokio::join!(
        fetch_workloads(deps),
        fetch_flux(deps),
        run_catalog_checks(deps, catalog)
    );

    let mut probes = Vec::with_capacity(catalog.services.len());
    for service in &catalog.services {
        probes.push(collect_service_probe(deps, service, &checks).await);
    }

    let input = ReportInput {
        catalog,
        now,
        timezone: deps.location.clone(),
        probes,
        alerts,
        errors,
        verbose,
        workloads,
        flux,
    };
    let detail = report::build_report(&input);
    let speech = detail.verdict.clone();
    respond(speech, Some(&detail))
}

type ChecksByService = Option<HashMap<String, Vec<CheckResult>>>;

/// Every Deployment, StatefulSet and DaemonSet in the cluster.
async fn fetch_workloads(deps: &Deps) -> Option<Result<Vec<report::WorkloadRef>, String>> {
    let kube = deps.kube()?;
    Some(kube.list_workloads().await.map(|items| {
        items
            .into_iter()
            .map(|workload| report::WorkloadRef {
                namespace: workload.namespace,
                kind: workload.kind,
                name: workload.name,
            })
            .collect()
    }))
}

/// Every Flux Kustomization and HelmRelease, read-only through the API.
async fn fetch_flux(deps: &Deps) -> Option<Result<Vec<report::FluxResource>, String>> {
    let kube = deps.kube()?;
    Some(kube.list_flux().await.map(|items| {
        items
            .into_iter()
            .map(|resource| report::FluxResource {
                kind: resource.kind,
                namespace: resource.namespace,
                name: resource.name,
                ready: resource.ready,
                suspended: resource.suspended,
                message: resource.message,
            })
            .collect()
    }))
}

/// Runs every catalog check exactly once and groups the results by service.
async fn run_catalog_checks(deps: &Deps, catalog: &Catalog) -> ChecksByService {
    let checks = deps.checks()?;
    if catalog
        .services
        .iter()
        .all(|service| service.checks.is_empty())
    {
        return None;
    }
    let mut by_service: HashMap<String, Vec<CheckResult>> = HashMap::new();
    for result in checks.run_all(&catalog.services).await {
        by_service
            .entry(result.service.clone())
            .or_default()
            .push(result);
    }
    Some(by_service)
}

async fn collect_service_probe(
    deps: &Deps,
    service: &Service,
    checks: &ChecksByService,
) -> ServiceProbe {
    let mut probe = ServiceProbe::default();
    if service.kind == catalog::SERVICE_K8S {
        match deps.kube() {
            None => probe.workload = None,
            Some(kube) => {
                match kube
                    .get_workload(
                        &service.namespace,
                        &service.workload.kind,
                        &service.workload.name,
                    )
                    .await
                {
                    Ok(workload) => {
                        let selector = workload.selector.clone();
                        probe.workload = Some(Ok(workload_state(workload)));
                        match kube.list_pods(&service.namespace, &selector).await {
                            Ok(pods) => {
                                probe.pods = Some(Ok(pods.into_iter().map(pod_state).collect()));
                            }
                            Err(error) => probe.pods = Some(Err(error)),
                        }
                    }
                    Err(error) => probe.workload = Some(Err(error)),
                }
            }
        }
    }
    if !service.checks.is_empty() {
        probe.checks_configured = true;
        probe.checks = checks
            .as_ref()
            .and_then(|by_service| by_service.get(&service.id))
            .cloned()
            .unwrap_or_else(|| {
                // A service that declares checks but has no runner reports them
                // as skipped, exactly like an unconfigured Home Assistant
                // endpoint.
                service
                    .checks
                    .iter()
                    .map(|check| CheckResult {
                        service: service.id.clone(),
                        check_type: check.check_type.clone(),
                        label: check.display_label(),
                        ok: true,
                        skipped: true,
                        value: "не настроено".to_owned(),
                        ..CheckResult::default()
                    })
                    .collect()
            });
    }
    probe
}

fn workload_state(workload: kubernetes::Workload) -> WorkloadState {
    WorkloadState {
        namespace: workload.namespace,
        kind: workload.kind,
        name: workload.name,
        generation: workload.generation,
        observed_generation: workload.observed_generation,
        desired: workload.desired,
        updated: workload.updated,
        ready: workload.ready,
        available: workload.available,
    }
}

fn pod_state(pod: kubernetes::Pod) -> PodState {
    PodState {
        namespace: pod.namespace,
        name: pod.name,
        ready: pod.ready,
        start_time: pod
            .start_time
            .and_then(|at| Timestamp::from_second(at.unix_timestamp()).ok()),
        containers: pod
            .containers
            .into_iter()
            .map(|container| ContainerState {
                name: container.name,
                image: container.image,
                ready: container.ready,
                restart_count: container.restart_count,
                waiting_reason: container.waiting_reason,
                last_terminated_at: container
                    .last_terminated_at
                    .and_then(|at| Timestamp::from_second(at.unix_timestamp()).ok()),
            })
            .collect(),
    }
}

// --- helpers --------------------------------------------------------------

fn string_arg(arguments: &Value, key: &str) -> String {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn integer_arg(arguments: &Value, key: &str) -> Option<usize> {
    arguments
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value as usize)
}

fn go_duration_string(duration: Duration) -> String {
    let total = duration.as_secs();
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    if hours > 0 {
        format!("{hours}h{minutes}m{seconds}s")
    } else if minutes > 0 {
        format!("{minutes}m{seconds}s")
    } else {
        format!("{seconds}s")
    }
}

fn human_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds >= 3600 && seconds.is_multiple_of(3600) {
        format!("{} ч", seconds / 3600)
    } else if seconds >= 60 {
        format!("{} мин", seconds / 60)
    } else {
        format!("{seconds} с")
    }
}

fn truncate_utf8(text: &str, limit: usize) -> String {
    if limit == 0 {
        return String::new();
    }
    if text.len() <= limit {
        return text.to_owned();
    }
    let mut cut = limit;
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    text[..cut].to_owned()
}

// Keep unused imports intentional while the group is assembled.
#[allow(dead_code)]
fn _types(_: &Sample, _: &LogLine) {}
