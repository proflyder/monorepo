use std::fmt::Write as _;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const FINGERPRINT_SHOWN: usize = 12;
pub const MAX_ALERTS: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertStatus {
    Firing,
    Resolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertCard {
    pub fingerprint: String,
    pub name: String,
    pub severity: String,
    pub status: AlertStatus,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub summary: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub namespace: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub object: String,
    pub starts_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ends_at: Option<i64>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub runbook_url: String,
}

impl AlertCard {
    #[must_use]
    pub fn key(&self) -> String {
        format!("{}@{}", self.fingerprint, self.starts_at)
    }

    #[must_use]
    pub fn text(&self, zone: &TimeZone) -> String {
        let mut out = String::new();
        let icon = match (self.status, self.severity.as_str()) {
            (AlertStatus::Resolved, _) => "✅",
            (AlertStatus::Firing, "critical") => "🔴",
            (AlertStatus::Firing, _) => "🟠",
        };
        let _ = writeln!(out, "{icon} {} · {}", self.name, self.severity);
        let headline = if self.summary.is_empty() {
            &self.name
        } else {
            &self.summary
        };
        let _ = writeln!(out, "▸ {headline}");
        if !self.description.is_empty() {
            let _ = writeln!(out, "{}", self.description);
        }
        let place = [self.namespace.as_str(), self.object.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" / ");
        if !place.is_empty() {
            let _ = writeln!(out, "📍 {place}");
        }
        let _ = writeln!(out, "🕒 с {}", local(self.starts_at, zone));
        if let Some(ends) = self.ends_at {
            let _ = writeln!(out, "✅ решено {}", local(ends, zone));
        }
        let shown: String = self.fingerprint.chars().take(FINGERPRINT_SHOWN).collect();
        let _ = write!(out, "🔎 {shown}");
        out
    }
}

fn local(millis: i64, zone: &TimeZone) -> String {
    Timestamp::from_millisecond(millis)
        .map(|at| {
            at.to_zoned(zone.clone())
                .strftime("%d.%m %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}

#[derive(Debug, Deserialize)]
pub struct Webhook {
    #[serde(default)]
    pub alerts: Vec<WebhookAlert>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebhookAlert {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub labels: serde_json::Map<String, Value>,
    #[serde(default)]
    pub annotations: serde_json::Map<String, Value>,
    #[serde(default)]
    pub starts_at: String,
    #[serde(default)]
    pub ends_at: String,
    #[serde(default)]
    pub fingerprint: String,
}

const OBJECT_LABELS: [&str; 6] = [
    "deployment",
    "statefulset",
    "daemonset",
    "job_name",
    "pod",
    "instance",
];

impl WebhookAlert {
    #[must_use]
    pub fn card(&self) -> Option<AlertCard> {
        let name = label(&self.labels, "alertname");
        let starts_at = millis(&self.starts_at)?;
        if self.fingerprint.is_empty() || name.is_empty() {
            return None;
        }
        let status = match self.status.as_str() {
            "resolved" => AlertStatus::Resolved,
            _ => AlertStatus::Firing,
        };
        Some(AlertCard {
            fingerprint: self.fingerprint.clone(),
            severity: label(&self.labels, "severity"),
            status,
            summary: label(&self.annotations, "summary"),
            description: label(&self.annotations, "description"),
            namespace: label(&self.labels, "namespace"),
            object: OBJECT_LABELS
                .iter()
                .map(|key| label(&self.labels, key))
                .find(|value| !value.is_empty())
                .unwrap_or_default(),
            starts_at,
            ends_at: (status == AlertStatus::Resolved)
                .then(|| millis(&self.ends_at))
                .flatten(),
            runbook_url: label(&self.annotations, "runbook_url"),
            name,
        })
    }
}

fn label(map: &serde_json::Map<String, Value>, key: &str) -> String {
    map.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_owned()
}

fn millis(raw: &str) -> Option<i64> {
    raw.parse::<Timestamp>()
        .ok()
        .map(Timestamp::as_millisecond)
        .filter(|value| *value > 0)
}
