//! Small read-mostly Kubernetes REST client.
//!
//! Ports `solutions/mcp-gateway/internal/kubernetes/client.go`. It talks to the
//! API server with the in-cluster ServiceAccount token and CA and performs
//! exactly one mutation: the rollout restart annotation patch.
//!
//! The projected ServiceAccount token rotates, so it is read from disk on every
//! request, not once at startup. The CA bundle is read at construction time.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::UtcOffset;
use time::format_description::well_known::Rfc3339;

use crate::ErrorKind;

/// In-cluster API server root.
pub const DEFAULT_API_URL: &str = "https://kubernetes.default.svc";
/// In-cluster projected ServiceAccount token.
pub const DEFAULT_TOKEN_FILE: &str = "/var/run/secrets/kubernetes.io/serviceaccount/token";
/// In-cluster CA bundle.
pub const DEFAULT_CA_FILE: &str = "/var/run/secrets/kubernetes.io/serviceaccount/ca.crt";

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
/// The Go client reads at most 4 MiB of any response.
const MAX_BODY_BYTES: usize = 4 << 20;

/// One Kubernetes REST call failure.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{detail}")]
    Invalid { detail: String },
    #[error("{operation}: {detail}")]
    Io {
        operation: &'static str,
        detail: String,
    },
    #[error("{operation}: request failed")]
    Transport {
        operation: &'static str,
        #[source]
        source: reqwest::Error,
    },
    #[error("{operation} returned HTTP {status}")]
    Status { operation: String, status: u16 },
    #[error("{operation}: response is not JSON")]
    Decode {
        operation: &'static str,
        #[source]
        source: serde_json::Error,
    },
    #[error("{operation}: unusable CA")]
    Certificate { operation: &'static str },
}

impl Error {
    /// Whether a retry may succeed.
    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::Transport { .. } => ErrorKind::Transient,
            Self::Status { status, .. } => match *status {
                408 | 425 | 429 => ErrorKind::Transient,
                status if status >= 500 => ErrorKind::Transient,
                _ => ErrorKind::Permanent,
            },
            Self::Invalid { .. }
            | Self::Io { .. }
            | Self::Decode { .. }
            | Self::Certificate { .. } => ErrorKind::Permanent,
        }
    }
}

/// Client options. Empty fields take the in-cluster defaults.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// API server root. Empty means [`DEFAULT_API_URL`].
    pub base_url: Option<String>,
    /// Static bearer token. When empty, `token_file` is read on every call.
    pub token: Option<String>,
    /// Projected token file. Defaults to [`DEFAULT_TOKEN_FILE`] when neither
    /// `token` nor `token_file` is set.
    pub token_file: Option<PathBuf>,
    /// CA bundle. Defaults to [`DEFAULT_CA_FILE`] when it exists.
    pub ca_file: Option<PathBuf>,
    /// Per-request timeout. Defaults to 10 s.
    pub timeout: Option<Duration>,
}

/// A Kubernetes REST client.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::Client,
    base_url: String,
    token: Option<String>,
    token_file: Option<PathBuf>,
}

impl Client {
    /// Build a client from options.
    pub fn new(options: Options) -> Result<Self, Error> {
        // The workspace `reqwest` has no default crypto provider; install it
        // before the first client. A provider installed earlier is kept.
        let _already_installed = rustls::crypto::ring::default_provider().install_default();
        let base_url = options
            .base_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_API_URL)
            .trim_end_matches('/')
            .to_owned();
        let token = options.token.filter(|value| !value.is_empty());
        let token_file = match (&token, options.token_file) {
            (None, Some(path)) => Some(path),
            (None, None) => Some(PathBuf::from(DEFAULT_TOKEN_FILE)),
            (Some(_), _) => None,
        };
        let ca_file = options
            .ca_file
            .unwrap_or_else(|| PathBuf::from(DEFAULT_CA_FILE));
        // The ServiceAccount CA is the only trust anchor: the workspace
        // `reqwest` verifies against the platform store by default, and relying
        // on the node's bundle would accept any publicly trusted certificate
        // for the API server. `tls_certs_only` replaces that store entirely.
        let certificates = if ca_file.exists() {
            let pem = std::fs::read(&ca_file).map_err(|_| Error::Certificate {
                operation: "read Kubernetes CA",
            })?;
            let certificates =
                reqwest::Certificate::from_pem_bundle(&pem).map_err(|_| Error::Certificate {
                    operation: "decode Kubernetes CA",
                })?;
            if certificates.is_empty() {
                return Err(Error::Certificate {
                    operation: "decode Kubernetes CA",
                });
            }
            certificates
        } else {
            Vec::new()
        };
        let builder = reqwest::Client::builder()
            .tls_certs_only(certificates)
            .timeout(options.timeout.unwrap_or(DEFAULT_TIMEOUT))
            .redirect(reqwest::redirect::Policy::none());
        let http = builder.build().map_err(|source| Error::Transport {
            operation: "build Kubernetes client",
            source,
        })?;
        Ok(Self {
            http,
            base_url,
            token,
            token_file,
        })
    }

    /// Read one workload object (`Deployment`, `StatefulSet` or `DaemonSet`).
    pub async fn get_workload(
        &self,
        namespace: &str,
        kind: &str,
        name: &str,
    ) -> Result<Workload, Error> {
        let resource = resource_for(kind)?;
        let path = format!(
            "/apis/apps/v1/namespaces/{}/{resource}/{}",
            escape_segment(namespace),
            escape_segment(name)
        );
        let object: ObjectJson = self.get_json("kubernetes workload", &path).await?;
        let mut workload = Workload {
            namespace: namespace.to_owned(),
            kind: kind.to_owned(),
            name: name.to_owned(),
            generation: object.metadata.generation,
            observed_generation: object.status.observed_generation,
            desired: 0,
            updated: 0,
            ready: 0,
            available: 0,
            annotations: object.metadata.annotations.unwrap_or_default(),
            containers: Vec::new(),
            selector: BTreeMap::new(),
        };
        if kind == "DaemonSet" {
            workload.desired = object.status.desired_number_scheduled;
            workload.ready = object.status.number_ready;
            workload.available = object.status.number_available;
            workload.updated = object.status.updated_number_scheduled;
        } else {
            workload.desired = i64::from(object.spec.replicas.unwrap_or_default());
            workload.ready = object.status.ready_replicas;
            workload.available = object.status.available_replicas;
            workload.updated = object.status.updated_replicas;
        }
        if let Some(selector) = object.spec.selector {
            workload.selector = selector.match_labels;
        }
        if let Some(template) = object.spec.template {
            workload.containers = template
                .spec
                .containers
                .into_iter()
                .map(|container| container.name)
                .collect();
        }
        Ok(workload)
    }

    /// List pods in a namespace, optionally filtered by labels.
    pub async fn list_pods(
        &self,
        namespace: &str,
        labels: &BTreeMap<String, String>,
    ) -> Result<Vec<Pod>, Error> {
        let mut path = format!("/api/v1/namespaces/{}/pods", escape_segment(namespace));
        if !labels.is_empty() {
            path.push_str("?labelSelector=");
            path.push_str(&query_escape(&label_selector(labels)));
        }
        let list: PodListJson = self.get_json("kubernetes pods", &path).await?;
        let mut pods = Vec::with_capacity(list.items.len());
        for item in list.items {
            let mut pod = Pod {
                namespace: namespace.to_owned(),
                name: item.metadata.name,
                phase: item.status.phase,
                node: item.spec.node_name,
                ready: true,
                restarts: 0,
                containers: Vec::new(),
                start_time: parse_time("kubernetes pods", item.status.start_time.as_deref())?,
            };
            if item.spec.containers.is_empty() {
                pod.ready = false;
            }
            let mut statuses = BTreeMap::new();
            for status in item.status.container_statuses {
                let last_terminated_at = status
                    .last_state
                    .as_ref()
                    .and_then(|state| state.terminated.as_ref())
                    .and_then(|terminated| terminated.finished_at.as_deref())
                    .or_else(|| {
                        status
                            .state
                            .as_ref()
                            .and_then(|state| state.terminated.as_ref())
                            .and_then(|terminated| terminated.finished_at.as_deref())
                    });
                let waiting_reason = first_non_empty([
                    status
                        .state
                        .as_ref()
                        .and_then(|state| state.waiting.as_ref())
                        .map(|waiting| waiting.reason.clone()),
                    status
                        .state
                        .as_ref()
                        .and_then(|state| state.terminated.as_ref())
                        .map(|terminated| terminated.reason.clone()),
                ]);
                let container = ContainerStatus {
                    name: status.name.clone(),
                    image: String::new(),
                    ready: status.ready,
                    restart_count: status.restart_count,
                    waiting_reason,
                    last_terminated_at: parse_time("kubernetes pods", last_terminated_at)?,
                };
                statuses.insert(status.name, container);
            }
            // Keep the pod template order instead of the API's alphabetical
            // containerStatuses order: reports pick the workload's own
            // container, not a sidecar that happens to sort first.
            for container in item.spec.containers {
                let mut status =
                    statuses
                        .get(&container.name)
                        .cloned()
                        .unwrap_or_else(|| ContainerStatus {
                            name: container.name.clone(),
                            image: String::new(),
                            ready: false,
                            restart_count: 0,
                            waiting_reason: String::new(),
                            last_terminated_at: None,
                        });
                status.image = container.image;
                if !status.ready {
                    pod.ready = false;
                }
                pod.restarts += status.restart_count;
                pod.containers.push(status);
            }
            pods.push(pod);
        }
        pods.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(pods)
    }

    /// Recent events for one object, newest first, capped at `limit`.
    pub async fn list_events(
        &self,
        namespace: &str,
        object_name: &str,
        limit: usize,
    ) -> Result<Vec<Event>, Error> {
        let limit = if limit == 0 { 10 } else { limit };
        let path = format!(
            "/api/v1/namespaces/{}/events?fieldSelector={}",
            escape_segment(namespace),
            query_escape(&format!("involvedObject.name={object_name}"))
        );
        let list: EventListJson = self.get_json("kubernetes events", &path).await?;
        let mut events = Vec::with_capacity(list.items.len());
        for item in list.items {
            let seen = item
                .last_timestamp
                .as_deref()
                .or(item.event_time.as_deref())
                .or(item.metadata.creation_timestamp.as_deref());
            events.push(Event {
                event_type: item.event_type,
                reason: item.reason,
                message: item.message,
                count: item.count,
                last_seen: parse_time("kubernetes events", seen)?,
            });
        }
        events.sort_by_key(|event| std::cmp::Reverse(event.last_seen));
        events.truncate(limit);
        Ok(events)
    }

    /// List every Deployment, StatefulSet and DaemonSet in the cluster.
    ///
    /// Used by the report coverage check: a workload that exists in the
    /// cluster but not in the catalog must become visible instead of silently
    /// disappearing from the report. Requires the cluster-wide read
    /// ClusterRole; the result is sorted by namespace, kind and name.
    pub async fn list_workloads(&self) -> Result<Vec<WorkloadRef>, Error> {
        let mut workloads = Vec::new();
        for (kind, resource) in [
            ("Deployment", "deployments"),
            ("StatefulSet", "statefulsets"),
            ("DaemonSet", "daemonsets"),
        ] {
            let path = format!("/apis/apps/v1/{resource}");
            let list: NamedListJson = self.get_json("kubernetes workloads", &path).await?;
            for item in list.items {
                workloads.push(WorkloadRef {
                    namespace: item.metadata.namespace,
                    kind: kind.to_owned(),
                    name: item.metadata.name,
                });
            }
        }
        workloads.sort_by(|left, right| {
            (&left.namespace, &left.kind, &left.name).cmp(&(
                &right.namespace,
                &right.kind,
                &right.name,
            ))
        });
        Ok(workloads)
    }

    /// List Flux Kustomizations and HelmReleases in every namespace.
    ///
    /// The report treats a missing `Ready=True` condition as a problem; a
    /// `suspend: true` resource is reported separately and is never an error,
    /// because a deliberately paused Kustomization is a normal state.
    pub async fn list_flux(&self) -> Result<Vec<FluxResource>, Error> {
        let mut resources = Vec::new();
        for (kind, path) in [
            (
                "Kustomization",
                "/apis/kustomize.toolkit.fluxcd.io/v1/kustomizations",
            ),
            (
                "HelmRelease",
                "/apis/helm.toolkit.fluxcd.io/v2/helmreleases",
            ),
        ] {
            let list: FluxListJson = self.get_json("flux resources", path).await?;
            for item in list.items {
                let mut message = String::new();
                let mut ready = false;
                let mut saw_ready = false;
                for condition in &item.status.conditions {
                    if condition.condition_type != "Ready" {
                        continue;
                    }
                    saw_ready = true;
                    ready = condition.status == "True";
                    if condition.reason.is_empty() {
                        message.clone_from(&condition.message);
                    } else {
                        message = format!("{}: {}", condition.reason, condition.message);
                    }
                }
                if !saw_ready && message.is_empty() {
                    "нет условия Ready".clone_into(&mut message);
                }
                resources.push(FluxResource {
                    kind: kind.to_owned(),
                    namespace: item.metadata.namespace,
                    name: item.metadata.name,
                    ready: ready && saw_ready,
                    suspended: item.spec.suspend,
                    message: message.trim().to_owned(),
                });
            }
        }
        resources.sort_by(|left, right| {
            (&left.namespace, &left.kind, &left.name).cmp(&(
                &right.namespace,
                &right.kind,
                &right.name,
            ))
        });
        Ok(resources)
    }

    /// Read the tail of one container's log.
    pub async fn pod_logs(
        &self,
        namespace: &str,
        pod: &str,
        container: &str,
        tail_lines: usize,
        since: Duration,
    ) -> Result<String, Error> {
        let tail_lines = if tail_lines == 0 { 200 } else { tail_lines };
        let mut query = format!("tailLines={tail_lines}");
        if !container.is_empty() {
            query.push_str("&container=");
            query.push_str(&query_escape(container));
        }
        if !since.is_zero() {
            let _ = std::fmt::Write::write_fmt(
                &mut query,
                format_args!("&sinceSeconds={}", since.as_secs()),
            );
        }
        let path = format!(
            "/api/v1/namespaces/{}/pods/{}/log?{query}",
            escape_segment(namespace),
            escape_segment(pod)
        );
        let body = self
            .send(reqwest::Method::GET, "kubernetes logs", &path, None, None)
            .await?;
        Ok(String::from_utf8_lossy(&body).into_owned())
    }

    /// The only mutation the gateway supports: a strategic merge patch of the
    /// pod template's `restartedAt` annotation plus a cooldown marker on the
    /// object itself.
    pub async fn patch_restart(
        &self,
        namespace: &str,
        kind: &str,
        name: &str,
        at: OffsetDateTime,
    ) -> Result<(), Error> {
        let resource = resource_for(kind)?;
        let stamp = at
            .to_offset(UtcOffset::UTC)
            .format(&Rfc3339)
            .unwrap_or_default();
        let patch = json!({
            "metadata": {
                "annotations": {"mcp-gateway/last-restart": stamp.clone()}
            },
            "spec": {
                "template": {
                    "metadata": {
                        "annotations": {"kubectl.kubernetes.io/restartedAt": stamp}
                    }
                }
            }
        });
        let path = format!(
            "/apis/apps/v1/namespaces/{}/{resource}/{}",
            escape_segment(namespace),
            escape_segment(name)
        );
        self.send(
            reqwest::Method::PATCH,
            "kubernetes patch",
            &path,
            Some(&patch),
            Some("application/strategic-merge-patch+json"),
        )
        .await?;
        Ok(())
    }

    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        operation: &'static str,
        path: &str,
    ) -> Result<T, Error> {
        let body = self
            .send(reqwest::Method::GET, operation, path, None, None)
            .await?;
        let body = &body[..body.len().min(MAX_BODY_BYTES)];
        serde_json::from_slice(body).map_err(|source| Error::Decode { operation, source })
    }

    async fn send(
        &self,
        method: reqwest::Method,
        operation: &'static str,
        path: &str,
        body: Option<&Value>,
        content_type: Option<&str>,
    ) -> Result<Vec<u8>, Error> {
        let token = self.auth_token()?;
        let mut request = self
            .http
            .request(method.clone(), format!("{}{path}", self.base_url))
            .header(reqwest::header::ACCEPT, "application/json");
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            let encoded = serde_json::to_vec(body).map_err(|error| Error::Invalid {
                detail: error.to_string(),
            })?;
            request = request.body(encoded);
        }
        if let Some(content_type) = content_type {
            request = request.header(reqwest::header::CONTENT_TYPE, content_type);
        }
        let response = request
            .send()
            .await
            .map_err(|source| Error::Transport { operation, source })?;
        let status = response.status().as_u16();
        let body = response
            .bytes()
            .await
            .map_err(|source| Error::Transport { operation, source })?;
        if status != 200 {
            return Err(Error::Status {
                operation: format!("{operation} {method} {path}"),
                status,
            });
        }
        Ok(body[..body.len().min(MAX_BODY_BYTES)].to_vec())
    }

    fn auth_token(&self) -> Result<Option<String>, Error> {
        if let Some(token) = &self.token {
            return Ok(Some(token.clone()));
        }
        let Some(path) = &self.token_file else {
            return Ok(None);
        };
        let data = std::fs::read_to_string(path).map_err(|error| Error::Io {
            operation: "read Kubernetes token",
            detail: error.to_string(),
        })?;
        let token = data.trim();
        if token.is_empty() {
            return Ok(None);
        }
        Ok(Some(token.to_owned()))
    }
}

/// A normalized Deployment/StatefulSet/DaemonSet view.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Workload {
    pub namespace: String,
    pub kind: String,
    pub name: String,
    pub generation: i64,
    pub observed_generation: i64,
    pub desired: i64,
    pub updated: i64,
    pub ready: i64,
    pub available: i64,
    pub annotations: BTreeMap<String, String>,
    pub containers: Vec<String>,
    pub selector: BTreeMap<String, String>,
}

impl Workload {
    /// Whether the workload has all desired replicas available.
    #[must_use]
    pub fn available_ok(&self) -> bool {
        if self.desired == 0 {
            return true;
        }
        self.available >= self.desired && self.ready >= self.desired
    }

    /// Whether the controller has not observed the current generation or not
    /// all replicas are updated.
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

/// One container's state in a pod.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ContainerStatus {
    pub name: String,
    pub image: String,
    pub ready: bool,
    pub restart_count: i32,
    pub waiting_reason: String,
    pub last_terminated_at: Option<OffsetDateTime>,
}

/// A normalized pod view.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Pod {
    pub namespace: String,
    pub name: String,
    pub phase: String,
    pub node: String,
    pub ready: bool,
    pub restarts: i32,
    pub containers: Vec<ContainerStatus>,
    pub start_time: Option<OffsetDateTime>,
}

/// One workload that exists in the cluster, used by the report coverage check.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkloadRef {
    pub namespace: String,
    pub kind: String,
    pub name: String,
}

/// One Flux Kustomization or HelmRelease as seen by the report.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FluxResource {
    pub kind: String,
    pub namespace: String,
    pub name: String,
    pub ready: bool,
    pub suspended: bool,
    pub message: String,
}

/// A normalized Kubernetes event.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Event {
    pub event_type: String,
    pub reason: String,
    pub message: String,
    pub count: i32,
    pub last_seen: Option<OffsetDateTime>,
}

/// Read the gateway cooldown annotation from a workload.
#[must_use]
pub fn last_restart(workload: &Workload) -> Option<OffsetDateTime> {
    let raw = workload.annotations.get("mcp-gateway/last-restart")?.trim();
    if raw.is_empty() {
        return None;
    }
    OffsetDateTime::parse(raw, &Rfc3339).ok()
}

fn resource_for(kind: &str) -> Result<&'static str, Error> {
    match kind {
        "Deployment" => Ok("deployments"),
        "StatefulSet" => Ok("statefulsets"),
        "DaemonSet" => Ok("daemonsets"),
        other => Err(Error::Invalid {
            detail: format!("unsupported workload kind {other:?}"),
        }),
    }
}

fn label_selector(labels: &BTreeMap<String, String>) -> String {
    labels
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn first_non_empty<const N: usize>(values: [Option<String>; N]) -> String {
    values
        .into_iter()
        .flatten()
        .find(|value| !value.is_empty())
        .unwrap_or_default()
}

fn parse_time(
    operation: &'static str,
    value: Option<&str>,
) -> Result<Option<OffsetDateTime>, Error> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    OffsetDateTime::parse(value, &Rfc3339)
        .map(Some)
        .map_err(|error| Error::Invalid {
            detail: format!("{operation}: invalid timestamp: {error}"),
        })
}

/// Percent-encode one URL path segment. Kubernetes names and namespaces are
/// DNS labels, so only the unreserved characters stay literal.
fn escape_segment(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                escaped.push(char::from(byte));
            }
            other => {
                let _ = std::fmt::Write::write_fmt(&mut escaped, format_args!("%{other:02X}"));
            }
        }
    }
    escaped
}

/// Percent-encode a query value the way Go's `url.QueryEscape` does for the
/// characters a label selector and a field selector use.
fn query_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                escaped.push(char::from(byte));
            }
            b' ' => escaped.push('+'),
            other => {
                let _ = std::fmt::Write::write_fmt(&mut escaped, format_args!("%{other:02X}"));
            }
        }
    }
    escaped
}

// Wire structures: only the fields the gateway reads.

#[derive(Debug, Default, Deserialize)]
struct ObjectJson {
    #[serde(default)]
    metadata: ObjectMetadata,
    #[serde(default)]
    spec: ObjectSpec,
    #[serde(default)]
    status: ObjectStatus,
}

#[derive(Debug, Default, Deserialize)]
struct ObjectMetadata {
    #[serde(default)]
    generation: i64,
    #[serde(default)]
    annotations: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Default, Deserialize)]
struct ObjectSpec {
    #[serde(default)]
    replicas: Option<i32>,
    #[serde(default)]
    selector: Option<ObjectSelector>,
    #[serde(default)]
    template: Option<ObjectTemplate>,
}

#[derive(Debug, Default, Deserialize)]
struct ObjectSelector {
    #[serde(default, rename = "matchLabels")]
    match_labels: BTreeMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
struct ObjectTemplate {
    #[serde(default)]
    spec: TemplateSpec,
}

#[derive(Debug, Default, Deserialize)]
struct TemplateSpec {
    #[serde(default)]
    containers: Vec<ContainerJson>,
}

#[derive(Debug, Deserialize)]
struct ContainerJson {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ObjectStatus {
    #[serde(default)]
    ready_replicas: i64,
    #[serde(default)]
    available_replicas: i64,
    #[serde(default)]
    updated_replicas: i64,
    #[serde(default)]
    observed_generation: i64,
    #[serde(default)]
    desired_number_scheduled: i64,
    #[serde(default)]
    number_ready: i64,
    #[serde(default)]
    number_available: i64,
    #[serde(default)]
    updated_number_scheduled: i64,
}

#[derive(Debug, Default, Deserialize)]
struct PodListJson {
    #[serde(default)]
    items: Vec<PodJson>,
}

#[derive(Debug, Default, Deserialize)]
struct PodJson {
    #[serde(default)]
    metadata: PodMetadata,
    #[serde(default)]
    spec: PodSpec,
    #[serde(default)]
    status: PodStatus,
}

#[derive(Debug, Default, Deserialize)]
struct PodMetadata {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Default, Deserialize)]
struct PodSpec {
    #[serde(default, rename = "nodeName")]
    node_name: String,
    #[serde(default)]
    containers: Vec<PodContainerJson>,
}

#[derive(Debug, Deserialize)]
struct PodContainerJson {
    #[serde(default)]
    name: String,
    #[serde(default)]
    image: String,
}

#[derive(Debug, Default, Deserialize)]
struct PodStatus {
    #[serde(default)]
    phase: String,
    #[serde(default, rename = "startTime")]
    start_time: Option<String>,
    #[serde(default, rename = "containerStatuses")]
    container_statuses: Vec<ContainerStatusJson>,
}

#[derive(Debug, Default, Deserialize)]
struct ContainerStatusJson {
    #[serde(default)]
    name: String,
    #[serde(default)]
    ready: bool,
    #[serde(default, rename = "restartCount")]
    restart_count: i32,
    #[serde(default)]
    state: Option<ContainerStateJson>,
    #[serde(default, rename = "lastState")]
    last_state: Option<ContainerStateJson>,
}

#[derive(Debug, Default, Deserialize)]
struct ContainerStateJson {
    #[serde(default)]
    waiting: Option<WaitingJson>,
    #[serde(default)]
    terminated: Option<TerminatedJson>,
}

#[derive(Debug, Default, Deserialize)]
struct WaitingJson {
    #[serde(default)]
    reason: String,
}

#[derive(Debug, Default, Deserialize)]
struct TerminatedJson {
    #[serde(default)]
    reason: String,
    #[serde(default, rename = "finishedAt")]
    finished_at: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct EventListJson {
    #[serde(default)]
    items: Vec<EventJson>,
}

#[derive(Debug, Default, Deserialize)]
struct EventJson {
    #[serde(default)]
    metadata: EventMetadata,
    #[serde(default, rename = "type")]
    event_type: String,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    count: i32,
    #[serde(default, rename = "lastTimestamp")]
    last_timestamp: Option<String>,
    #[serde(default, rename = "eventTime")]
    event_time: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct EventMetadata {
    #[serde(default, rename = "creationTimestamp")]
    creation_timestamp: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct NamedListJson {
    #[serde(default)]
    items: Vec<NamedItemJson>,
}

#[derive(Debug, Default, Deserialize)]
struct NamedItemJson {
    #[serde(default)]
    metadata: NamespacedMetadata,
}

#[derive(Debug, Default, Deserialize)]
struct NamespacedMetadata {
    #[serde(default)]
    namespace: String,
    #[serde(default)]
    name: String,
}

#[derive(Debug, Default, Deserialize)]
struct FluxListJson {
    #[serde(default)]
    items: Vec<FluxItemJson>,
}

#[derive(Debug, Default, Deserialize)]
struct FluxItemJson {
    #[serde(default)]
    metadata: NamespacedMetadata,
    #[serde(default)]
    spec: FluxSpecJson,
    #[serde(default)]
    status: FluxStatusJson,
}

#[derive(Debug, Default, Deserialize)]
struct FluxSpecJson {
    #[serde(default)]
    suspend: bool,
}

#[derive(Debug, Default, Deserialize)]
struct FluxStatusJson {
    #[serde(default)]
    conditions: Vec<FluxConditionJson>,
}

#[derive(Debug, Default, Deserialize)]
struct FluxConditionJson {
    #[serde(default, rename = "type")]
    condition_type: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    message: String,
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use axum::Router;
    use axum::body::Bytes;
    use axum::extract::{Query, State};
    use axum::http::{HeaderMap, StatusCode, Uri};
    use axum::routing::{get, patch};

    use super::*;

    #[derive(Debug, Default)]
    struct Recorded {
        authorization: String,
        content_type: String,
        query: HashMap<String, String>,
        body: Bytes,
    }

    type Records = Arc<Mutex<Vec<Recorded>>>;

    async fn serve(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        format!("http://{addr}")
    }

    fn client(base: &str) -> Client {
        Client::new(Options {
            base_url: Some(base.to_owned()),
            token: Some("test-token".to_owned()),
            ..Options::default()
        })
        .expect("client")
    }

    fn record(headers: &HeaderMap, query: HashMap<String, String>, body: Bytes) -> Recorded {
        Recorded {
            authorization: headers
                .get(reqwest::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .to_owned(),
            content_type: headers
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .to_owned(),
            query,
            body,
        }
    }

    #[tokio::test]
    async fn get_workload_deployment() {
        let records: Records = Arc::default();
        let app = Router::new()
            .route(
                "/apis/apps/v1/namespaces/home/deployments/scheduler",
                get(
                    |State(records): State<Records>,
                     headers: HeaderMap,
                     Query(query): Query<HashMap<String, String>>,
                     uri: Uri,
                     body: Bytes| async move {
                        records
                            .lock()
                            .expect("lock")
                            .push(record(&headers, query, body));
                        assert_eq!(uri.path(), "/apis/apps/v1/namespaces/home/deployments/scheduler");
                        axum::Json(serde_json::json!({
                            "metadata": {
                                "name": "scheduler",
                                "namespace": "home",
                                "generation": 3,
                                "annotations": {"mcp-gateway/last-restart": "2026-11-01T10:00:00Z"}
                            },
                            "spec": {
                                "replicas": 1,
                                "selector": {"matchLabels": {"app.kubernetes.io/name": "scheduler"}},
                                "template": {"spec": {"containers": [{"name": "scheduler"}]}}
                            },
                            "status": {
                                "replicas": 1,
                                "readyReplicas": 1,
                                "availableReplicas": 1,
                                "updatedReplicas": 1,
                                "observedGeneration": 3
                            }
                        }))
                    },
                ),
            )
            .with_state(records.clone());
        let base = serve(app).await;
        let workload = client(&base)
            .get_workload("home", "Deployment", "scheduler")
            .await
            .expect("workload");
        assert_eq!(workload.desired, 1);
        assert_eq!(workload.ready, 1);
        assert_eq!(workload.available, 1);
        assert!(workload.available_ok());
        assert!(!workload.rollout_in_progress());
        assert_eq!(workload.containers, ["scheduler"]);
        assert_eq!(workload.selector["app.kubernetes.io/name"], "scheduler");
        assert_eq!(
            last_restart(&workload),
            Some(OffsetDateTime::parse("2026-11-01T10:00:00Z", &Rfc3339).expect("time"))
        );
        assert_eq!(
            records.lock().expect("lock")[0].authorization,
            "Bearer test-token"
        );
    }

    #[tokio::test]
    async fn get_workload_daemonset_and_rollout() {
        let app = Router::new().route(
            "/apis/apps/v1/namespaces/monitoring/daemonsets/alloy",
            get(|| async {
                axum::Json(serde_json::json!({
                    "metadata": {"name": "alloy", "namespace": "monitoring", "generation": 5},
                    "spec": {"selector": {"matchLabels": {"app.kubernetes.io/name": "alloy"}}},
                    "status": {
                        "desiredNumberScheduled": 3,
                        "numberReady": 2,
                        "numberAvailable": 2,
                        "updatedNumberScheduled": 1,
                        "observedGeneration": 4
                    }
                }))
            }),
        );
        let base = serve(app).await;
        let workload = client(&base)
            .get_workload("monitoring", "DaemonSet", "alloy")
            .await
            .expect("workload");
        assert_eq!(workload.desired, 3);
        assert_eq!(workload.ready, 2);
        assert_eq!(workload.updated, 1);
        assert!(workload.rollout_in_progress());
        assert!(!workload.available_ok());
    }

    #[tokio::test]
    async fn an_unsupported_kind_is_rejected_without_a_request() {
        let client = client("http://127.0.0.1:1");
        let error = client
            .get_workload("home", "Pod", "x")
            .await
            .expect_err("unsupported kind");
        assert_eq!(error.kind(), ErrorKind::Permanent);
    }

    #[tokio::test]
    async fn list_pods_keeps_template_order() {
        let records: Records = Arc::default();
        let app = Router::new()
            .route(
                "/api/v1/namespaces/home/pods",
                get(
                    |State(records): State<Records>,
                     headers: HeaderMap,
                     Query(query): Query<HashMap<String, String>>| async move {
                        records
                            .lock()
                            .expect("lock")
                            .push(record(&headers, query, Bytes::new()));
                        axum::Json(serde_json::json!({"items": [
                            {
                                "metadata": {"name": "hermes-2"},
                                "spec": {
                                    "nodeName": "node1",
                                    "containers": [
                                        {"name": "hermes", "image": "ghcr.io/example/hermes@sha256:aaaa"},
                                        {"name": "voice-proxy", "image": "ghcr.io/example/proxy@sha256:bbbb"}
                                    ]
                                },
                                "status": {
                                    "phase": "Running",
                                    "startTime": "2026-11-01T09:00:00Z",
                                    "containerStatuses": [
                                        {
                                            "name": "hermes",
                                            "ready": true,
                                            "restartCount": 2,
                                            "state": {"waiting": {"reason": ""}},
                                            "lastState": {"terminated": {"finishedAt": "2026-11-01T10:55:00Z"}}
                                        },
                                        {
                                            "name": "voice-proxy",
                                            "ready": false,
                                            "restartCount": 7,
                                            "state": {"waiting": {"reason": "CrashLoopBackOff"}}
                                        }
                                    ]
                                }
                            },
                            {"metadata": {"name": "hermes-1"}, "status": {"phase": "Pending"}}
                        ]}))
                    },
                ),
            )
            .with_state(records.clone());
        let base = serve(app).await;
        let labels = BTreeMap::from([("app".to_owned(), "hermes".to_owned())]);
        let pods = client(&base)
            .list_pods("home", &labels)
            .await
            .expect("pods");
        assert_eq!(pods.len(), 2);
        assert_eq!(pods[0].name, "hermes-1");
        assert_eq!(pods[1].name, "hermes-2");
        assert!(!pods[0].ready);
        assert!(!pods[1].ready);
        assert_eq!(pods[1].restarts, 9);
        assert_eq!(pods[1].containers[1].waiting_reason, "CrashLoopBackOff");
        assert!(pods[1].containers[0].last_terminated_at.is_some());
        assert_eq!(
            pods[1].containers[0].image,
            "ghcr.io/example/hermes@sha256:aaaa"
        );
        assert_eq!(
            records.lock().expect("lock")[0]
                .query
                .get("labelSelector")
                .map(String::as_str),
            Some("app=hermes")
        );
    }

    #[tokio::test]
    async fn list_events_is_newest_first_and_capped() {
        let records: Records = Arc::default();
        let app = Router::new()
            .route(
                "/api/v1/namespaces/home/events",
                get(
                    |State(records): State<Records>,
                     Query(query): Query<HashMap<String, String>>| async move {
                        records
                            .lock()
                            .expect("lock")
                            .push(record(&HeaderMap::new(), query, Bytes::new()));
                        axum::Json(serde_json::json!({"items": [
                            {"type": "Warning", "reason": "BackOff", "message": "back-off restarting", "count": 4, "lastTimestamp": "2026-11-01T09:00:00Z"},
                            {"type": "Normal", "reason": "Pulled", "message": "image pulled", "count": 1, "lastTimestamp": "2026-11-01T10:00:00Z"},
                            {"type": "Normal", "reason": "Old", "message": "very old", "count": 1, "lastTimestamp": "2026-10-01T10:00:00Z"}
                        ]}))
                    },
                ),
            )
            .with_state(records.clone());
        let base = serve(app).await;
        let events = client(&base)
            .list_events("home", "scheduler", 2)
            .await
            .expect("events");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].reason, "Pulled");
        assert_eq!(events[1].reason, "BackOff");
        assert_eq!(
            records.lock().expect("lock")[0]
                .query
                .get("fieldSelector")
                .map(String::as_str),
            Some("involvedObject.name=scheduler")
        );
    }

    #[tokio::test]
    async fn pod_logs_uses_tail_and_since() {
        let records: Records = Arc::default();
        let app = Router::new()
            .route(
                "/api/v1/namespaces/home/pods/scheduler-1/log",
                get(
                    |State(records): State<Records>,
                     Query(query): Query<HashMap<String, String>>| async move {
                        records.lock().expect("lock").push(record(
                            &HeaderMap::new(),
                            query,
                            Bytes::new(),
                        ));
                        "line1\nline2\n"
                    },
                ),
            )
            .with_state(records.clone());
        let base = serve(app).await;
        let body = client(&base)
            .pod_logs(
                "home",
                "scheduler-1",
                "scheduler",
                50,
                Duration::from_secs(900),
            )
            .await
            .expect("logs");
        assert_eq!(body, "line1\nline2\n");
        let query = &records.lock().expect("lock")[0].query;
        assert_eq!(
            query.get("container").map(String::as_str),
            Some("scheduler")
        );
        assert_eq!(query.get("tailLines").map(String::as_str), Some("50"));
        assert_eq!(query.get("sinceSeconds").map(String::as_str), Some("900"));
    }

    #[tokio::test]
    async fn patch_restart_sends_the_two_annotations() {
        let records: Records = Arc::default();
        let app = Router::new()
            .route(
                "/apis/apps/v1/namespaces/home/deployments/scheduler",
                patch(
                    |State(records): State<Records>,
                     headers: HeaderMap,
                     Query(query): Query<HashMap<String, String>>,
                     body: Bytes| async move {
                        records
                            .lock()
                            .expect("lock")
                            .push(record(&headers, query, body));
                        axum::Json(serde_json::json!({"metadata": {"name": "scheduler"}}))
                    },
                ),
            )
            .with_state(records.clone());
        let base = serve(app).await;
        let at = OffsetDateTime::parse("2026-11-01T12:00:00Z", &Rfc3339).expect("time");
        client(&base)
            .patch_restart("home", "Deployment", "scheduler", at)
            .await
            .expect("patch");
        let records = records.lock().expect("lock");
        assert_eq!(
            records[0].content_type,
            "application/strategic-merge-patch+json"
        );
        let body: Value = serde_json::from_slice(&records[0].body).expect("json");
        assert_eq!(
            body["spec"]["template"]["metadata"]["annotations"]["kubectl.kubernetes.io/restartedAt"],
            "2026-11-01T12:00:00Z"
        );
        assert_eq!(
            body["metadata"]["annotations"]["mcp-gateway/last-restart"],
            "2026-11-01T12:00:00Z"
        );
        drop(records);
    }

    #[tokio::test]
    async fn patch_errors_are_classified() {
        let app = Router::new().route(
            "/apis/apps/v1/namespaces/home/deployments/scheduler",
            patch(|| async { StatusCode::FORBIDDEN }),
        );
        let base = serve(app).await;
        let error = client(&base)
            .patch_restart("home", "Deployment", "scheduler", OffsetDateTime::now_utc())
            .await
            .expect_err("HTTP 403");
        assert_eq!(error.kind(), ErrorKind::Permanent);
    }

    #[test]
    fn last_restart_rejects_a_bad_annotation() {
        let mut workload = Workload {
            namespace: String::new(),
            kind: String::new(),
            name: String::new(),
            generation: 0,
            observed_generation: 0,
            desired: 0,
            updated: 0,
            ready: 0,
            available: 0,
            annotations: BTreeMap::from([(
                "mcp-gateway/last-restart".to_owned(),
                "not-a-time".to_owned(),
            )]),
            containers: Vec::new(),
            selector: BTreeMap::new(),
        };
        assert!(last_restart(&workload).is_none());
        workload.annotations.clear();
        assert!(last_restart(&workload).is_none());
    }

    #[tokio::test]
    async fn list_workloads_reads_all_three_kinds_and_sorts() {
        let app = Router::new()
            .route(
                "/apis/apps/v1/deployments",
                get(|| async {
                    axum::Json(serde_json::json!({"items": [
                        {"metadata": {"namespace": "kube-system", "name": "coredns"}},
                        {"metadata": {"namespace": "home-assistant", "name": "scheduler"}}
                    ]}))
                }),
            )
            .route(
                "/apis/apps/v1/statefulsets",
                get(|| async {
                    axum::Json(serde_json::json!({"items": [
                        {"metadata": {"namespace": "nexus", "name": "nexus"}}
                    ]}))
                }),
            )
            .route(
                "/apis/apps/v1/daemonsets",
                get(|| async {
                    axum::Json(serde_json::json!({"items": [
                        {"metadata": {"namespace": "monitoring", "name": "alloy"}}
                    ]}))
                }),
            );
        let base = serve(app).await;
        let workloads = client(&base).list_workloads().await.expect("workloads");
        let keys: Vec<(String, String, String)> = workloads
            .into_iter()
            .map(|w| (w.namespace, w.kind, w.name))
            .collect();
        assert_eq!(
            keys,
            vec![
                (
                    "home-assistant".to_owned(),
                    "Deployment".to_owned(),
                    "scheduler".to_owned()
                ),
                (
                    "kube-system".to_owned(),
                    "Deployment".to_owned(),
                    "coredns".to_owned()
                ),
                (
                    "monitoring".to_owned(),
                    "DaemonSet".to_owned(),
                    "alloy".to_owned()
                ),
                (
                    "nexus".to_owned(),
                    "StatefulSet".to_owned(),
                    "nexus".to_owned()
                ),
            ]
        );
    }

    #[tokio::test]
    async fn list_flux_maps_ready_and_suspended() {
        let app = Router::new()
            .route(
                "/apis/kustomize.toolkit.fluxcd.io/v1/kustomizations",
                get(|| async {
                    axum::Json(serde_json::json!({"items": [
                        {
                            "metadata": {"namespace": "flux-system", "name": "apps"},
                            "spec": {"suspend": false},
                            "status": {"conditions": [
                                {"type": "Reconciling", "status": "False", "reason": "Progressing", "message": "когда-нибудь"},
                                {"type": "Ready", "status": "False", "reason": "HealthCheckFailed", "message": "Deployment/nexus is not ready"}
                            ]}
                        },
                        {
                            "metadata": {"namespace": "flux-system", "name": "paused"},
                            "spec": {"suspend": true},
                            "status": {"conditions": [{"type": "Ready", "status": "False", "reason": "Suspended", "message": "Reconciliation is suspended"}]}
                        }
                    ]}))
                }),
            )
            .route(
                "/apis/helm.toolkit.fluxcd.io/v2/helmreleases",
                get(|| async {
                    axum::Json(serde_json::json!({"items": [
                        {
                            "metadata": {"namespace": "flux-system", "name": "runtime"},
                            "status": {"conditions": [{"type": "Ready", "status": "True"}]}
                        }
                    ]}))
                }),
            );
        let base = serve(app).await;
        let flux = client(&base).list_flux().await.expect("flux");
        assert_eq!(flux.len(), 3);
        assert_eq!(flux[0].kind, "HelmRelease");
        assert!(flux[0].ready);
        assert!(!flux[1].ready);
        assert_eq!(
            flux[1].message,
            "HealthCheckFailed: Deployment/nexus is not ready"
        );
        assert!(flux[2].suspended);
        assert!(!flux[2].ready);
    }

    #[test]
    fn label_selector_is_sorted() {
        let labels = BTreeMap::from([
            ("b".to_owned(), "2".to_owned()),
            ("a".to_owned(), "1".to_owned()),
        ]);
        assert_eq!(label_selector(&labels), "a=1,b=2");
    }
}
