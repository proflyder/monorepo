use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Ok,
    Unknown,
    Warning,
    Critical,
}

impl Level {
    #[must_use]
    pub fn worst(self, other: Self) -> Self {
        self.max(other)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub status: Level,
    pub generated_at: i64,
    pub sections: Sections,
    pub alerts: Vec<AlertItem>,
    pub alerts_available: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sections {
    pub host: Level,
    pub guests: Level,
    pub k8s: Level,
    pub devices: Level,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertItem {
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub severity: String,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hosts {
    pub status: Level,
    pub guests_status: Level,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<Host>,
    pub guests_available: bool,
    pub guests: Vec<Guest>,
    pub targets_available: bool,
    pub targets: Vec<Target>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub job: String,
    pub up: u32,
    pub total: u32,
    pub status: Level,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Host {
    pub name: String,
    pub up: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_ratio: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_used_bytes: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_total_bytes: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_used_bytes: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_total_bytes: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temp_celsius: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uptime_seconds: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GuestKind {
    Vm,
    Ct,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Guest {
    pub vmid: u32,
    pub name: String,
    pub kind: GuestKind,
    pub running: bool,
    pub onboot: bool,
    pub status: Level,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_ratio: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_used_bytes: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_total_bytes: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_used_bytes: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_total_bytes: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uptime_seconds: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Kubernetes {
    pub status: Level,
    pub available: bool,
    pub pod_count: usize,
    pub problem_count: usize,
    pub namespaces: Vec<Namespace>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Namespace {
    pub name: String,
    pub status: Level,
    pub pods: Vec<Pod>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pod {
    pub name: String,
    pub phase: String,
    pub ready: u32,
    pub containers: u32,
    pub restarts: u32,
    pub recent_restarts: u32,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub reason: String,
    pub status: Level,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_cores: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_bytes: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub age_seconds: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Devices {
    pub status: Level,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub station: Option<Station>,
    pub phones_available: bool,
    pub phones: Vec<Phone>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Station {
    pub up: bool,
    pub status: Level,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub version: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wifi_rssi_dbm: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temp_celsius: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uptime_seconds: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Phone {
    pub id: String,
    pub name: String,
    pub platform: String,
}
