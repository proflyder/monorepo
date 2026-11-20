use std::fmt::Write as _;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use serde::{Deserialize, Serialize};

use super::alert::{self, Alert, AlertDetail};
use super::catalog::{self, Catalog, Service};
use super::status::{CheckResult, check_issue};

pub const REPORT_TEXT_CAP: usize = 3500;

pub const PLATFORM_GROUP: &str = "Платформа";
pub const SERVICE_FLUX: &str = "flux";
pub const SERVICE_CATALOG: &str = "catalog";

pub const STATUS_OK: &str = "ok";
pub const STATUS_DEGRADED: &str = "degraded";
pub const STATUS_DOWN: &str = "down";
pub const STATUS_UNKNOWN: &str = "unknown";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkloadState {
    pub namespace: String,
    pub kind: String,
    pub name: String,
    pub generation: i64,
    pub observed_generation: i64,
    pub desired: i64,
    pub updated: i64,
    pub ready: i64,
    pub available: i64,
}

impl WorkloadState {
    #[must_use]
    pub fn available_ok(&self) -> bool {
        if self.desired == 0 {
            return true;
        }
        self.available >= self.desired && self.ready >= self.desired
    }

    #[must_use]
    pub fn rollout_in_progress(&self) -> bool {
        if self.observed_generation != 0
            && self.generation != 0
            && self.observed_generation < self.generation
        {
            return true;
        }
        self.updated != 0 && self.desired != 0 && self.updated < self.desired
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerState {
    pub name: String,
    #[serde(default)]
    pub image: String,
    #[serde(default)]
    pub ready: bool,
    #[serde(default)]
    pub restart_count: i32,
    #[serde(default)]
    pub waiting_reason: String,
    #[serde(skip)]
    pub last_terminated_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PodState {
    pub namespace: String,
    pub name: String,
    #[serde(default)]
    pub ready: bool,
    #[serde(skip)]
    pub start_time: Option<Timestamp>,
    #[serde(default)]
    pub containers: Vec<ContainerState>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServiceProbe {
    pub workload: Option<Result<WorkloadState, String>>,
    pub pods: Option<Result<Vec<PodState>, String>>,
    pub checks: Vec<CheckResult>,
    pub checks_configured: bool,
}

#[derive(Debug, Clone)]
pub struct ReportInput<'a> {
    pub catalog: &'a Catalog,
    pub now: Timestamp,
    pub timezone: TimeZone,
    pub probes: Vec<ServiceProbe>,
    pub alerts: Vec<Alert>,
    pub errors: Vec<String>,
    pub verbose: bool,
    pub workloads: Option<Result<Vec<WorkloadRef>, String>>,
    pub flux: Option<Result<Vec<FluxResource>, String>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkloadRef {
    pub namespace: String,
    pub kind: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FluxResource {
    pub kind: String,
    pub namespace: String,
    pub name: String,
    pub ready: bool,
    pub suspended: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceReport {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub group: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub namespace: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub ready: i64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub desired: i64,
    pub restarts_last_hour: i64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub image: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub age: String,
    pub checks: Vec<CheckResult>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alerts: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

fn is_zero(value: &i64) -> bool {
    *value == 0
}

fn is_empty_ok<T>(result: &Result<Vec<T>, String>) -> bool {
    matches!(result, Ok(items) if items.is_empty())
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportDetail {
    pub verdict: String,
    pub checked_at: String,
    pub total: i64,
    pub ok: i64,
    pub problems: i64,
    pub unknown: i64,
    pub alerts: Vec<AlertDetail>,
    pub services: Vec<ServiceReport>,
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}

#[must_use]
pub fn build_report(input: &ReportInput<'_>) -> ReportDetail {
    assert_eq!(
        input.probes.len(),
        input.catalog.services.len(),
        "one probe per catalog service"
    );
    let mut detail = ReportDetail {
        checked_at: input.now.to_string(),
        total: i64::try_from(input.catalog.services.len()).unwrap_or(i64::MAX),
        alerts: Vec::new(),
        services: input
            .catalog
            .services
            .iter()
            .zip(&input.probes)
            .map(|(service, probe)| collect_service_report(service, probe, input.now))
            .collect(),
        ..ReportDetail::default()
    };
    detail.errors.clone_from(&input.errors);

    if let Some(flux) = &input.flux
        && !is_empty_ok(flux)
    {
        detail.services.push(collect_flux_report(flux));
    }
    if let Some(workloads) = &input.workloads
        && !is_empty_ok(workloads)
    {
        detail
            .services
            .push(collect_coverage_report(input.catalog, workloads));
    }
    detail.total = i64::try_from(detail.services.len()).unwrap_or(i64::MAX);

    let compacts = input
        .alerts
        .iter()
        .filter(|alert| !input.catalog.ignores_alert(&alert.name()))
        .map(alert::compact_alert)
        .collect();
    detail.alerts = alert::dedupe_alerts(compacts);

    for report in &mut detail.services {
        let Some(service) = input.catalog.find(&report.id) else {
            continue;
        };
        for alert in &input.alerts {
            if alert::alert_matches(alert, service) {
                let summary = alert.summary();
                report.alerts.push(summary.clone());
                report.issues.push(format!("алерт: {summary}"));
                if report.status == STATUS_OK {
                    STATUS_DEGRADED.clone_into(&mut report.status);
                }
            }
        }
    }

    for report in &detail.services {
        match report.status.as_str() {
            STATUS_OK => detail.ok += 1,
            STATUS_UNKNOWN => {
                if report.kind == catalog::SERVICE_K8S {
                    detail.problems += 1;
                } else {
                    detail.unknown += 1;
                }
            }
            _ => detail.problems += 1,
        }
    }
    detail.verdict = report_verdict(&detail);
    detail.text = format_service_report(&detail, input.catalog, input.verbose, &input.timezone);
    detail
}

fn collect_service_report(
    service: &Service,
    probe: &ServiceProbe,
    now: Timestamp,
) -> ServiceReport {
    let mut item = ServiceReport {
        id: service.id.clone(),
        name: service.display_name(),
        group: service_group(service),
        kind: service.kind.clone(),
        namespace: service.namespace.clone(),
        status: STATUS_OK.to_owned(),
        checks: Vec::new(),
        ..ServiceReport::default()
    };
    if service.kind == catalog::SERVICE_K8S {
        item = collect_workload_report(service, probe, item, now);
    }
    if probe.checks_configured && !service.checks.is_empty() {
        item.checks.clone_from(&probe.checks);
    }
    classify_service(&mut item, service);
    for check in &item.checks {
        if check.check_type == catalog::CHECK_HA_TEMPLATE
            && check.ok
            && !check.value.is_empty()
            && check.value != "нет данных"
        {
            item.versions
                .push(format!("{}: {}", check.label, check.value));
        }
    }
    if item.status == STATUS_UNKNOWN && !item.checks.is_empty() {
        let all_skipped = item.checks.iter().all(|check| check.skipped);
        if all_skipped {
            item.issues.push("проверки не настроены".to_owned());
        }
    }
    item
}

fn collect_workload_report(
    service: &Service,
    probe: &ServiceProbe,
    mut item: ServiceReport,
    now: Timestamp,
) -> ServiceReport {
    let Some(workload) = probe.workload.as_ref() else {
        STATUS_UNKNOWN.clone_into(&mut item.status);
        item.issues.push("Kubernetes API не настроен".to_owned());
        return item;
    };
    let workload = match workload {
        Ok(workload) => workload,
        Err(error) => {
            STATUS_UNKNOWN.clone_into(&mut item.status);
            item.issues.push(error.clone());
            return item;
        }
    };
    item.ready = workload.ready;
    item.desired = workload.desired;
    match probe.pods.as_ref() {
        Some(Ok(pods)) => {
            let cutoff = now.as_second() - 3600;
            let mut oldest: Option<Timestamp> = None;
            for pod in pods {
                if item.image.is_empty() {
                    item.image = short_image(&pick_image(pod, service));
                }
                if let Some(start) = pod.start_time
                    && oldest.is_none_or(|current| start < current)
                {
                    oldest = Some(start);
                }
                for container in &pod.containers {
                    if container
                        .last_terminated_at
                        .is_some_and(|terminated| terminated.as_second() > cutoff)
                    {
                        item.restarts_last_hour += 1;
                    }
                }
            }
            if let Some(oldest) = oldest {
                item.age = human_age(now.as_second() - oldest.as_second());
            }
        }
        Some(Err(error)) => item.issues.push(format!("поды: {error}")),
        None => {}
    }
    if !workload.available_ok() {
        item.status = if workload.ready == 0 {
            STATUS_DOWN.to_owned()
        } else {
            STATUS_DEGRADED.to_owned()
        };
        item.issues
            .push(format!("готовы {} из {}", workload.ready, workload.desired));
    }
    item
}

fn collect_flux_report(flux: &Result<Vec<FluxResource>, String>) -> ServiceReport {
    let mut item = ServiceReport {
        id: SERVICE_FLUX.to_owned(),
        name: "Flux".to_owned(),
        group: PLATFORM_GROUP.to_owned(),
        kind: SERVICE_FLUX.to_owned(),
        namespace: "flux-system".to_owned(),
        status: STATUS_OK.to_owned(),
        ..ServiceReport::default()
    };
    let resources = match flux {
        Ok(resources) => resources,
        Err(error) => {
            STATUS_UNKNOWN.clone_into(&mut item.status);
            item.issues
                .push(format!("проверка Flux недоступна: {error}"));
            return item;
        }
    };
    let mut suspended = 0usize;
    for resource in resources {
        if resource.suspended {
            suspended += 1;
            continue;
        }
        if resource.ready {
            continue;
        }
        if item.status == STATUS_OK {
            STATUS_DEGRADED.clone_into(&mut item.status);
        }
        let message = if resource.message.trim().is_empty() {
            "не Ready".to_owned()
        } else {
            resource.message.trim().to_owned()
        };
        item.issues.push(format!(
            "{} {}/{}: {message}",
            resource.kind, resource.namespace, resource.name
        ));
    }
    if suspended > 0 {
        item.note = format!("приостановлено {suspended}");
    }
    item
}

fn collect_coverage_report(
    catalog: &Catalog,
    workloads: &Result<Vec<WorkloadRef>, String>,
) -> ServiceReport {
    let mut item = ServiceReport {
        id: SERVICE_CATALOG.to_owned(),
        name: "Каталог отчёта".to_owned(),
        group: PLATFORM_GROUP.to_owned(),
        kind: SERVICE_CATALOG.to_owned(),
        status: STATUS_OK.to_owned(),
        ..ServiceReport::default()
    };
    let listed = match workloads {
        Ok(listed) => listed,
        Err(error) => {
            STATUS_UNKNOWN.clone_into(&mut item.status);
            item.issues
                .push(format!("список workload'ов недоступен: {error}"));
            return item;
        }
    };
    let registered = |namespace: &str, kind: &str, name: &str| {
        catalog.services.iter().any(|service| {
            service.kind == catalog::SERVICE_K8S
                && service.namespace == namespace
                && service.workload.kind == kind
                && service.workload.name == name
        })
    };
    let mut missing = 0usize;
    for workload in listed {
        if registered(&workload.namespace, &workload.kind, &workload.name)
            || catalog.ignores_workload(&workload.namespace, &workload.kind, &workload.name)
        {
            continue;
        }
        missing += 1;
        item.issues.push(format!(
            "не в каталоге отчёта: {}/{}/{}",
            workload.namespace, workload.kind, workload.name
        ));
    }
    if missing > 0 {
        STATUS_DEGRADED.clone_into(&mut item.status);
    }
    item
}

fn classify_service(item: &mut ServiceReport, service: &Service) {
    let mut evaluated = 0i64;
    let mut failed = 0i64;
    for check in &item.checks {
        if check.skipped {
            continue;
        }
        evaluated += 1;
        if check.ok {
            continue;
        }
        failed += 1;
        item.issues.push(check_issue(check));
    }
    if failed > 0 {
        if service.kind == catalog::SERVICE_EXTERNAL && failed == evaluated {
            STATUS_DOWN.clone_into(&mut item.status);
        } else if item.status == STATUS_OK {
            STATUS_DEGRADED.clone_into(&mut item.status);
        }
    }
    if item.restarts_last_hour > 0 && item.status == STATUS_OK {
        STATUS_DEGRADED.clone_into(&mut item.status);
        item.issues
            .push(format!("рестартов за час: {}", item.restarts_last_hour));
    }
    if service.kind == catalog::SERVICE_EXTERNAL && evaluated == 0 {
        STATUS_UNKNOWN.clone_into(&mut item.status);
    }
}

fn report_verdict(detail: &ReportDetail) -> String {
    if detail.problems == 0 {
        let mut verdict = format!("Всё работает: {} из {}", detail.ok, detail.total);
        if detail.unknown > 0 {
            let _ = write!(verdict, ", не проверено {}", detail.unknown);
        }
        return verdict + ". Прислать подробности в Telegram?";
    }
    let names: Vec<&str> = detail
        .services
        .iter()
        .filter(|service| service.status != STATUS_OK)
        .map(|service| service.name.as_str())
        .collect();
    let shown = &names[..names.len().min(2)];
    let suffix = if names.len() > shown.len() {
        format!(" и ещё {}", names.len() - shown.len())
    } else {
        String::new()
    };
    format!(
        "Проверено сервисов: {}, работают {}. Проблемы: {}{}. Прислать подробности в Telegram?",
        detail.total,
        detail.ok,
        shown.join(", "),
        suffix
    )
}

fn report_headline(detail: &ReportDetail) -> String {
    if detail.problems == 0 {
        let mut line = format!("✅ Всё работает: {} из {}", detail.ok, detail.total);
        if detail.unknown > 0 {
            let _ = write!(line, ", не проверено {}", detail.unknown);
        }
        return line;
    }
    format!("⚠️ Есть проблемы: {} из {}", detail.problems, detail.total)
}

fn format_service_report(
    detail: &ReportDetail,
    catalog: &Catalog,
    verbose: bool,
    timezone: &TimeZone,
) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Отчёт по инфраструктуре, {}",
        human_stamp(&detail.checked_at, timezone)
    );
    out.push_str(&report_headline(detail));
    out.push('\n');
    if let Some(first) = detail.errors.first() {
        let _ = writeln!(out, "⚠️ Проверка неполная: {first}");
    }

    if detail.problems > 0 {
        out.push_str("\nПроблемы\n");
        for (_, services) in report_groups(detail, catalog) {
            for service in services {
                if is_problem(service) {
                    write_problem(&mut out, service);
                }
            }
        }
    }

    let normal = normal_group_lines(detail, catalog);
    if !normal.is_empty() {
        out.push_str("\nВ норме\n");
        for line in normal {
            out.push_str(&line);
            out.push('\n');
        }
    }

    if !detail.alerts.is_empty() {
        out.push_str("\nАлерты\n");
        for alert in &detail.alerts {
            out.push_str(&alert::alert_line(alert));
            out.push('\n');
        }
    }

    if verbose {
        let lines = version_lines(detail);
        if !lines.is_empty() {
            out.push_str("\nВерсии\n");
            for line in lines {
                out.push_str(&line);
                out.push('\n');
            }
        }
    }
    cap_report(out.trim())
}

fn is_problem(service: &ServiceReport) -> bool {
    if service.status == STATUS_OK {
        return false;
    }
    !(service.status == STATUS_UNKNOWN && service.kind == catalog::SERVICE_EXTERNAL)
}

fn write_problem(out: &mut String, service: &ServiceReport) {
    let marker = if service.status == STATUS_DOWN {
        "❌"
    } else {
        "⚠️"
    };
    let reason = problem_reason(service);
    let _ = writeln!(out, "{marker} {} — {reason}", service.name);
    let facts = problem_facts(service, &reason);
    if !facts.is_empty() {
        let _ = writeln!(out, "   {}", facts.join(", "));
    }
}

fn problem_reason(service: &ServiceReport) -> String {
    if let Some(issue) = service.issues.iter().find(|issue| !issue.trim().is_empty()) {
        return issue.clone();
    }
    if let Some(alert) = service.alerts.first() {
        return alert.clone();
    }
    match service.status.as_str() {
        STATUS_DOWN => "недоступен".to_owned(),
        STATUS_UNKNOWN => "состояние неизвестно".to_owned(),
        _ => "работает неполностью".to_owned(),
    }
}

fn problem_facts(service: &ServiceReport, reason: &str) -> Vec<String> {
    let mut facts: Vec<String> = Vec::new();
    if service.kind == catalog::SERVICE_K8S {
        if service.desired > 0 || service.ready > 0 {
            push_fact(
                &mut facts,
                &format!("готовы {} из {}", service.ready, service.desired),
                reason,
            );
        }
        push_fact(
            &mut facts,
            &format!("рестартов 1ч: {}", service.restarts_last_hour),
            reason,
        );
    }
    for check in &service.checks {
        if check.skipped || check.ok {
            continue;
        }
        push_fact(&mut facts, &check_issue(check), reason);
    }
    for alert in &service.alerts {
        push_fact(&mut facts, &format!("алерт: {alert}"), reason);
    }
    for issue in &service.issues {
        push_fact(&mut facts, issue, reason);
    }
    let limit = if service.kind == SERVICE_CATALOG {
        20
    } else {
        5
    };
    facts.truncate(limit);
    facts
}

fn push_fact(facts: &mut Vec<String>, value: &str, reason: &str) {
    let value = value.trim();
    if value.is_empty() || value == reason || facts.iter().any(|existing| existing == value) {
        return;
    }
    facts.push(value.to_owned());
}

type ReportGroup<'a> = (String, Vec<&'a ServiceReport>);

fn report_groups<'a>(detail: &'a ReportDetail, catalog: &Catalog) -> Vec<ReportGroup<'a>> {
    let mut groups: Vec<ReportGroup<'a>> = Vec::new();
    let mut index: Vec<(String, usize)> = Vec::new();
    for name in &catalog.group_order {
        for service in &detail.services {
            if service.group != *name {
                continue;
            }
            push_group(&mut groups, &mut index, name, service);
        }
    }
    for service in &detail.services {
        if catalog.group_order.contains(&service.group) {
            continue;
        }
        push_group(&mut groups, &mut index, &service.group, service);
    }
    groups
}

fn push_group<'a>(
    groups: &mut Vec<ReportGroup<'a>>,
    index: &mut Vec<(String, usize)>,
    name: &str,
    service: &'a ServiceReport,
) {
    let name = if name.is_empty() {
        "Прочее"
    } else {
        name
    };
    if let Some(position) = index.iter().find(|(existing, _)| existing == name) {
        groups[position.1].1.push(service);
        return;
    }
    index.push((name.to_owned(), groups.len()));
    groups.push((name.to_owned(), vec![service]));
}

fn normal_group_lines(detail: &ReportDetail, catalog: &Catalog) -> Vec<String> {
    let mut lines = Vec::new();
    for (name, services) in report_groups(detail, catalog) {
        let names: Vec<String> = services
            .iter()
            .filter(|service| service.status == STATUS_OK)
            .map(|service| {
                if service.note.is_empty() {
                    service.name.clone()
                } else {
                    format!("{} ({})", service.name, service.note)
                }
            })
            .collect();
        if names.is_empty() {
            continue;
        }
        if names.len() > 5 {
            lines.push(format!("✅ {name} ({})", names.len()));
            continue;
        }
        lines.push(format!("✅ {name}: {}", names.join(", ")));
    }
    lines
}

fn version_lines(detail: &ReportDetail) -> Vec<String> {
    let mut lines = Vec::new();
    for service in &detail.services {
        let mut facts: Vec<String> = Vec::new();
        if !service.image.is_empty() {
            facts.push(service.image.clone());
        }
        if !service.age.is_empty() {
            facts.push(format!("возраст {}", service.age));
        }
        facts.extend(service.versions.iter().cloned());
        if facts.is_empty() {
            continue;
        }
        lines.push(format!("{} — {}", service.name, facts.join(", ")));
    }
    lines
}

#[must_use]
pub fn service_group(service: &Service) -> String {
    let group = service.group.trim();
    if !group.is_empty() {
        return group.to_owned();
    }
    if service.kind == catalog::SERVICE_K8S {
        return format!("K3s / {}", service.namespace);
    }
    let host = service.host_title.trim();
    if !host.is_empty() {
        return host.to_owned();
    }
    for check in &service.checks {
        match check.check_type.as_str() {
            catalog::CHECK_TCP if !check.host.is_empty() => {
                return format!("LAN / {}", check.host);
            }
            catalog::CHECK_HTTP => {
                if let Some(host) = url_hostname(&check.url) {
                    return format!("LAN / {host}");
                }
            }
            _ => {}
        }
    }
    "Внешние".to_owned()
}

fn url_hostname(raw: &str) -> Option<String> {
    let (_, rest) = raw.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit('@').next()?;
    if let Some(inner) = host.strip_prefix('[') {
        return inner.split(']').next().map(str::to_owned);
    }
    Some(host.split(':').next().unwrap_or(host).to_owned()).filter(|host| !host.is_empty())
}

fn cap_report(text: &str) -> String {
    let runes: Vec<char> = text.chars().collect();
    if runes.len() <= REPORT_TEXT_CAP {
        return text.to_owned();
    }
    let cut: String = runes[..REPORT_TEXT_CAP].iter().collect();
    let cut = match cut.rfind('\n') {
        Some(index) if index > 0 => &cut[..index],
        _ => cut.as_str(),
    };
    format!("{}\n… отчёт сокращён", cut.trim())
}

#[must_use]
pub fn pick_image(pod: &PodState, service: &Service) -> String {
    if let Some(container) = service.containers.first()
        && let Some(image) = image_of_container(pod, container)
    {
        return image;
    }
    let name = service.workload.name.trim();
    if !name.is_empty() {
        let mut candidates = vec![name.to_owned()];
        if let Some((_, suffix)) = name
            .rsplit_once('-')
            .filter(|(_, suffix)| !suffix.is_empty())
        {
            candidates.push(suffix.to_owned());
        }
        for candidate in candidates {
            if let Some(image) = image_of_container(pod, &candidate) {
                return image;
            }
        }
    }
    pod.containers
        .iter()
        .find(|container| !container.image.is_empty())
        .map(|container| container.image.clone())
        .unwrap_or_default()
}

fn image_of_container(pod: &PodState, name: &str) -> Option<String> {
    pod.containers
        .iter()
        .find(|container| container.name == name)
        .map(|container| container.image.clone())
}

#[must_use]
pub fn short_image(image: &str) -> String {
    let image = image.trim();
    if image.is_empty() {
        return String::new();
    }
    if let Some(index) = image.find("@sha256:") {
        let digest = &image[index + "@sha256:".len()..];
        let digest = &digest[..digest.len().min(8)];
        let base = image[..index].rsplit('/').next().unwrap_or(&image[..index]);
        return format!("{base}@sha256:{digest}");
    }
    image.rsplit('/').next().unwrap_or(image).to_owned()
}

fn human_age(seconds: i64) -> String {
    if seconds >= 86_400 {
        format!("{}д", seconds / 86_400)
    } else if seconds >= 3_600 {
        format!("{}ч", seconds / 3_600)
    } else if seconds >= 60 {
        format!("{}м", seconds / 60)
    } else {
        format!("{seconds}с")
    }
}

fn human_stamp(raw: &str, timezone: &TimeZone) -> String {
    let Ok(instant) = raw.parse::<Timestamp>() else {
        return raw.to_owned();
    };
    instant
        .to_zoned(timezone.clone())
        .strftime("%d.%m %H:%M")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flux(kind: &str, name: &str, ready: bool, suspended: bool) -> FluxResource {
        FluxResource {
            kind: kind.to_owned(),
            namespace: "flux-system".to_owned(),
            name: name.to_owned(),
            ready,
            suspended,
            message: if ready {
                String::new()
            } else {
                "health check failed".to_owned()
            },
        }
    }

    #[test]
    fn flux_reports_every_not_ready_resource_with_its_kind_and_message() {
        let entry = collect_flux_report(&Ok(vec![
            flux("Kustomization", "apps", false, false),
            flux("HelmRelease", "runtime", false, false),
            flux("Kustomization", "paused", false, true),
            flux("Kustomization", "platform", true, false),
        ]));
        assert_eq!(entry.status, STATUS_DEGRADED);
        assert_eq!(entry.kind, SERVICE_FLUX);
        assert_eq!(entry.group, PLATFORM_GROUP);
        assert_eq!(
            entry.issues,
            vec![
                "Kustomization flux-system/apps: health check failed",
                "HelmRelease flux-system/runtime: health check failed",
            ]
        );
        assert_eq!(entry.note, "приостановлено 1");
    }

    #[test]
    fn flux_is_ok_when_everything_is_ready_or_suspended() {
        let entry = collect_flux_report(&Ok(vec![
            flux("Kustomization", "apps", true, false),
            flux("Kustomization", "paused", false, true),
        ]));
        assert_eq!(entry.status, STATUS_OK);
        assert!(entry.issues.is_empty());
        assert_eq!(entry.note, "приостановлено 1");
    }

    #[test]
    fn flux_read_error_is_unknown_and_visible() {
        let entry = collect_flux_report(&Err("HTTP 403".to_owned()));
        assert_eq!(entry.status, STATUS_UNKNOWN);
        assert_eq!(entry.issues, vec!["проверка Flux недоступна: HTTP 403"]);
        assert!(is_problem(&entry));
    }

    fn catalog(yaml: &str) -> Catalog {
        Catalog::parse(yaml).expect("catalog")
    }

    const ONE_SERVICE: &str = r#"
services:
  - id: scheduler
    title: Scheduler
    aliases: {en: [scheduler]}
    kind: k8s
    namespace: home-assistant
    workload: {kind: Deployment, name: scheduler}
    logs: {source: none}
    restart: {allowed: true, warning: "restart"}
"#;

    fn workload(namespace: &str, kind: &str, name: &str) -> WorkloadRef {
        WorkloadRef {
            namespace: namespace.to_owned(),
            kind: kind.to_owned(),
            name: name.to_owned(),
        }
    }

    #[test]
    fn coverage_names_workloads_missing_from_the_catalog() {
        let catalog = catalog(ONE_SERVICE);
        let entry = collect_coverage_report(
            &catalog,
            &Ok(vec![
                workload("home-assistant", "Deployment", "scheduler"),
                workload("home-assistant", "Deployment", "intercom"),
                workload("kube-system", "DaemonSet", "svclb-traefik"),
            ]),
        );
        assert_eq!(entry.status, STATUS_DEGRADED);
        assert_eq!(entry.kind, SERVICE_CATALOG);
        assert_eq!(
            entry.issues,
            vec![
                "не в каталоге отчёта: home-assistant/Deployment/intercom",
                "не в каталоге отчёта: kube-system/DaemonSet/svclb-traefik",
            ]
        );
    }

    #[test]
    fn coverage_respects_the_explicit_ignore_list() {
        let catalog = catalog(&format!(
            "{ONE_SERVICE}\nignore_workloads:\n  - kube-system/DaemonSet/svclb-traefik\n"
        ));
        let entry = collect_coverage_report(
            &catalog,
            &Ok(vec![
                workload("home-assistant", "Deployment", "scheduler"),
                workload("kube-system", "DaemonSet", "svclb-traefik"),
            ]),
        );
        assert_eq!(entry.status, STATUS_OK);
        assert!(entry.issues.is_empty());
    }

    #[test]
    fn coverage_read_error_is_a_problem() {
        let catalog = catalog(ONE_SERVICE);
        let entry = collect_coverage_report(&catalog, &Err("HTTP 403".to_owned()));
        assert_eq!(entry.status, STATUS_UNKNOWN);
        assert!(is_problem(&entry));
        assert_eq!(
            entry.issues,
            vec!["список workload'ов недоступен: HTTP 403"]
        );
    }

    #[test]
    fn report_total_counts_the_synthetic_checks() {
        let catalog = catalog(ONE_SERVICE);
        let now: Timestamp = "2026-11-01T12:00:00Z".parse().expect("time");
        let probe = ServiceProbe {
            workload: Some(Ok(WorkloadState {
                namespace: "home-assistant".to_owned(),
                kind: "Deployment".to_owned(),
                name: "scheduler".to_owned(),
                desired: 1,
                ready: 1,
                available: 1,
                updated: 1,
                ..WorkloadState::default()
            })),
            pods: Some(Ok(Vec::new())),
            ..ServiceProbe::default()
        };
        let detail = build_report(&ReportInput {
            catalog: &catalog,
            now,
            timezone: TimeZone::UTC,
            probes: vec![probe],
            alerts: Vec::new(),
            errors: Vec::new(),
            verbose: false,
            workloads: Some(Ok(vec![workload(
                "home-assistant",
                "Deployment",
                "scheduler",
            )])),
            flux: Some(Ok(vec![flux("Kustomization", "apps", true, false)])),
        });
        assert_eq!(detail.total, 3);
        assert_eq!(detail.ok, 3);
        assert_eq!(detail.problems, 0);
        assert!(detail.text.contains("✅ Всё работает: 3 из 3"));
        assert!(detail.text.contains("Платформа"));
    }
}
