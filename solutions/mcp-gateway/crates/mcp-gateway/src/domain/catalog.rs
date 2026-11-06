//! Deployment catalog: YAML schema, validation and spoken-name resolution.
//!
//! Ports `internal/catalog/catalog.go` one-to-one, including the exact
//! validation messages: the catalog is deployment data, never solution data,
//! and a rejected catalog must explain itself the same way it did in Go.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Workload kinds. The only mutation the gateway performs is a rollout
/// restart, so these three are the complete set.
pub const KIND_DEPLOYMENT: &str = "Deployment";
/// See [`KIND_DEPLOYMENT`].
pub const KIND_STATEFUL_SET: &str = "StatefulSet";
/// See [`KIND_DEPLOYMENT`].
pub const KIND_DAEMON_SET: &str = "DaemonSet";

/// Kubernetes-backed service kind.
pub const SERVICE_K8S: &str = "k8s";
/// Read-only service outside Kubernetes.
pub const SERVICE_EXTERNAL: &str = "external";

/// Logs come from Loki.
pub const LOGS_LOKI: &str = "loki";
/// Logs come from `pods/log`.
pub const LOGS_PODS: &str = "pods";
/// Logs are not available.
pub const LOGS_NONE: &str = "none";

/// TCP check type.
pub const CHECK_TCP: &str = "tcp";
/// HTTP check type.
pub const CHECK_HTTP: &str = "http";
/// Home Assistant state check type.
pub const CHECK_HA_STATE: &str = "ha_state";
/// Home Assistant template check type.
pub const CHECK_HA_TEMPLATE: &str = "ha_template";
/// Home Assistant unavailable-entity summary check type.
pub const CHECK_HA_UNAVAILABLE: &str = "ha_unavailable_summary";

/// HTTP check without authentication.
pub const AUTH_NONE: &str = "none";
/// HTTP check with the Home Assistant token.
pub const AUTH_HA: &str = "ha";

/// Validation failure of a catalog document.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CatalogError {
    /// The YAML itself could not be decoded.
    #[error("parse catalog: {0}")]
    Parse(String),
    /// The decoded document breaks one or more schema rules.
    #[error("invalid catalog: {0}")]
    Invalid(String),
}

/// Spoken-name resolution failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ResolveError {
    /// The spoken name was empty.
    #[error("empty service name")]
    Empty,
    /// No service matched the spoken name.
    #[error("unknown service {0:?}")]
    Unknown(String),
}

/// One typed external probe for a service. Only the fields relevant to the
/// selected type are used; validation rejects everything else.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Check {
    /// Check type: `tcp`, `http`, `ha_state`, `ha_template` or
    /// `ha_unavailable_summary`.
    #[serde(rename = "type", default)]
    pub check_type: String,
    /// Human label shown in reports.
    #[serde(default)]
    pub label: String,
    /// TCP host.
    #[serde(default)]
    pub host: String,
    /// TCP port.
    #[serde(default)]
    pub port: i64,
    /// HTTP URL.
    #[serde(default)]
    pub url: String,
    /// Expected HTTP status; `0` means any 2xx/3xx.
    #[serde(default)]
    pub expect_status: i64,
    /// HTTP authentication mode: `none` or `ha`.
    #[serde(default)]
    pub auth: String,
    /// Home Assistant entity id for `ha_state`.
    #[serde(default)]
    pub entity_id: String,
    /// Entity states that count as a failure for `ha_state`.
    #[serde(default)]
    pub bad_states: Vec<String>,
    /// Home Assistant template for `ha_template`.
    #[serde(default)]
    pub template: String,
    /// Whether the check must succeed.
    #[serde(default)]
    pub required: bool,
    /// Domains included in `ha_unavailable_summary`.
    #[serde(default)]
    pub domains: Vec<String>,
}

impl Check {
    /// The human label used in reports: the configured label, otherwise a
    /// type-specific fallback.
    #[must_use]
    pub fn display_label(&self) -> String {
        let label = self.label.trim();
        if !label.is_empty() {
            return label.to_owned();
        }
        match self.check_type.as_str() {
            CHECK_TCP => format!("{}:{}", self.host, self.port),
            CHECK_HTTP => self.url.clone(),
            CHECK_HA_STATE => self.entity_id.clone(),
            CHECK_HA_TEMPLATE => "шаблон Home Assistant".to_owned(),
            CHECK_HA_UNAVAILABLE => "недоступные сущности Home Assistant".to_owned(),
            other => other.to_owned(),
        }
    }
}

/// Spoken names for a service. Both languages are optional, but at least one
/// alias across both lists is required.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Aliases {
    /// Russian aliases.
    #[serde(default)]
    pub ru: Vec<String>,
    /// English aliases.
    #[serde(default)]
    pub en: Vec<String>,
}

impl Aliases {
    /// Every alias, Russian first.
    #[must_use]
    pub fn all(&self) -> Vec<String> {
        let mut out = Vec::with_capacity(self.ru.len() + self.en.len());
        out.extend(self.ru.iter().cloned());
        out.extend(self.en.iter().cloned());
        out
    }
}

/// One Kubernetes workload.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workload {
    /// Deployment, StatefulSet or DaemonSet.
    #[serde(default)]
    pub kind: String,
    /// Object name.
    #[serde(default)]
    pub name: String,
}

/// One Prometheus query used as a health signal for the service.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signal {
    /// Instant PromQL query.
    #[serde(default)]
    pub promql: String,
    /// Short human explanation spoken in a verdict.
    #[serde(default)]
    pub meaning: String,
    /// Whether the signal participates in the `infra_status` aggregate.
    #[serde(default)]
    pub status: bool,
}

/// Where logs for the service come from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogSource {
    /// One of `loki`, `pods`, `none`.
    #[serde(default)]
    pub source: String,
    /// LogQL selector used when `source` is `loki`.
    #[serde(default)]
    pub selector: String,
    /// Containers used by `pods/log`.
    #[serde(default)]
    pub containers: Vec<String>,
}

/// Restart policy of the service.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Restart {
    /// Enables `infra_prepare_restart`/`infra_confirm` for this service.
    #[serde(default)]
    pub allowed: bool,
    /// Defers the restart so the reply can be spoken first.
    #[serde(default)]
    pub self_disrupting: bool,
    /// Spoken inside the confirmation question.
    #[serde(default)]
    pub warning: String,
}

/// One catalog entry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Service {
    /// Stable identifier and dedupe key.
    #[serde(default)]
    pub id: String,
    /// Human display name used in reports, status and speech.
    #[serde(default)]
    pub title: String,
    /// Report section this service belongs to.
    #[serde(default)]
    pub group: String,
    /// Human display name of the external host.
    #[serde(default)]
    pub host_title: String,
    /// Spoken names.
    #[serde(default)]
    pub aliases: Aliases,
    /// `k8s` or `external`.
    #[serde(default)]
    pub kind: String,
    /// Kubernetes namespace for `k8s`.
    #[serde(default)]
    pub namespace: String,
    /// Kubernetes workload for `k8s`.
    #[serde(default)]
    pub workload: Workload,
    /// Containers used for logs and image selection.
    #[serde(default)]
    pub containers: Vec<String>,
    /// Prometheus health signals.
    #[serde(default)]
    pub signals: Vec<Signal>,
    /// Typed external probes.
    #[serde(default)]
    pub checks: Vec<Check>,
    /// Log source.
    #[serde(default)]
    pub logs: LogSource,
    /// Restart policy.
    #[serde(default)]
    pub restart: Restart,
    /// Free-form description shown in `infra://catalog`.
    #[serde(default)]
    pub description: String,
    /// Aliases in catalog order (ru then en); built by [`Catalog::parse`].
    #[serde(skip)]
    pub alias_index: Vec<String>,
    /// Normalized aliases; built by [`Catalog::parse`].
    #[serde(skip)]
    pub normalized: Vec<String>,
}

impl Service {
    /// Best spoken name: the title, then the first Russian alias, then the
    /// first English alias, then the id.
    #[must_use]
    pub fn display_name(&self) -> String {
        let title = self.title.trim();
        if !title.is_empty() {
            return title.to_owned();
        }
        if let Some(alias) = self.aliases.ru.first() {
            return capitalize(alias);
        }
        if let Some(alias) = self.aliases.en.first() {
            return capitalize(alias);
        }
        capitalize(&self.id)
    }
}

/// The whole catalog file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Catalog {
    /// Sanitized free-form markdown about the topology.
    #[serde(default)]
    pub overview: String,
    /// Alert names that are expected to fire permanently.
    #[serde(default)]
    pub ignore_alerts: Vec<String>,
    /// Fixed report group order.
    #[serde(default)]
    pub group_order: Vec<String>,
    /// Allow-list of services.
    #[serde(default)]
    pub services: Vec<Service>,
}

impl Catalog {
    /// Decodes and validates catalog YAML.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogError::Parse`] for malformed YAML and
    /// [`CatalogError::Invalid`] for a document that breaks a schema rule.
    pub fn parse(data: &str) -> Result<Self, CatalogError> {
        let mut catalog: Self =
            serde_yaml::from_str(data).map_err(|error| CatalogError::Parse(error.to_string()))?;
        catalog.validate()?;
        catalog.prepare();
        Ok(catalog)
    }

    /// Returns every schema problem it can find, joined into one error.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogError::Invalid`] when at least one rule is broken.
    pub fn validate(&self) -> Result<(), CatalogError> {
        let mut problems: Vec<String> = Vec::new();
        if self.services.is_empty() {
            problems.push("catalog has no services".to_owned());
        }
        for (index, name) in self.ignore_alerts.iter().enumerate() {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                problems.push(format!("ignore_alerts[{index}]: alert name is required"));
                continue;
            }
            let key = trimmed.to_lowercase();
            if let Some(first) = self
                .ignore_alerts
                .iter()
                .take(index)
                .position(|previous| previous.trim().to_lowercase() == key)
            {
                problems.push(format!(
                    "ignore_alerts[{index}]: duplicate alert {trimmed:?} (first at ignore_alerts[{first}])"
                ));
            }
        }
        for (index, group) in self.group_order.iter().enumerate() {
            let trimmed = group.trim();
            if trimmed.is_empty() {
                problems.push(format!("group_order[{index}]: group name is required"));
                continue;
            }
            if let Some(first) = self
                .group_order
                .iter()
                .take(index)
                .position(|previous| previous.trim() == trimmed)
            {
                problems.push(format!(
                    "group_order[{index}]: duplicate group {trimmed:?} (first at group_order[{first}])"
                ));
            }
        }

        let mut seen: Vec<(String, usize)> = Vec::new();
        for (index, service) in self.services.iter().enumerate() {
            let mut prefix = format!("services[{index}]");
            if service.id.is_empty() {
                problems.push(format!("{prefix}: id is required"));
            } else {
                prefix = format!("service {}", service.id);
                if let Some((_, first)) = seen.iter().find(|(id, _)| *id == service.id) {
                    problems.push(format!(
                        "{prefix}: duplicate id (first at services[{first}])"
                    ));
                } else {
                    seen.push((service.id.clone(), index));
                }
            }
            for (name, value) in [
                ("title", &service.title),
                ("group", &service.group),
                ("host_title", &service.host_title),
            ] {
                if !value.is_empty() && value.trim().is_empty() {
                    problems.push(format!("{prefix}: {name} must not be blank"));
                }
            }
            if service.aliases.ru.is_empty() && service.aliases.en.is_empty() {
                problems.push(format!("{prefix}: at least one ru or en alias is required"));
            }
            if service
                .aliases
                .all()
                .iter()
                .any(|alias| alias.trim().is_empty())
            {
                problems.push(format!("{prefix}: aliases must not contain empty entries"));
            }

            match service.kind.as_str() {
                SERVICE_K8S => {
                    if service.namespace.is_empty() {
                        problems.push(format!("{prefix}: namespace is required for kind k8s"));
                    }
                    if !matches!(
                        service.workload.kind.as_str(),
                        KIND_DEPLOYMENT | KIND_STATEFUL_SET | KIND_DAEMON_SET
                    ) {
                        problems.push(format!(
                            "{prefix}: workload.kind must be Deployment, StatefulSet or DaemonSet (got {:?})",
                            service.workload.kind
                        ));
                    }
                    if service.workload.name.is_empty() {
                        problems.push(format!("{prefix}: workload.name is required for kind k8s"));
                    }
                }
                SERVICE_EXTERNAL => {
                    if !service.workload.name.is_empty() || !service.namespace.is_empty() {
                        problems.push(format!(
                            "{prefix}: external services must not declare a workload or namespace"
                        ));
                    }
                    if service.restart.allowed {
                        problems.push(format!("{prefix}: external services cannot allow restart"));
                    }
                    if service.logs.source == LOGS_PODS {
                        problems.push(format!("{prefix}: external services cannot read pods/log"));
                    }
                }
                other => problems.push(format!(
                    "{prefix}: kind must be k8s or external (got {other:?})"
                )),
            }

            for (signal_index, signal) in service.signals.iter().enumerate() {
                if signal.promql.trim().is_empty() {
                    problems.push(format!(
                        "{prefix}: signals[{signal_index}].promql is required"
                    ));
                }
                if signal.meaning.trim().is_empty() {
                    problems.push(format!(
                        "{prefix}: signals[{signal_index}].meaning is required"
                    ));
                }
            }
            for (check_index, check) in service.checks.iter().enumerate() {
                problems.extend(validate_check(&prefix, check_index, check));
            }

            match service.logs.source.as_str() {
                "" | LOGS_NONE => {}
                LOGS_LOKI => {
                    if service.logs.selector.trim().is_empty() {
                        problems.push(format!(
                            "{prefix}: logs.selector is required for source loki"
                        ));
                    }
                }
                LOGS_PODS => {
                    if service.kind != SERVICE_K8S {
                        problems.push(format!("{prefix}: logs.source pods requires kind k8s"));
                    }
                }
                other => problems.push(format!(
                    "{prefix}: logs.source must be loki, pods or none (got {other:?})"
                )),
            }
            if service.restart.allowed && service.restart.warning.trim().is_empty() {
                problems.push(format!(
                    "{prefix}: restart.warning is required when restart is allowed"
                ));
            }
        }

        if problems.is_empty() {
            return Ok(());
        }
        problems.sort();
        Err(CatalogError::Invalid(problems.join("; ")))
    }

    fn prepare(&mut self) {
        for name in &mut self.ignore_alerts {
            *name = name.trim().to_owned();
        }
        for service in &mut self.services {
            service.alias_index = service.aliases.all();
            service.normalized = service
                .alias_index
                .iter()
                .map(|alias| normalize(alias))
                .collect();
        }
    }

    /// Reports whether `infra_status` should treat the named alert as expected
    /// background noise. Matching ignores surrounding spaces and case.
    #[must_use]
    pub fn ignores_alert(&self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() {
            return false;
        }
        let lowered = name.to_lowercase();
        self.ignore_alerts
            .iter()
            .any(|ignored| ignored.trim().to_lowercase() == lowered)
    }

    /// Returns the service with the given id.
    #[must_use]
    pub fn find(&self, id: &str) -> Option<&Service> {
        self.services.iter().find(|service| service.id == id)
    }

    /// Maps a spoken name to exactly one service.
    ///
    /// Matching is tolerant: case, ё/е and punctuation are ignored, then an
    /// exact normalized match wins, then a unique substring match, then a
    /// unique one-edit-distance match for names long enough to make that safe.
    ///
    /// # Errors
    ///
    /// Returns [`ResolveError`] for an empty, ambiguous or unknown name.
    pub fn resolve(&self, name: &str) -> Result<&Service, ResolveError> {
        let query = normalize(name);
        if query.is_empty() {
            return Err(ResolveError::Empty);
        }
        if let Some(service) = self
            .services
            .iter()
            .find(|service| service.normalized.contains(&query))
        {
            return Ok(service);
        }
        let substring = |service: &Service| {
            service
                .normalized
                .iter()
                .any(|alias| alias.contains(&query) || query.contains(alias))
        };
        if let Some(service) = unique_match(&self.services, substring) {
            return Ok(service);
        }
        if query.chars().count() >= 5 {
            let one_edit = |service: &Service| {
                service
                    .normalized
                    .iter()
                    .any(|alias| within_one_edit(&query, alias))
            };
            if let Some(service) = unique_match(&self.services, one_edit) {
                return Ok(service);
            }
        }
        Err(ResolveError::Unknown(name.trim().to_owned()))
    }

    /// The overview markdown, or a short fallback.
    #[must_use]
    pub fn overview_text(&self) -> String {
        if !self.overview.trim().is_empty() {
            return self.overview.clone();
        }
        "Каталог сервисов без описания топологии.".to_owned()
    }

    /// Renders the catalog as human-readable markdown for the model.
    #[must_use]
    pub fn catalog_text(&self) -> String {
        let mut out = String::from("# Каталог сервисов\n\n");
        if !self.ignore_alerts.is_empty() {
            let _ = writeln!(
                out,
                "Ожидаемо всегда активные алерты (не считаются проблемой): {}\n",
                self.ignore_alerts.join(", ")
            );
        }
        for service in &self.services {
            let _ = writeln!(out, "## {}", service.id);
            if !service.title.is_empty() {
                let _ = writeln!(out, "- title: {}", service.title);
            }
            if !service.group.is_empty() {
                let _ = writeln!(out, "- group: {}", service.group);
            }
            if !service.description.is_empty() {
                let _ = writeln!(out, "{}", service.description);
            }
            let _ = writeln!(out, "- aliases: {}", service.alias_index.join(", "));
            if service.kind == SERVICE_K8S {
                let _ = writeln!(
                    out,
                    "- workload: {} {}/{}",
                    service.workload.kind, service.namespace, service.workload.name
                );
            } else {
                out.push_str("- workload: external (read-only signals)\n");
            }
            if service.restart.allowed {
                let _ = writeln!(
                    out,
                    "- restart: allowed; warning: {}",
                    service.restart.warning
                );
                if service.restart.self_disrupting {
                    out.push_str("- restart is deferred (self-disrupting)\n");
                }
            } else {
                out.push_str("- restart: not allowed\n");
            }
            for signal in &service.signals {
                let _ = writeln!(out, "- signal ({}): {}", signal.meaning, signal.promql);
            }
            for check in &service.checks {
                let _ = writeln!(
                    out,
                    "- check {}: {}",
                    check.check_type,
                    check.display_label()
                );
            }
            out.push('\n');
        }
        out
    }

    /// Only the Kubernetes-backed services.
    #[must_use]
    pub fn k8s_services(&self) -> Vec<&Service> {
        self.services
            .iter()
            .filter(|service| service.kind == SERVICE_K8S)
            .collect()
    }
}

fn validate_check(prefix: &str, index: usize, check: &Check) -> Vec<String> {
    let name = format!("{prefix}: checks[{index}]");
    let mut problems = Vec::new();
    if check.label.trim().is_empty() {
        problems.push(format!("{name}.label is required"));
    }
    match check.check_type.as_str() {
        CHECK_TCP => {
            if check.host.trim().is_empty() {
                problems.push(format!("{name}.host is required for type tcp"));
            }
            if !(1..=65535).contains(&check.port) {
                problems.push(format!("{name}.port must be in 1..65535"));
            }
        }
        CHECK_HTTP => {
            if check.url.trim().is_empty() {
                problems.push(format!("{name}.url is required for type http"));
            }
            if check.expect_status != 0 && !(100..=599).contains(&check.expect_status) {
                problems.push(format!("{name}.expect_status must be in 100..599"));
            }
            if !matches!(check.auth.as_str(), "" | AUTH_NONE | AUTH_HA) {
                problems.push(format!(
                    "{name}.auth must be none or ha (got {:?})",
                    check.auth
                ));
            }
        }
        CHECK_HA_STATE => {
            if check.entity_id.trim().is_empty() {
                problems.push(format!("{name}.entity_id is required for type ha_state"));
            }
            if check.bad_states.iter().any(|state| state.trim().is_empty()) {
                problems.push(format!("{name}.bad_states must not contain empty entries"));
            }
        }
        CHECK_HA_TEMPLATE => {
            if check.template.trim().is_empty() {
                problems.push(format!("{name}.template is required for type ha_template"));
            }
        }
        CHECK_HA_UNAVAILABLE => {
            if check.domains.iter().any(|domain| domain.trim().is_empty()) {
                problems.push(format!("{name}.domains must not contain empty entries"));
            }
        }
        other => problems.push(format!(
            "{name}.type must be tcp, http, ha_state, ha_template or ha_unavailable_summary (got {other:?})"
        )),
    }
    problems
}

fn unique_match(services: &[Service], matches: impl Fn(&Service) -> bool) -> Option<&Service> {
    let mut found = services.iter().filter(|service| matches(service));
    let first = found.next()?;
    if found.next().is_some() {
        return None;
    }
    Some(first)
}

/// Lowercases a spoken name, folds ё to е and replaces any run of
/// non-alphanumeric runes with a single space.
#[must_use]
pub fn normalize(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut space = true;
    for r in raw.trim().to_lowercase().chars() {
        let r = if r == 'ё' { 'е' } else { r };
        if r.is_alphabetic() || r.is_numeric() {
            out.push(r);
            space = false;
            continue;
        }
        if !space {
            out.push(' ');
            space = true;
        }
    }
    out.trim().to_owned()
}

fn within_one_edit(a: &str, b: &str) -> bool {
    let left: Vec<char> = a.chars().collect();
    let right: Vec<char> = b.chars().collect();
    if left.len().abs_diff(right.len()) > 1 {
        return false;
    }
    let (short, long) = if left.len() > right.len() {
        (right.as_slice(), left.as_slice())
    } else {
        (left.as_slice(), right.as_slice())
    };
    let (mut i, mut j) = (0usize, 0usize);
    let mut edits = 0usize;
    while i < short.len() && j < long.len() {
        if short[i] == long[j] {
            i += 1;
            j += 1;
            continue;
        }
        edits += 1;
        if edits > 1 {
            return false;
        }
        if short.len() == long.len() {
            i += 1;
            j += 1;
        } else {
            j += 1;
        }
    }
    edits + (short.len() - i) + (long.len() - j) <= 1
}

fn capitalize(raw: &str) -> String {
    let trimmed = raw.trim();
    let mut chars = trimmed.chars();
    let Some(first) = chars.next() else {
        return raw.to_owned();
    };
    let mut out = String::new();
    if let Some(upper) = first.to_uppercase().next() {
        out.push(upper);
    }
    out.push_str(chars.as_str());
    out
}
