use std::collections::BTreeMap;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use super::catalog::{self, Service};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Alert {
    #[serde(default)]
    pub fingerprint: String,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    #[serde(default)]
    pub annotations: BTreeMap<String, String>,
    #[serde(skip)]
    pub starts_at: Option<Timestamp>,
}

impl Alert {
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

    #[must_use]
    pub fn severity(&self) -> String {
        self.labels.get("severity").cloned().unwrap_or_default()
    }

    #[must_use]
    pub fn name(&self) -> String {
        self.labels.get("alertname").cloned().unwrap_or_default()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertDetail {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub fingerprint: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub severity: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub started_at: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub annotations: BTreeMap<String, String>,
}

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

#[must_use]
pub fn alert_matches(alert: &Alert, service: &Service) -> bool {
    for (key, value) in &alert.labels {
        let lowered = value.to_lowercase();
        if key == "namespace"
            && !service.namespace.is_empty()
            && lowered == service.namespace.to_lowercase()
        {
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
