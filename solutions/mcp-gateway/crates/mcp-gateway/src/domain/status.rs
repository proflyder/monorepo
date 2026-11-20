use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use super::alert::AlertDetail;
use super::catalog::{self, Catalog};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckResult {
    #[serde(default)]
    pub service: String,
    #[serde(rename = "type", default)]
    pub check_type: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub ok: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub skipped: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub value: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub error: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalDetail {
    pub service: String,
    pub meaning: String,
    pub promql: String,
    pub firing: bool,
    #[serde(default, serialize_with = "serialize_go_f64")]
    pub value: f64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub error: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkloadDetail {
    pub service: String,
    pub namespace: String,
    pub kind: String,
    pub name: String,
    pub desired: i64,
    pub ready: i64,
    pub available: i64,
    pub healthy: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub error: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PodDetail {
    #[serde(default)]
    pub service: String,
    pub namespace: String,
    pub pod: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub container: String,
    pub reason: String,
    pub restarts: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusDetail {
    pub verdict: String,
    pub checked_at: String,
    pub service_count: usize,
    pub degraded: bool,
    pub partial: bool,
    pub alerts: Vec<AlertDetail>,
    pub ignored_alerts: Vec<AlertDetail>,
    pub signals: Vec<SignalDetail>,
    pub workloads: Vec<WorkloadDetail>,
    pub restarts: Vec<PodDetail>,
    pub external_checks: Vec<CheckResult>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

pub fn serialize_go_f64<S>(value: &f64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    if value.is_finite()
        && value.fract() == 0.0
        && *value >= i64::MIN as f64
        && *value <= i64::MAX as f64
        && *value >= 0.0
    {
        return serializer.serialize_i64(*value as i64);
    }
    serializer.serialize_f64(*value)
}

#[must_use]
pub fn check_issue(check: &CheckResult) -> String {
    if check.check_type == catalog::CHECK_HA_UNAVAILABLE {
        return unavailable_issue(check);
    }
    format!("{}: {}", check.label, check.error)
}

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

#[must_use]
pub fn has_status_signals(catalog: &Catalog) -> bool {
    catalog
        .services
        .iter()
        .flat_map(|service| &service.signals)
        .any(|signal| signal.status)
}

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

#[must_use]
pub fn display_name(catalog: &Catalog, id: &str) -> String {
    catalog
        .find(id)
        .map_or_else(|| id.to_owned(), catalog::Service::display_name)
}
