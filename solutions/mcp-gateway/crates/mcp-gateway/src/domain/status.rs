//! The `infra_status` aggregate.
//!
//! Ports `statusIssues`/`statusVerdict` from `internal/tools/read.go`: the
//! caller fetches alerts, signals, workloads, restarts and external checks,
//! this module only turns them into an ordered issue list and a spoken verdict.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use super::alert::AlertDetail;
use super::catalog::{self, Catalog};

/// Result of one typed external probe.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckResult {
    /// Catalog service id.
    #[serde(default)]
    pub service: String,
    /// Check type.
    #[serde(rename = "type", default)]
    pub check_type: String,
    /// Human label.
    #[serde(default)]
    pub label: String,
    /// Whether the check passed.
    #[serde(default)]
    pub ok: bool,
    /// The check was not configured; it is never a failure.
    #[serde(default)]
    pub skipped: bool,
    /// Observed value, for example the unavailable entity count.
    #[serde(default)]
    pub value: String,
    /// Failure reason.
    #[serde(default)]
    pub error: String,
    /// Additional details, for example unavailable entity ids.
    #[serde(default)]
    pub details: Vec<String>,
}

/// One Prometheus signal observation.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalDetail {
    /// Catalog service id.
    pub service: String,
    /// Human explanation.
    pub meaning: String,
    /// The PromQL query.
    pub promql: String,
    /// Whether the signal fired.
    pub firing: bool,
    /// First sample value.
    #[serde(default)]
    pub value: f64,
    /// Query error.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub error: String,
}

/// One workload availability observation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkloadDetail {
    /// Catalog service id.
    pub service: String,
    /// Namespace.
    pub namespace: String,
    /// Workload kind.
    pub kind: String,
    /// Workload name.
    pub name: String,
    /// Desired replicas.
    pub desired: i64,
    /// Ready replicas.
    pub ready: i64,
    /// Available replicas.
    pub available: i64,
    /// Whether the workload has all desired replicas available.
    pub healthy: bool,
    /// Read error.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub error: String,
}

/// One pod restart finding.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PodDetail {
    /// Catalog service id, when known.
    #[serde(default)]
    pub service: String,
    /// Namespace.
    pub namespace: String,
    /// Pod name.
    pub pod: String,
    /// Container name.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub container: String,
    /// Human reason: `CrashLoopBackOff` or `перезапуск за последний час`.
    pub reason: String,
    /// Container restart count.
    pub restarts: i32,
}

/// The whole `infra_status` detail.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusDetail {
    /// Spoken verdict.
    pub verdict: String,
    /// RFC3339 UTC check time.
    pub checked_at: String,
    /// Number of catalog services.
    pub service_count: usize,
    /// Whether at least one issue was found.
    pub degraded: bool,
    /// Whether part of the check could not run.
    pub partial: bool,
    /// Active alerts that count as problems.
    pub alerts: Vec<AlertDetail>,
    /// Active alerts the catalog marks as permanent background noise.
    pub ignored_alerts: Vec<AlertDetail>,
    /// Prometheus signals.
    pub signals: Vec<SignalDetail>,
    /// Kubernetes workloads.
    pub workloads: Vec<WorkloadDetail>,
    /// Recent pod restarts.
    pub restarts: Vec<PodDetail>,
    /// Typed external checks.
    pub external_checks: Vec<CheckResult>,
    /// Errors that made the check partial.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}

/// Renders one failed check without repeating its label.
///
/// The `ha_unavailable_summary` check already says "недоступно сущностей", so
/// prefixing its label again produced "недоступные сущности: недоступно
/// сущностей: 1"; here the count and the entity ids stay inline and the label
/// is used at most once.
#[must_use]
pub fn check_issue(check: &CheckResult) -> String {
    if check.check_type == catalog::CHECK_HA_UNAVAILABLE {
        return unavailable_issue(check);
    }
    format!("{}: {}", check.label, check.error)
}

/// Maximum number of unavailable entity ids kept in a spoken issue.
pub const MAX_UNAVAILABLE_ENTITIES_SHOWN: usize = 5;

fn unavailable_issue(check: &CheckResult) -> String {
    let mut count = check.value.trim();
    if count.is_empty() {
        count = check
            .error
            .trim()
            .trim_start_matches("недоступно сущностей:")
            .trim();
    }
    if count.is_empty() {
        count = "?";
    }
    let ids = check
        .details
        .iter()
        .take(MAX_UNAVAILABLE_ENTITIES_SHOWN)
        .cloned()
        .collect::<Vec<_>>();
    let mut text = format!("недоступно сущностей: {count}");
    if !ids.is_empty() {
        text.push_str(" (");
        text.push_str(&ids.join(", "));
        text.push(')');
    }
    text
}

/// The ordered issue list that drives the verdict and `degraded`.
#[must_use]
pub fn status_issues(detail: &StatusDetail, catalog: &Catalog) -> Vec<String> {
    let mut issues = Vec::new();
    for alert in &detail.alerts {
        let mut label = alert.summary.clone();
        if !alert.severity.is_empty() {
            label.push_str(" (");
            label.push_str(&alert.severity);
            label.push(')');
        }
        issues.push(label);
    }
    for signal in &detail.signals {
        if signal.firing {
            issues.push(signal.meaning.clone());
        }
    }
    for workload in &detail.workloads {
        if !workload.error.is_empty() || !workload.healthy {
            issues.push(format!(
                "{}: готово {} из {}",
                display_name(catalog, &workload.service),
                workload.ready,
                workload.desired
            ));
        }
    }
    for restart in &detail.restarts {
        issues.push(format!("{}: {}", restart.pod, restart.reason));
    }
    for check in &detail.external_checks {
        if check.skipped || check.ok {
            continue;
        }
        issues.push(check_issue(check));
    }
    issues
}

/// The spoken verdict for a status detail.
#[must_use]
pub fn status_verdict(detail: &StatusDetail, issues: &[String]) -> String {
    if issues.is_empty() && detail.errors.is_empty() {
        return format!(
            "Всё в порядке: {} сервисов, алертов нет.",
            detail.service_count
        );
    }
    if issues.is_empty() {
        let error = detail.errors.first().map_or("", String::as_str);
        return format!("Проверка неполная: {error}.");
    }
    let shown = if issues.len() > 2 {
        &issues[..2]
    } else {
        issues
    };
    let mut text = format!("Проблемы: {}", shown.join("; "));
    if issues.len() > shown.len() {
        let _ = write!(text, " и ещё {}", issues.len() - shown.len());
    }
    text.push('.');
    text
}

/// Whether any catalog signal participates in the status aggregate.
#[must_use]
pub fn has_status_signals(catalog: &Catalog) -> bool {
    catalog
        .services
        .iter()
        .flat_map(|service| &service.signals)
        .any(|signal| signal.status)
}

/// Sorted unique namespaces of the Kubernetes services.
#[must_use]
pub fn namespaces_of(catalog: &Catalog) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for service in catalog.k8s_services() {
        if !out.contains(&service.namespace) {
            out.push(service.namespace.clone());
        }
    }
    out.sort();
    out
}

/// Display name of a service id, falling back to the id itself.
#[must_use]
pub fn display_name(catalog: &Catalog, id: &str) -> String {
    catalog
        .find(id)
        .map_or_else(|| id.to_owned(), catalog::Service::display_name)
}
