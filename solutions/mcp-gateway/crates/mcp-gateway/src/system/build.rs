use std::collections::{BTreeMap, HashMap};

use crate::domain::alert::Alert;
use crate::push::Device;
use crate::tools::Sample;

use super::model::{
    AlertItem, Devices, Guest, GuestKind, Host, Hosts, Kubernetes, Level, Namespace, Phone, Pod,
    Station,
};

#[derive(Debug, Default, Clone)]
pub struct Results {
    answers: HashMap<&'static str, Vec<Sample>>,
}

impl Results {
    pub fn insert(&mut self, key: &'static str, samples: Vec<Sample>) {
        self.answers.insert(key, samples);
    }

    fn get(&self, key: &str) -> Option<&[Sample]> {
        self.answers.get(key).map(Vec::as_slice)
    }

    fn scalar(&self, key: &str) -> Option<f64> {
        self.get(key)?
            .first()
            .map(|sample| sample.value)
            .filter(|value| value.is_finite())
    }

    fn by_label(&self, key: &str, label: &str) -> BTreeMap<String, f64> {
        self.get(key)
            .unwrap_or_default()
            .iter()
            .filter(|sample| sample.value.is_finite())
            .filter_map(|sample| Some((sample.labels.get(label)?.clone(), sample.value)))
            .collect()
    }

    fn by_pod(&self, key: &str) -> BTreeMap<(String, String), f64> {
        self.get(key)
            .unwrap_or_default()
            .iter()
            .filter(|sample| sample.value.is_finite())
            .filter_map(|sample| Some((pod_key(sample)?, sample.value)))
            .collect()
    }
}

fn pod_key(sample: &Sample) -> Option<(String, String)> {
    Some((
        sample.labels.get("namespace")?.clone(),
        sample.labels.get("pod")?.clone(),
    ))
}

pub const HOST_QUERIES: &[(&str, &str)] = &[
    ("up", r#"max(up{job="pve-node"})"#),
    (
        "cpu",
        r#"1 - avg(rate(node_cpu_seconds_total{job="pve-node",mode="idle"}[2m]))"#,
    ),
    (
        "mem_total",
        r#"max(node_memory_MemTotal_bytes{job="pve-node"})"#,
    ),
    (
        "mem_available",
        r#"max(node_memory_MemAvailable_bytes{job="pve-node"})"#,
    ),
    (
        "disk_total",
        r#"max(node_filesystem_size_bytes{job="pve-node",mountpoint="/"})"#,
    ),
    (
        "disk_available",
        r#"max(node_filesystem_avail_bytes{job="pve-node",mountpoint="/"})"#,
    ),
    (
        "temp",
        r#"max(node_hwmon_temp_celsius{job="pve-node",chip=~".*coretemp.*"})"#,
    ),
    (
        "uptime",
        r#"max(time() - node_boot_time_seconds{job="pve-node"})"#,
    ),
];

pub const GUEST_QUERIES: &[(&str, &str)] = &[
    ("info", r#"pve_guest_info{template!="1"}"#),
    ("up", "max by (id) (pve_up)"),
    ("onboot", "max by (id) (pve_onboot_status)"),
    ("cpu", "max by (id) (pve_cpu_usage_ratio)"),
    ("mem_used", "max by (id) (pve_memory_usage_bytes)"),
    ("mem_total", "max by (id) (pve_memory_size_bytes)"),
    ("disk_used", "max by (id) (pve_disk_usage_bytes)"),
    ("disk_total", "max by (id) (pve_disk_size_bytes)"),
    ("uptime", "max by (id) (pve_uptime_seconds)"),
];

pub const K8S_QUERIES: &[(&str, &str)] = &[
    (
        "phase",
        "max by (namespace, pod, phase) (kube_pod_status_phase == 1)",
    ),
    (
        "containers",
        "count by (namespace, pod) (kube_pod_container_info)",
    ),
    (
        "ready",
        "sum by (namespace, pod) (kube_pod_container_status_ready)",
    ),
    (
        "restarts",
        "sum by (namespace, pod) (kube_pod_container_status_restarts_total)",
    ),
    (
        "recent_restarts",
        "sum by (namespace, pod) (increase(kube_pod_container_status_restarts_total[1h]))",
    ),
    (
        "waiting",
        "max by (namespace, pod, reason) (kube_pod_container_status_waiting_reason == 1)",
    ),
    (
        "cpu",
        r#"sum by (namespace, pod) (rate(container_cpu_usage_seconds_total{container!=""}[5m]))"#,
    ),
    (
        "mem",
        r#"sum by (namespace, pod) (container_memory_working_set_bytes{container!=""})"#,
    ),
    ("age", "max by (namespace, pod) (time() - kube_pod_created)"),
];

pub const STATION_QUERIES: &[(&str, &str)] = &[
    ("up", r#"max(up{job="station"})"#),
    ("info", r#"pulsar_info{job="station"}"#),
    ("latency", r#"max(scrape_duration_seconds{job="station"})"#),
    ("rssi", r#"max(pulsar_wifi_rssi_dbm{job="station"})"#),
    (
        "temp",
        r#"max(pulsar_cpu_temperature_celsius{job="station"})"#,
    ),
    ("uptime", r#"max(pulsar_uptime_seconds{job="station"})"#),
];

pub const SAMPLE_LIMIT: usize = 2000;

const HOT_CELSIUS: f64 = 75.0;
const CRITICAL_CELSIUS: f64 = 85.0;
const FULL_RATIO: f64 = 0.9;

#[must_use]
pub fn host(results: &Results) -> (Level, Option<Host>) {
    let Some(up) = results.scalar("up") else {
        return (Level::Unknown, None);
    };
    let mem_total = results.scalar("mem_total");
    let disk_total = results.scalar("disk_total");
    let host = Host {
        name: "pve".to_owned(),
        up: up >= 1.0,
        cpu_ratio: results.scalar("cpu").map(|ratio| ratio.clamp(0.0, 1.0)),
        mem_used_bytes: used(mem_total, results.scalar("mem_available")),
        mem_total_bytes: mem_total,
        disk_used_bytes: used(disk_total, results.scalar("disk_available")),
        disk_total_bytes: disk_total,
        temp_celsius: results.scalar("temp"),
        uptime_seconds: results.scalar("uptime"),
    };
    (host_level(&host), Some(host))
}

fn used(total: Option<f64>, available: Option<f64>) -> Option<f64> {
    Some((total? - available?).max(0.0))
}

fn host_level(host: &Host) -> Level {
    if !host.up {
        return Level::Critical;
    }
    let temp = match host.temp_celsius {
        Some(value) if value >= CRITICAL_CELSIUS => Level::Critical,
        Some(value) if value >= HOT_CELSIUS => Level::Warning,
        _ => Level::Ok,
    };
    let full = |used: Option<f64>, total: Option<f64>| match (used, total) {
        (Some(used), Some(total)) if total > 0.0 && used / total >= FULL_RATIO => Level::Warning,
        _ => Level::Ok,
    };
    temp.worst(full(host.mem_used_bytes, host.mem_total_bytes))
        .worst(full(host.disk_used_bytes, host.disk_total_bytes))
}

#[must_use]
pub fn guests(results: &Results) -> (Level, Option<Vec<Guest>>) {
    let info = results.get("info").unwrap_or_default();
    if info.is_empty() {
        return (Level::Unknown, None);
    }
    let up = results.by_label("up", "id");
    let onboot = results.by_label("onboot", "id");
    let cpu = results.by_label("cpu", "id");
    let mem_used = results.by_label("mem_used", "id");
    let mem_total = results.by_label("mem_total", "id");
    let disk_used = results.by_label("disk_used", "id");
    let disk_total = results.by_label("disk_total", "id");
    let uptime = results.by_label("uptime", "id");
    let mut guests: Vec<Guest> = info
        .iter()
        .filter_map(|sample| {
            let id = sample.labels.get("id")?;
            let (kind, vmid) = parse_guest_id(id)?;
            let running = up.get(id).is_some_and(|value| *value >= 1.0);
            let onboot = onboot.get(id).is_some_and(|value| *value >= 1.0);
            let status = if !running && onboot {
                Level::Critical
            } else {
                Level::Ok
            };
            Some(Guest {
                vmid,
                name: sample.labels.get("name").cloned().unwrap_or_default(),
                kind,
                running,
                onboot,
                status,
                cpu_ratio: cpu.get(id).copied().filter(|_| running),
                mem_used_bytes: mem_used.get(id).copied().filter(|_| running),
                mem_total_bytes: mem_total.get(id).copied(),
                disk_used_bytes: disk_used.get(id).copied().filter(|value| *value > 0.0),
                disk_total_bytes: disk_total.get(id).copied().filter(|value| *value > 0.0),
                uptime_seconds: uptime.get(id).copied().filter(|_| running),
            })
        })
        .collect();
    guests.sort_by(|left, right| {
        right
            .status
            .cmp(&left.status)
            .then(left.vmid.cmp(&right.vmid))
    });
    let level = guests
        .iter()
        .map(|guest| guest.status)
        .fold(Level::Ok, Level::worst);
    (level, Some(guests))
}

fn parse_guest_id(id: &str) -> Option<(GuestKind, u32)> {
    let (kind, vmid) = id.split_once('/')?;
    let kind = match kind {
        "qemu" => GuestKind::Vm,
        "lxc" => GuestKind::Ct,
        _ => return None,
    };
    Some((kind, vmid.parse().ok()?))
}

#[must_use]
pub fn kubernetes(results: &Results) -> Kubernetes {
    let Some(phases) = results.get("phase").filter(|samples| !samples.is_empty()) else {
        return Kubernetes {
            status: Level::Unknown,
            available: false,
            pod_count: 0,
            problem_count: 0,
            namespaces: Vec::new(),
        };
    };
    let containers = results.by_pod("containers");
    let ready = results.by_pod("ready");
    let restarts = results.by_pod("restarts");
    let recent = results.by_pod("recent_restarts");
    let cpu = results.by_pod("cpu");
    let mem = results.by_pod("mem");
    let age = results.by_pod("age");
    let mut waiting: BTreeMap<(String, String), String> = BTreeMap::new();
    for sample in results.get("waiting").unwrap_or_default() {
        if let (Some(key), Some(reason)) = (pod_key(sample), sample.labels.get("reason")) {
            waiting.entry(key).or_insert_with(|| reason.clone());
        }
    }

    let mut namespaces: BTreeMap<String, Vec<Pod>> = BTreeMap::new();
    for sample in phases {
        let (Some(key), Some(phase)) = (pod_key(sample), sample.labels.get("phase")) else {
            continue;
        };
        if phase == "Succeeded" {
            continue;
        }
        let count = |map: &BTreeMap<(String, String), f64>| {
            map.get(&key)
                .map_or(0, |value| value.round().max(0.0) as u32)
        };
        let mut pod = Pod {
            name: key.1.clone(),
            phase: phase.clone(),
            ready: count(&ready),
            containers: count(&containers),
            restarts: count(&restarts),
            recent_restarts: count(&recent),
            reason: waiting.get(&key).cloned().unwrap_or_default(),
            status: Level::Ok,
            cpu_cores: cpu.get(&key).copied(),
            mem_bytes: mem.get(&key).copied(),
            age_seconds: age.get(&key).copied(),
        };
        pod.status = pod_level(&pod);
        namespaces.entry(key.0).or_default().push(pod);
    }

    let mut namespaces: Vec<Namespace> = namespaces
        .into_iter()
        .map(|(name, mut pods)| {
            pods.sort_by(|left, right| {
                right
                    .status
                    .cmp(&left.status)
                    .then_with(|| left.name.cmp(&right.name))
            });
            let status = pods
                .iter()
                .map(|pod| pod.status)
                .fold(Level::Ok, Level::worst);
            Namespace { name, status, pods }
        })
        .collect();
    namespaces.sort_by(|left, right| {
        right
            .status
            .cmp(&left.status)
            .then_with(|| left.name.cmp(&right.name))
    });
    let pods = namespaces.iter().flat_map(|namespace| &namespace.pods);
    let pod_count = pods.clone().count();
    let problem_count = pods.filter(|pod| pod.status != Level::Ok).count();
    let status = namespaces
        .iter()
        .map(|namespace| namespace.status)
        .fold(Level::Ok, Level::worst);
    Kubernetes {
        status,
        available: true,
        pod_count,
        problem_count,
        namespaces,
    }
}

fn pod_level(pod: &Pod) -> Level {
    match pod.phase.as_str() {
        "Failed" | "Unknown" => return Level::Critical,
        "Pending" => return Level::Warning,
        _ => {}
    }
    if pod.reason == "CrashLoopBackOff" || pod.reason.contains("ImagePull") {
        return Level::Critical;
    }
    if pod.ready < pod.containers || !pod.reason.is_empty() || pod.recent_restarts > 0 {
        return Level::Warning;
    }
    Level::Ok
}

#[must_use]
pub fn station(results: &Results) -> (Level, Option<Station>) {
    let Some(up) = results.scalar("up") else {
        return (Level::Unknown, None);
    };
    let info = results
        .get("info")
        .and_then(|samples| samples.first())
        .map(|sample| sample.labels.clone())
        .unwrap_or_default();
    let up = up >= 1.0;
    let status = if up { Level::Ok } else { Level::Critical };
    let station = Station {
        up,
        status,
        version: info.get("version").cloned().unwrap_or_default(),
        model: info.get("model").cloned().unwrap_or_default(),
        latency_ms: results
            .scalar("latency")
            .filter(|_| up)
            .map(|seconds| (seconds * 1000.0).round()),
        wifi_rssi_dbm: results.scalar("rssi").filter(|_| up),
        temp_celsius: results.scalar("temp").filter(|_| up),
        uptime_seconds: results.scalar("uptime").filter(|_| up),
    };
    (status, Some(station))
}

#[must_use]
pub fn devices(station_results: &Results, phones: Option<Vec<Device>>) -> Devices {
    let (status, station) = station(station_results);
    let phones_available = phones.is_some();
    let mut phones: Vec<Phone> = phones
        .unwrap_or_default()
        .into_iter()
        .map(|device| Phone {
            id: device.id,
            name: device.name,
            platform: device.platform,
        })
        .collect();
    phones.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.id.cmp(&right.id))
    });
    Devices {
        status,
        station,
        phones_available,
        phones,
    }
}

#[must_use]
pub fn hosts(host_results: &Results, guest_results: &Results) -> Hosts {
    let (status, host) = host(host_results);
    let (guests_status, guests) = guests(guest_results);
    Hosts {
        status,
        guests_status,
        host,
        guests_available: guests.is_some(),
        guests: guests.unwrap_or_default(),
    }
}

const QUIET_ALERTS: &[&str] = &["Watchdog", "InfoInhibitor"];

#[must_use]
pub fn alerts(alerts: &[Alert]) -> Vec<AlertItem> {
    let mut items: Vec<AlertItem> = alerts
        .iter()
        .filter(|alert| {
            let name = alert.labels.get("alertname").map_or("", String::as_str);
            !QUIET_ALERTS.contains(&name)
                && alert.labels.get("severity").map_or("", String::as_str) != "none"
        })
        .map(|alert| AlertItem {
            name: alert.labels.get("alertname").cloned().unwrap_or_default(),
            severity: alert.labels.get("severity").cloned().unwrap_or_default(),
            summary: alert.summary(),
            started_at: alert.starts_at.map(jiff::Timestamp::as_second),
        })
        .collect();
    items.sort_by(|left, right| {
        alert_level(&right.severity)
            .cmp(&alert_level(&left.severity))
            .then(right.started_at.cmp(&left.started_at))
            .then_with(|| left.name.cmp(&right.name))
    });
    items
}

#[must_use]
pub fn alert_level(severity: &str) -> Level {
    match severity {
        "critical" => Level::Critical,
        "warning" => Level::Warning,
        _ => Level::Ok,
    }
}
