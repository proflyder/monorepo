//! Alertmanager alert ↔ catalog service matching.
//!
//! Ports the alert half of `internal/tools/read.go`: fingerprint/name lookup
//! for `infra_alert` (a fingerprint prefix must be at least six characters) and
//! the label heuristics that attach an active alert to a catalog service.
//! Namespace alone is never a specific match.

use std::collections::BTreeMap;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use super::catalog::{self, Service};

/// One Alertmanager alert (the subset the gateway reports).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Alert {
    /// Alertmanager's stable identifier.
    #[serde(default)]
    pub fingerprint: String,
    /// Alert labels.
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    /// Alert annotations.
    #[serde(default)]
    pub annotations: BTreeMap<String, String>,
    /// When the alert started.
    #[serde(skip)]
    pub starts_at: Option<Timestamp>,
}

impl Alert {
    /// One-line human description.
    #[must_use]
    pub fn summary(&self) -> String {
        if let Some(text) = self
            .annotations
            .get("summary")
            .filter(|text| !text.is_empty())
        {
            return text.clone();
        }
        if let Some(text) = self
            .annotations
            .get("description")
            .filter(|text| !text.is_empty())
        {
            return text.clone();
        }
        if let Some(name) = self.labels.get("alertname").filter(|name| !name.is_empty()) {
            return name.clone();
        }
        "alert".to_owned()
    }

    /// The severity label, or an empty string.
    #[must_use]
    pub fn severity(&self) -> String {
        self.labels.get("severity").cloned().unwrap_or_default()
    }

    /// The alert name label.
    #[must_use]
    pub fn name(&self) -> String {
        self.labels.get("alertname").cloned().unwrap_or_default()
    }
}

/// The compact alert view returned by lists and reports.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertDetail {
    /// Alert name.
    pub name: String,
    /// Alertmanager fingerprint; only the full view sets it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub fingerprint: String,
    /// Severity label.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub severity: String,
    /// One-line summary.
    pub summary: String,
    /// Description annotation; only the full view sets it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// RFC3339 start time.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub started_at: String,
    /// Full labels; only the full view sets them.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,
    /// Full annotations; only the full view sets them.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub annotations: BTreeMap<String, String>,
}

/// The compact view: name, severity, summary and start time.
#[must_use]
pub fn compact_alert(alert: &Alert) -> AlertDetail {
    AlertDetail {
        name: alert.name(),
        severity: alert.severity(),
        summary: alert.summary(),
        started_at: alert
            .starts_at
            .map(|starts| starts.to_string())
            .unwrap_or_default(),
        ..AlertDetail::default()
    }
}

/// The full view used by `infra_alert`: fingerprint, labels and annotations.
#[must_use]
pub fn full_alert(alert: &Alert) -> AlertDetail {
    AlertDetail {
        fingerprint: alert.fingerprint.clone(),
        description: alert
            .annotations
            .get("description")
            .cloned()
            .unwrap_or_default(),
        labels: alert.labels.clone(),
        annotations: alert.annotations.clone(),
        ..compact_alert(alert)
    }
}

/// Selects alerts by name (case-insensitive, exact) or by Alertmanager
/// fingerprint prefix. A prefix must be at least six characters so a short
/// accidental string cannot match half the alert list.
#[must_use]
pub fn match_alerts<'a>(alerts: &'a [Alert], reference: &str) -> Vec<&'a Alert> {
    let reference = reference.trim().to_lowercase();
    if reference.is_empty() {
        return Vec::new();
    }
    alerts
        .iter()
        .filter(|alert| {
            let name = alert.name();
            let name = name.trim().to_lowercase();
            if name == reference {
                return true;
            }
            let fingerprint = alert.fingerprint.trim().to_lowercase();
            reference.chars().count() >= 6
                && !fingerprint.is_empty()
                && fingerprint.starts_with(&reference)
        })
        .collect()
}

/// Attaches an alert to a catalog service for the full report.
///
/// Comparison is deliberately simple here (lowercase only, namespace labels
/// skipped): this is the degradation path that matches the Go report, while
/// [`alert_service_match`] is the normalized path used by `infra_alert`.
#[must_use]
pub fn alert_matches(alert: &Alert, service: &Service) -> bool {
    for (key, value) in &alert.labels {
        let lowered = value.to_lowercase();
        if key == "namespace"
            && !service.namespace.is_empty()
            && lowered == service.namespace.to_lowercase()
        {
            // Namespace alone is too broad: require another match.
            continue;
        }
        if lowered.is_empty() {
            continue;
        }
        let workload = service.workload.name.to_lowercase();
        if lowered == service.id.to_lowercase()
            || (!workload.is_empty() && lowered.contains(&workload))
        {
            return true;
        }
    }
    false
}

/// Whether an alert label refers to the service, and whether that match is
/// specific (not namespace-only).
#[must_use]
pub fn alert_service_match(alert: &Alert, service: &Service) -> (bool, bool) {
    let mut namespace_match = false;
    for (key, value) in &alert.labels {
        let normalized = catalog::normalize(value);
        if normalized.is_empty() {
            continue;
        }
        if key.to_lowercase() == "namespace" {
            if !service.namespace.is_empty() && normalized == catalog::normalize(&service.namespace)
            {
                namespace_match = true;
            }
            continue;
        }
        if service_matches_label_value(service, &normalized) {
            return (true, true);
        }
    }
    (namespace_match, false)
}

/// Whether a normalized label value names the service by id, workload (or a
/// pod/endpoint suffix of it) or a spoken alias.
#[must_use]
pub fn service_matches_label_value(service: &Service, value: &str) -> bool {
    if value == catalog::normalize(&service.id) {
        return true;
    }
    let name = catalog::normalize(&service.workload.name);
    if !name.is_empty() && (value == name || value.starts_with(&format!("{name}-"))) {
        return true;
    }
    service
        .alias_index
        .iter()
        .any(|alias| value == catalog::normalize(alias))
}

/// Deduplicates report alerts by name+summary and sorts them by name.
#[must_use]
pub fn dedupe_alerts(alerts: Vec<AlertDetail>) -> Vec<AlertDetail> {
    let mut seen: Vec<(String, String)> = Vec::new();
    let mut out: Vec<AlertDetail> = Vec::with_capacity(alerts.len());
    for alert in alerts {
        let key = (alert.name.clone(), alert.summary.clone());
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push(alert);
    }
    out.sort_by(|left, right| left.name.cmp(&right.name));
    out
}

/// One line of the report alerts section.
#[must_use]
pub fn alert_line(alert: &AlertDetail) -> String {
    use std::fmt::Write as _;

    let mut line = format!("⚠️ {}", alert.name);
    if !alert.severity.is_empty() {
        let _ = write!(line, " ({})", alert.severity);
    }
    if !alert.summary.is_empty() {
        let _ = write!(line, ": {}", alert.summary);
    }
    line
}
