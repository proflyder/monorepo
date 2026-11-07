//! Promotes Hermes/voice-proxy/health-exporter digests into the `runtime` Helm
//! release.
//!
//! Ported from `platform/deploy/internal/hermes`: the same argv, the same
//! no-op detection, the same smoke checks (health exporter, Assist turn,
//! voice-proxy probe, log scan) and the same rollback/exit contract (0 healthy,
//! 1 failure, 2 rollback verification failed).

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::LazyLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use regex::Regex;
use serde_json::{Value, json};

use crate::gojson;
use crate::process::{CommandRunner, Invocation, Output};
use crate::yaml::{self, Value as YamlValue};

/// Helm release name.
pub const RELEASE: &str = "runtime";
/// Kubernetes namespace.
pub const NAMESPACE: &str = "home-assistant";
/// In-cluster Home Assistant URL used by the smoke Job.
pub const HA_URL: &str = "http://home-assistant-external.monitoring.svc.cluster.local:8123";
/// Secret mounted into the smoke Job.
pub const HA_SECRET: &str = "ha-bridge-ha";
/// Conversation agent used by the smoke turn.
pub const HA_AGENT: &str = "conversation.hermes_agent";
/// Text sent to Assist.
pub const SMOKE_TEXT: &str = "Представься одним коротким предложением. Ничего не изменяй.";
/// Health exporter Deployment.
pub const EXPORTER_DEPLOYMENT: &str = "runtime-health-exporter";
/// Default Helm timeout.
pub const DEFAULT_TIMEOUT: &str = "8m";

/// Health exporter endpoints that must be up.
pub const EXPECTED_ENDPOINTS: &[&str] = &[
    "ha",
    "music_assistant",
    "piper",
    "whisper",
    "gigaam",
    "hermes",
    "voice_proxy",
];

static DIGEST_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[^@]+@sha256:[0-9a-f]{64}$").expect("digest regex"));
static ERROR_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(ERROR|CRITICAL|Traceback)\b|panic:").expect("error regex"));
static HEALTH_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^ricardo_endpoint_up\{endpoint="([^"]+)"\}\s+([0-9.]+)"#).expect("health regex")
});

const TRANSIENT_WORDS: &[&str] = &[
    "URLError",
    "TimeoutError",
    "ConnectionError",
    "ConnectionResetError",
    "RemoteDisconnected",
    "timed out",
    "connection refused",
];

/// One rollout request.
#[derive(Clone)]
pub struct Options {
    /// Private runtime values file.
    pub values_file: String,
    /// Pushed Hermes digest.
    pub hermes_digest: String,
    /// Pushed voice-proxy digest.
    pub voice_proxy_digest: String,
    /// Pushed health-exporter digest.
    pub exporter_digest: String,
    /// Digest-pinned home-deploy image for the smoke Job.
    pub smoke_image: String,
    /// Helm release.
    pub release: String,
    /// Hermes Deployment.
    pub deployment: String,
    /// Health exporter Deployment.
    pub exporter_deployment: String,
    /// Kubernetes namespace.
    pub namespace: String,
    /// Runtime chart directory.
    pub chart: String,
    /// Rollback state directory.
    pub state_dir: String,
    /// Helm timeout.
    pub timeout: String,
    /// Upgrade even when the digests are unchanged.
    pub force: bool,
    /// Fail after the upgrade (rollback test).
    pub simulate_failure: bool,
    /// Roll back the recorded revision.
    pub rollback_only: bool,
    /// kubectl binary.
    pub kubectl: String,
    /// helm binary.
    pub helm: String,
    /// Assist warm-up retry window in seconds.
    pub retry_window_secs: u64,
    /// Delay between Assist retries in seconds.
    pub retry_delay_secs: u64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            values_file: String::new(),
            hermes_digest: String::new(),
            voice_proxy_digest: String::new(),
            exporter_digest: String::new(),
            smoke_image: String::new(),
            release: RELEASE.to_string(),
            deployment: "hermes".to_string(),
            exporter_deployment: EXPORTER_DEPLOYMENT.to_string(),
            namespace: NAMESPACE.to_string(),
            chart: String::new(),
            state_dir: String::new(),
            timeout: DEFAULT_TIMEOUT.to_string(),
            force: false,
            simulate_failure: false,
            rollback_only: false,
            kubectl: "kubectl".to_string(),
            helm: "helm".to_string(),
            retry_window_secs: 90,
            retry_delay_secs: 5,
        }
    }
}

/// Returns a digest-pinned image reference for `name` (hermes/voice-proxy/health-exporter).
pub fn normalize_digest(name: &str, value: &str) -> Result<String, String> {
    let repository = match name {
        "hermes" => "ghcr.io/proflyder/hermes",
        "voice-proxy" => "ghcr.io/proflyder/voice-proxy",
        "health-exporter" => "ghcr.io/proflyder/health-exporter",
        _ => return Err(format!("{name}: unknown image")),
    };
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("{name}: empty digest"));
    }
    if value.starts_with("sha256:") {
        return Ok(format!("{repository}@{value}"));
    }
    if DIGEST_PATTERN.is_match(value) {
        return Ok(value.to_string());
    }
    Err(format!(
        "{name}: expected sha256:<64 hex> or <repo>@sha256:<64 hex>, got {value:?}"
    ))
}

/// Returns the `sha256:…` part of a pinned image reference.
#[must_use]
pub fn image_digest(image: &str) -> &str {
    image.split_once('@').map_or("", |(_, digest)| digest)
}

/// Reads the private values and requires at least two containers under
/// `workloads.hermes` (gateway and voice-proxy).
pub fn load_values(path: &str) -> Result<YamlValue, String> {
    let raw = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let values = if raw.trim().is_empty() {
        YamlValue::Map(Vec::new())
    } else {
        yaml::parse(&raw).map_err(|error| format!("{path}: {error}"))?
    };
    let containers = values
        .get("workloads")
        .and_then(|workloads| workloads.get("hermes"))
        .and_then(|hermes| hermes.get("containers"));
    let Some(YamlValue::Seq(containers)) = containers else {
        return Err(format!(
            "{path}: workloads.hermes.containers must hold gateway and voice-proxy"
        ));
    };
    if containers.len() < 2 {
        return Err(format!(
            "{path}: workloads.hermes.containers must hold gateway and voice-proxy"
        ));
    }
    Ok(values)
}

/// Drops the cluster-scoped storage block.
#[must_use]
pub fn workload_values(mut values: YamlValue) -> YamlValue {
    values.remove("storage");
    values
}

/// Writes the workload values to a mode-600 temporary file.
pub fn write_values(values: &YamlValue) -> Result<TempValues, String> {
    let raw = yaml::to_string(values);
    let path = std::env::temp_dir().join(format!(
        "hermes-values-{}-{}.yaml",
        std::process::id(),
        monotonic_millis()
    ));
    fs::write(&path, raw).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(TempValues(path))
}

/// Temporary values file removed on drop.
pub struct TempValues(std::path::PathBuf);

impl TempValues {
    /// Returns the file path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempValues {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Returns the container name to image map of one Deployment.
pub fn live_images(
    runner: &dyn CommandRunner,
    kubectl: &str,
    namespace: &str,
    deployment: &str,
) -> Result<BTreeMap<String, String>, String> {
    let output = checked(
        runner,
        &Invocation {
            program: kubectl.to_string(),
            args: vec![
                "-n".to_string(),
                namespace.to_string(),
                "get".to_string(),
                "deployment".to_string(),
                deployment.to_string(),
                "-o".to_string(),
                "json".to_string(),
            ],
            stdin: None,
        },
    )?;
    let payload: Value = serde_json::from_str(&output).map_err(|error| error.to_string())?;
    let mut images = BTreeMap::new();
    if let Some(containers) = payload
        .get("spec")
        .and_then(|spec| spec.get("template"))
        .and_then(|template| template.get("spec"))
        .and_then(|spec| spec.get("containers"))
        .and_then(Value::as_array)
    {
        for container in containers {
            let name = container.get("name").and_then(Value::as_str);
            let image = container.get("image").and_then(Value::as_str);
            if let (Some(name), Some(image)) = (name, image) {
                images.insert(name.to_string(), image.to_string());
            }
        }
    }
    Ok(images)
}

/// Reports whether any live image differs from the requested digest.
#[must_use]
pub fn rollout_needed(current: &BTreeMap<String, String>, hermes: &str, voice_proxy: &str) -> bool {
    current.get("gateway").map(String::as_str) != Some(hermes)
        || current.get("voice-proxy").map(String::as_str) != Some(voice_proxy)
}

/// Returns the `--set` arguments that pin the Hermes containers.
#[must_use]
pub fn set_args(hermes: &str, voice_proxy: &str) -> Vec<String> {
    vec![
        "--set".to_string(),
        format!("workloads.hermes.containers[0].image={hermes}"),
        "--set".to_string(),
        format!("workloads.hermes.containers[1].image={voice_proxy}"),
    ]
}

/// Pins the health exporter and drops the old ConfigMap script mount.
#[must_use]
pub fn exporter_set_args(exporter_deployment: &str, image: &str) -> Vec<String> {
    let container = format!("workloads.{exporter_deployment}.containers[0]");
    let workload = format!("workloads.{exporter_deployment}");
    vec![
        "--set".to_string(),
        format!("{container}.image={image}"),
        "--set-json".to_string(),
        format!("{container}.command=[\"/usr/local/bin/health-exporter\"]"),
        "--set-json".to_string(),
        format!("{container}.volumeMounts=[]"),
        "--set-json".to_string(),
        format!("{workload}.volumes=[]"),
    ]
}

/// Returns the newest Helm revision, or 0 when history is unavailable.
#[must_use]
pub fn latest_revision(
    runner: &dyn CommandRunner,
    helm: &str,
    namespace: &str,
    release: &str,
) -> i64 {
    let result = runner.run(&Invocation {
        program: helm.to_string(),
        args: vec![
            "-n".to_string(),
            namespace.to_string(),
            "history".to_string(),
            release.to_string(),
            "-o".to_string(),
            "json".to_string(),
        ],
        stdin: None,
    });
    if result.code != 0 {
        return 0;
    }
    let Ok(revisions) = serde_json::from_str::<Value>(&result.stdout) else {
        return 0;
    };
    let Some(items) = revisions.as_array() else {
        return 0;
    };
    items
        .iter()
        .filter_map(|item| item.get("revision").and_then(Value::as_i64))
        .max()
        .unwrap_or(0)
}

/// Runs the Helm upgrade with the digests pinned by `--set`.
#[must_use]
pub fn upgrade(
    runner: &dyn CommandRunner,
    options: &Options,
    chart: &str,
    values_path: &Path,
    hermes: &str,
    voice_proxy: &str,
) -> Output {
    let mut args = vec![
        "upgrade".to_string(),
        "--install".to_string(),
        options.release.clone(),
        chart.to_string(),
        "-n".to_string(),
        options.namespace.clone(),
        "-f".to_string(),
        values_path.display().to_string(),
    ];
    args.extend(set_args(hermes, voice_proxy));
    if !options.exporter_digest.is_empty() {
        args.extend(exporter_set_args(
            &options.exporter_deployment,
            &options.exporter_digest,
        ));
    }
    args.push("--wait=legacy".to_string());
    args.push(format!("--timeout={}", options.timeout));
    runner.run(&Invocation {
        program: options.helm.clone(),
        args,
        stdin: None,
    })
}

/// Restores one Helm revision.
pub fn rollback(
    runner: &dyn CommandRunner,
    options: &Options,
    revision: i64,
) -> Result<(), String> {
    checked(
        runner,
        &Invocation {
            program: options.helm.clone(),
            args: vec![
                "rollback".to_string(),
                options.release.clone(),
                revision.to_string(),
                "-n".to_string(),
                options.namespace.clone(),
                "--wait=legacy".to_string(),
                format!("--timeout={}", options.timeout),
            ],
            stdin: None,
        },
    )?;
    Ok(())
}

/// Reads the exporter's `/metrics` through the API server's service proxy.
pub fn health_metrics(runner: &dyn CommandRunner, options: &Options) -> Result<String, String> {
    checked(
        runner,
        &Invocation {
            program: options.kubectl.clone(),
            args: vec![
                "get".to_string(),
                "--raw".to_string(),
                exporter_metrics_path(&options.namespace, &options.exporter_deployment),
            ],
            stdin: None,
        },
    )
}

/// Service-proxy path of the exporter's `/metrics`.
#[must_use]
pub fn exporter_metrics_path(namespace: &str, service: &str) -> String {
    format!("/api/v1/namespaces/{namespace}/services/{service}:9116/proxy/metrics")
}

/// Parses `ricardo_endpoint_up{endpoint="…"} N`.
#[must_use]
pub fn parse_health(text: &str) -> BTreeMap<String, i64> {
    let mut values = BTreeMap::new();
    for line in text.split('\n') {
        let Some(captures) = HEALTH_PATTERN.captures(line) else {
            continue;
        };
        let (Some(endpoint), Some(number)) = (captures.get(1), captures.get(2)) else {
            continue;
        };
        if let Ok(value) = number.as_str().parse::<f64>() {
            #[allow(clippy::cast_possible_truncation)]
            values.insert(endpoint.as_str().to_string(), value as i64);
        }
    }
    values
}

/// Requires every expected endpoint to be present and up.
pub fn check_health(text: &str) -> Result<(), String> {
    let values = parse_health(text);
    let missing: Vec<&str> = EXPECTED_ENDPOINTS
        .iter()
        .copied()
        .filter(|name| !values.contains_key(*name))
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "health exporter is missing endpoints: [{}]",
            missing.join(" ")
        ));
    }
    let down: Vec<String> = EXPECTED_ENDPOINTS
        .iter()
        .filter(|name| values.get(**name) != Some(&1))
        .map(|name| format!("{name}:{}", values.get(*name).copied().unwrap_or(0)))
        .collect();
    if !down.is_empty() {
        return Err(format!(
            "health exporter endpoints are down: map[{}]",
            down.join(" ")
        ));
    }
    Ok(())
}

/// Returns the log lines matching the error pattern.
#[must_use]
pub fn voice_proxy_log_errors(text: &str) -> Vec<String> {
    text.split('\n')
        .filter(|line| ERROR_PATTERN.is_match(line))
        .map(str::to_string)
        .collect()
}

/// Fails when the voice-proxy log contains errors.
pub fn check_voice_logs(text: &str) -> Result<(), String> {
    let errors = voice_proxy_log_errors(text);
    if errors.is_empty() {
        return Ok(());
    }
    let sample = &errors[..errors.len().min(10)];
    Err(format!(
        "voice-proxy logs contain errors:\n{}",
        sample.join("\n")
    ))
}

/// The hardened Assist smoke Job manifest.
#[must_use]
pub fn smoke_job(name: &str, namespace: &str, image: &str) -> Value {
    json!({
        "apiVersion": "batch/v1",
        "kind": "Job",
        "metadata": {
            "name": name,
            "namespace": namespace,
            "labels": {"app.kubernetes.io/name": "hermes-smoke"},
        },
        "spec": {
            "backoffLimit": 0,
            "activeDeadlineSeconds": 300,
            "ttlSecondsAfterFinished": 600,
            "template": {
                "metadata": {"labels": {"app.kubernetes.io/name": "hermes-smoke"}},
                "spec": {
                    "restartPolicy": "Never",
                    "automountServiceAccountToken": false,
                    "enableServiceLinks": false,
                    "securityContext": {
                        "runAsNonRoot": true,
                        "runAsUser": 65534,
                        "runAsGroup": 65534,
                        "seccompProfile": {"type": "RuntimeDefault"},
                    },
                    "containers": [{
                        "name": "smoke",
                        "image": image,
                        "imagePullPolicy": "IfNotPresent",
                        "command": ["/usr/local/bin/home-deploy", "hermes", "assist-smoke"],
                        "env": [
                            {"name": "HA_URL", "value": HA_URL},
                            {"name": "HA_AGENT", "value": HA_AGENT},
                            {"name": "SMOKE_TEXT", "value": SMOKE_TEXT},
                        ],
                        "resources": {
                            "requests": {"cpu": "10m", "memory": "32Mi"},
                            "limits": {"memory": "128Mi"},
                        },
                        "securityContext": {
                            "allowPrivilegeEscalation": false,
                            "readOnlyRootFilesystem": true,
                            "capabilities": {"drop": ["ALL"]},
                        },
                        "volumeMounts": [{
                            "name": "ha-token",
                            "mountPath": "/run/secrets/ha",
                            "readOnly": true,
                        }],
                    }],
                    "volumes": [{
                        "name": "ha-token",
                        "secret": {"secretName": HA_SECRET, "defaultMode": 292},
                    }],
                },
            },
        },
    })
}

fn job_condition(runner: &dyn CommandRunner, options: &Options, name: &str) -> String {
    let result = runner.run(&Invocation {
        program: options.kubectl.clone(),
        args: vec![
            "-n".to_string(),
            options.namespace.clone(),
            "get".to_string(),
            "job".to_string(),
            name.to_string(),
            "-o".to_string(),
            r#"jsonpath={range .status.conditions[*]}{.type}={.status}{"\n"}{end}"#.to_string(),
        ],
        stdin: None,
    });
    if result.code != 0 {
        return String::new();
    }
    for line in result.stdout.split('\n') {
        if line == "Complete=True" || line == "Failed=True" {
            return line
                .split_once('=')
                .map_or("", |(kind, _)| kind)
                .to_string();
        }
    }
    String::new()
}

fn wait_for_job(
    runner: &dyn CommandRunner,
    options: &Options,
    name: &str,
) -> Result<String, String> {
    let deadline = Instant::now() + Duration::from_secs(600);
    while Instant::now() < deadline {
        let condition = job_condition(runner, options, name);
        if condition == "Complete" || condition == "Failed" {
            return Ok(condition);
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    Err(format!("smoke job {name} did not finish within 600s"))
}

fn smoke_result(output: &str) -> Option<BTreeMap<String, Value>> {
    let text = output.trim();
    let line = text.split('\n').next_back().unwrap_or("");
    serde_json::from_str::<Value>(line)
        .ok()
        .and_then(|value| match value {
            Value::Object(map) => Some(map.into_iter().collect()),
            _ => None,
        })
}

/// Runs one smoke Job and returns the JSON result it printed.
pub fn assist_attempt(
    runner: &dyn CommandRunner,
    options: &Options,
) -> Result<BTreeMap<String, Value>, String> {
    let name = format!("hermes-smoke-{}", monotonic_millis());
    let manifest = yaml::to_string(&YamlValue::from_json(&smoke_job(
        &name,
        &options.namespace,
        &options.smoke_image,
    )));
    checked(
        runner,
        &Invocation {
            program: options.kubectl.clone(),
            args: vec![
                "-n".to_string(),
                options.namespace.clone(),
                "apply".to_string(),
                "-f".to_string(),
                "-".to_string(),
            ],
            stdin: Some(manifest),
        },
    )?;
    let outcome = (|| {
        let condition = wait_for_job(runner, options, &name)?;
        let logs = runner.run(&Invocation {
            program: options.kubectl.clone(),
            args: vec![
                "-n".to_string(),
                options.namespace.clone(),
                "logs".to_string(),
                format!("job/{name}"),
                "--tail=20".to_string(),
            ],
            stdin: None,
        });
        let output = logs.stdout.trim().to_string();
        if let Some(result) = smoke_result(&output) {
            return Ok(result);
        }
        if condition == "Complete" {
            return Err(format!(
                "Assist smoke produced unexpected output: {output:?}"
            ));
        }
        let detail = runner
            .run(&Invocation {
                program: options.kubectl.clone(),
                args: vec![
                    "-n".to_string(),
                    options.namespace.clone(),
                    "describe".to_string(),
                    "job".to_string(),
                    name.clone(),
                    "--tail=40".to_string(),
                ],
                stdin: None,
            })
            .stdout;
        let stderr = if logs.stderr.trim().is_empty() {
            output
        } else {
            logs.stderr.trim().to_string()
        };
        Err(format!(
            "Assist smoke job failed: {stderr}\n{}",
            detail.trim()
        ))
    })();
    let _ = runner.run(&Invocation {
        program: options.kubectl.clone(),
        args: vec![
            "-n".to_string(),
            options.namespace.clone(),
            "delete".to_string(),
            "job".to_string(),
            name,
            "--ignore-not-found".to_string(),
        ],
        stdin: None,
    });
    outcome
}

/// Reports whether a failed Assist result may succeed once Hermes warms up.
#[must_use]
pub fn transient_smoke_result(result: &BTreeMap<String, Value>) -> bool {
    if result.get("ok").and_then(Value::as_bool) == Some(true) {
        return false;
    }
    if let Some(status) = result.get("status").and_then(Value::as_i64) {
        if (500..600).contains(&status) {
            return true;
        }
        if status != 200 {
            return false;
        }
    }
    let error_text = result.get("error").map(display_value).unwrap_or_default();
    TRANSIENT_WORDS
        .iter()
        .any(|marker| error_text.contains(marker))
}

/// Asks Hermes one Assist turn, retrying a bounded transient window.
pub fn assist_smoke(
    options: &Options,
    attempt: &mut dyn FnMut() -> Result<BTreeMap<String, Value>, String>,
    stdout: &mut dyn Write,
) -> Result<String, String> {
    let deadline = Instant::now() + Duration::from_secs(options.retry_window_secs);
    loop {
        let result = attempt()?;
        if result.get("ok").and_then(Value::as_bool) == Some(true) {
            let speech = result.get("speech").map(display_value).unwrap_or_default();
            let speech = speech.trim();
            if !speech.is_empty() {
                return Ok(speech.to_string());
            }
        }
        if !transient_smoke_result(&result) {
            return Err(format!("Assist smoke failed: {}", display_map(&result)));
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "Assist smoke still failing after {}s: {}",
                options.retry_window_secs,
                display_map(&result)
            ));
        }
        let reason = result
            .get("status")
            .map(display_value)
            .or_else(|| result.get("error").map(display_value))
            .unwrap_or_else(|| "transient failure".to_string());
        let _ = writeln!(
            stdout,
            "Assist smoke is not ready yet ({reason}); retrying in {}s",
            options.retry_delay_secs
        );
        std::thread::sleep(Duration::from_secs(options.retry_delay_secs));
    }
}

/// Runs the distroless proxy's own probe inside the live container.
pub fn voice_proxy_healthcheck(
    runner: &dyn CommandRunner,
    options: &Options,
) -> Result<(), String> {
    checked(
        runner,
        &Invocation {
            program: options.kubectl.clone(),
            args: vec![
                "-n".to_string(),
                options.namespace.clone(),
                "exec".to_string(),
                format!("deployment/{}", options.deployment),
                "-c".to_string(),
                "voice-proxy".to_string(),
                "--".to_string(),
                "/usr/local/bin/voice-proxy".to_string(),
                "-healthcheck".to_string(),
            ],
            stdin: None,
        },
    )?;
    Ok(())
}

/// Waits for the rollout and runs every post-upgrade check.
pub fn run_smoke(
    runner: &dyn CommandRunner,
    options: &Options,
    simulate_failure: bool,
    probe_proxy: bool,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    checked(
        runner,
        &Invocation {
            program: options.kubectl.clone(),
            args: vec![
                "-n".to_string(),
                options.namespace.clone(),
                "rollout".to_string(),
                "status".to_string(),
                format!("deployment/{}", options.deployment),
                "--timeout=240s".to_string(),
            ],
            stdin: None,
        },
    )?;
    if simulate_failure {
        return Err("simulated smoke failure requested".to_string());
    }
    let metrics = health_metrics(runner, options)?;
    check_health(&metrics)?;
    let mut attempt = || assist_attempt(runner, options);
    let speech = assist_smoke(options, &mut attempt, stdout)?;
    let _ = writeln!(stdout, "Assist reply: {speech}");
    if probe_proxy {
        voice_proxy_healthcheck(runner, options)?;
        let _ = writeln!(stdout, "voice-proxy -healthcheck: ok");
    }
    let logs = checked(
        runner,
        &Invocation {
            program: options.kubectl.clone(),
            args: vec![
                "-n".to_string(),
                options.namespace.clone(),
                "logs".to_string(),
                format!("deployment/{}", options.deployment),
                "-c".to_string(),
                "voice-proxy".to_string(),
                "--since=10m".to_string(),
            ],
            stdin: None,
        },
    )?;
    check_voice_logs(&logs)?;
    let _ = writeln!(stdout, "voice-proxy logs: no errors");
    Ok(())
}

fn write_state(state_dir: &str, name: &str, value: &str) -> Result<(), String> {
    fs::create_dir_all(state_dir).map_err(|error| error.to_string())?;
    fs::write(Path::new(state_dir).join(name), value).map_err(|error| error.to_string())
}

/// Restores the recorded revision only when this job applied an upgrade.
pub fn rollback_only(
    runner: &dyn CommandRunner,
    options: &Options,
    stdout: &mut dyn Write,
) -> Result<i32, String> {
    let applied = Path::new(&options.state_dir).join("upgrade-applied");
    let previous = Path::new(&options.state_dir).join("previous-revision");
    if !applied.exists() || !previous.exists() {
        let _ = writeln!(
            stdout,
            "hermes rollout: no upgrade was applied, nothing to roll back"
        );
        return Ok(0);
    }
    let raw = fs::read_to_string(&previous).map_err(|error| error.to_string())?;
    let revision = raw
        .trim()
        .parse::<i64>()
        .map_err(|error| format!("previous-revision: {error}"))?;
    let _ = writeln!(
        stdout,
        "hermes rollout: rolling back to revision {revision}"
    );
    rollback(runner, options, revision)?;
    checked(
        runner,
        &Invocation {
            program: options.kubectl.clone(),
            args: vec![
                "-n".to_string(),
                options.namespace.clone(),
                "rollout".to_string(),
                "status".to_string(),
                format!("deployment/{}", options.deployment),
                "--timeout=240s".to_string(),
            ],
            stdin: None,
        },
    )?;
    Ok(0)
}

/// Performs the rollout and returns the process exit status.
pub fn run(
    runner: &dyn CommandRunner,
    options: &Options,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<i32, String> {
    if options.rollback_only {
        return rollback_only(runner, options, stdout);
    }
    if options.values_file.is_empty()
        || options.hermes_digest.is_empty()
        || options.voice_proxy_digest.is_empty()
    {
        return Err(
            "--values-file, --hermes-digest and --voice-proxy-digest are required".to_string(),
        );
    }
    if options.smoke_image.is_empty() {
        return Err("--smoke-image is required (the home-deploy image digest)".to_string());
    }
    if options.exporter_digest.is_empty() {
        return Err("--exporter-digest is required (the health-exporter image digest)".to_string());
    }
    if options.chart.is_empty() {
        return Err("--chart is required".to_string());
    }
    if options.state_dir.is_empty() {
        return Err("--state-dir is required".to_string());
    }
    let hermes = normalize_digest("hermes", &options.hermes_digest)?;
    let voice_proxy = normalize_digest("voice-proxy", &options.voice_proxy_digest)?;
    let exporter = normalize_digest("health-exporter", &options.exporter_digest)?;
    let mut options = options.clone();
    options.exporter_digest.clone_from(&exporter);
    let values = load_values(&options.values_file)?;

    let current = live_images(
        runner,
        &options.kubectl,
        &options.namespace,
        &options.deployment,
    )?;
    let mut need_rollout = rollout_needed(&current, &hermes, &voice_proxy);
    let live_exporter = live_images(
        runner,
        &options.kubectl,
        &options.namespace,
        &options.exporter_deployment,
    );
    if live_exporter.is_err() || !exporter_matches(&live_exporter.unwrap_or_default(), &exporter) {
        need_rollout = true;
    }
    if !options.force && !need_rollout {
        let _ = writeln!(
            stdout,
            "hermes rollout: live images already match the built digests; nothing to do"
        );
        return Ok(0);
    }

    let previous = latest_revision(runner, &options.helm, &options.namespace, &options.release);
    write_state(
        &options.state_dir,
        "previous-revision",
        &previous.to_string(),
    )?;
    let _ = writeln!(stdout, "hermes rollout: revision {previous} -> new digests");

    let values_path = write_values(&workload_values(values))?;
    let result = upgrade(
        runner,
        &options,
        &options.chart,
        values_path.path(),
        &hermes,
        &voice_proxy,
    );
    let applied =
        latest_revision(runner, &options.helm, &options.namespace, &options.release) > previous;
    if applied {
        write_state(&options.state_dir, "upgrade-applied", "1")?;
    }
    if result.code != 0 {
        return fail_rollout(
            runner,
            &options,
            previous,
            applied,
            &format!(
                "helm upgrade failed: {}",
                trim(first_non_empty(&[&result.stderr, &result.stdout]))
            ),
            stdout,
            stderr,
        );
    }
    if let Err(cause) = run_smoke(runner, &options, options.simulate_failure, true, stdout) {
        return fail_rollout(runner, &options, previous, applied, &cause, stdout, stderr);
    }

    let _ = fs::remove_file(Path::new(&options.state_dir).join("upgrade-applied"));
    let _ = writeln!(
        stdout,
        "hermes rollout: revision {} is healthy",
        latest_revision(runner, &options.helm, &options.namespace, &options.release)
    );
    Ok(0)
}

fn fail_rollout(
    runner: &dyn CommandRunner,
    options: &Options,
    previous: i64,
    applied: bool,
    cause: &str,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<i32, String> {
    let _ = writeln!(stderr, "hermes rollout failed: {cause}");
    if !applied {
        return Ok(1);
    }
    let _ = writeln!(
        stderr,
        "hermes rollout: rolling back to revision {previous}"
    );
    if let Err(error) = rollback(runner, options, previous) {
        let _ = writeln!(
            stderr,
            "hermes rollout: rollback verification failed: {error}"
        );
        return Ok(2);
    }
    if let Err(error) = run_smoke(runner, options, false, false, stdout) {
        let _ = writeln!(
            stderr,
            "hermes rollout: rollback verification failed: {error}"
        );
        return Ok(2);
    }
    let _ = writeln!(
        stderr,
        "hermes rollout: rollback to revision {previous} verified"
    );
    Ok(1)
}

fn exporter_matches(images: &BTreeMap<String, String>, digest: &str) -> bool {
    if images.is_empty() {
        return false;
    }
    images.values().all(|image| image == digest)
}

fn checked(runner: &dyn CommandRunner, command: &Invocation) -> Result<String, String> {
    let result = runner.run(command);
    if result.code != 0 {
        let detail = trim(first_non_empty(&[&result.stderr, &result.stdout]));
        return Err(format!(
            "{} {} failed: {detail}",
            command.program,
            command.args.join(" ")
        ));
    }
    Ok(result.stdout)
}

fn first_non_empty<'a>(values: &[&'a str]) -> &'a str {
    for value in values {
        if !value.trim().is_empty() {
            return value;
        }
    }
    ""
}

fn trim(value: &str) -> &str {
    value.trim()
}

fn monotonic_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

fn display_value(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => "<nil>".to_string(),
        other => gojson::marshal(other),
    }
}

fn display_map(result: &BTreeMap<String, Value>) -> String {
    let entries: Vec<String> = result
        .iter()
        .map(|(key, value)| format!("{key}:{}", display_value(value)))
        .collect();
    format!("map[{}]", entries.join(" "))
}

/// Performs one Assist turn through the in-cluster HA API and prints the JSON
/// result line the rollout parses.
///
/// The token is read from the mounted Secret file and is never printed. The
/// exit status is 1 on every failure; the structured result lets the caller
/// decide whether a 5xx is a transient Hermes warm-up.
pub fn assist_smoke_client(
    ha_url: &str,
    agent: &str,
    text: &str,
    token_file: &str,
    timeout: Duration,
    stdout: &mut dyn Write,
) -> i32 {
    let mut emit = |payload: Value| {
        let _ = writeln!(stdout, "{}", gojson::marshal(&payload));
    };
    let token = match fs::read_to_string(token_file) {
        Ok(token) => token,
        Err(error) => {
            emit(json!({"ok": false, "error": format!("ReadError: {error}")}));
            return 1;
        }
    };
    let body = gojson::marshal(&json!({
        "text": text,
        "language": "ru",
        "agent_id": agent,
    }));
    let url = format!("{}/api/conversation/process", ha_url.trim_end_matches('/'));
    let config = ureq::Agent::config_builder()
        .proxy(None)
        .http_status_as_error(false)
        .timeout_global(Some(timeout))
        .build();
    let client = ureq::Agent::new_with_config(config);
    let response = client
        .post(&url)
        .header("Authorization", &format!("Bearer {}", token.trim()))
        .header("Content-Type", "application/json")
        .send(body.as_str());
    let mut response = match response {
        Ok(response) => response,
        Err(error) => {
            emit(json!({"ok": false, "error": format!("URLError: {error}")}));
            return 1;
        }
    };
    let status = response.status().as_u16();
    let raw = match response.body_mut().read_to_string() {
        Ok(raw) => raw,
        Err(error) => {
            emit(json!({
                "ok": false,
                "status": status,
                "error": format!("ReadError: {error}"),
            }));
            return 1;
        }
    };
    if status != 200 {
        emit(json!({
            "ok": false,
            "status": status,
            "error": format!("HTTPError: {status}"),
        }));
        return 1;
    }
    let data: Value = match serde_json::from_str(&raw) {
        Ok(data) => data,
        Err(error) => {
            emit(json!({
                "ok": false,
                "status": status,
                "error": format!("JSONError: {error}"),
            }));
            return 1;
        }
    };
    let reply = data.get("response").cloned().unwrap_or(Value::Null);
    let speech = reply
        .get("speech")
        .and_then(|speech| speech.get("plain"))
        .and_then(|plain| plain.get("speech"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let response_type = match reply.get("response_type") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => display_value(other),
    };
    if response_type == "error" || speech.trim().is_empty() {
        emit(json!({
            "ok": false,
            "status": status,
            "response_type": response_type,
            "speech": speech,
            "data": reply.get("data").cloned().unwrap_or(Value::Null),
        }));
        return 1;
    }
    emit(json!({"ok": true, "speech": speech}));
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::Output;
    use crate::process::testing::FakeCommandRunner;

    const HERMES: &str = "ghcr.io/proflyder/hermes@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const VOICE: &str = "ghcr.io/proflyder/voice-proxy@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn all_up_metrics() -> String {
        EXPECTED_ENDPOINTS
            .iter()
            .map(|name| format!("ricardo_endpoint_up{{endpoint=\"{name}\"}} 1"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn accepts_bare_and_pinned_digests() {
        let bare = format!("sha256:{}", "a".repeat(64));
        assert_eq!(
            normalize_digest("hermes", &bare).expect("bare"),
            format!("ghcr.io/proflyder/hermes@{bare}")
        );
        assert_eq!(normalize_digest("hermes", HERMES).expect("pinned"), HERMES);
        assert!(normalize_digest("hermes", "ghcr.io/proflyder/hermes:latest").is_err());
        assert_eq!(
            image_digest(HERMES),
            &HERMES[HERMES.find('@').unwrap() + 1..]
        );
    }

    #[test]
    fn workload_values_drop_storage() {
        let values = yaml::parse(
            "nodeName: lab-host\nstorage:\n  hermes:\n    size: 10Gi\nworkloads:\n  hermes:\n    containers:\n    - name: gateway\n    - name: voice-proxy\n",
        )
        .expect("values");
        let workload = workload_values(values);
        assert!(workload.get("storage").is_none());
        assert!(workload.get("workloads").is_some());
    }

    #[test]
    fn load_values_requires_two_containers() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("values.yaml");
        fs::write(
            &path,
            "workloads:\n  hermes:\n    containers:\n    - name: gateway\n",
        )
        .expect("write");
        let error = load_values(&path.display().to_string()).expect_err("one container");
        assert!(error.contains("must hold gateway and voice-proxy"));
    }

    #[test]
    fn rollout_needed_compares_gateway_and_proxy() {
        let mut current = BTreeMap::new();
        current.insert("gateway".to_string(), HERMES.to_string());
        current.insert("voice-proxy".to_string(), VOICE.to_string());
        assert!(!rollout_needed(&current, HERMES, VOICE));
        assert!(rollout_needed(&current, HERMES, HERMES));
    }

    #[test]
    fn set_args_target_the_container_indexes() {
        assert_eq!(
            set_args(HERMES, VOICE),
            vec![
                "--set",
                &format!("workloads.hermes.containers[0].image={HERMES}"),
                "--set",
                &format!("workloads.hermes.containers[1].image={VOICE}"),
            ]
        );
    }

    #[test]
    fn exporter_args_pin_the_image_and_drop_the_script_mount() {
        let args = exporter_set_args("runtime-health-exporter", "image:new");
        assert!(
            args.iter()
                .any(|arg| arg == "workloads.runtime-health-exporter.volumes=[]")
        );
        assert!(
            args.iter()
                .any(|arg| arg.contains("command=[\"/usr/local/bin/health-exporter\"]"))
        );
    }

    #[test]
    fn health_checks_all_endpoints() {
        assert!(check_health(&all_up_metrics()).is_ok());
        let down = all_up_metrics().replace("endpoint=\"ha\"} 1", "endpoint=\"ha\"} 0");
        assert!(check_health(&down).is_err());
        let missing = all_up_metrics().replace("ricardo_endpoint_up{endpoint=\"piper\"} 1\n", "");
        assert!(
            check_health(&missing)
                .unwrap_err()
                .contains("missing endpoints")
        );
    }

    #[test]
    fn log_scan_matches_errors_only() {
        assert!(check_voice_logs("INFO retrying").is_ok());
        assert!(check_voice_logs("ERROR boom").is_err());
        assert!(voice_proxy_log_errors("a panic: x").len() == 1);
    }

    #[test]
    fn smoke_job_mounts_the_existing_secret() {
        let job = smoke_job("hermes-smoke-1", NAMESPACE, "image");
        let text = gojson::marshal(&job);
        assert!(text.contains("ha-bridge-ha"));
        assert!(job.to_string().contains("/usr/local/bin/home-deploy"));
    }

    #[test]
    fn no_new_digest_skips() {
        let directory = tempfile::tempdir().expect("tempdir");
        let values = directory.path().join("values.yaml");
        fs::write(
            &values,
            "workloads:\n  hermes:\n    containers:\n    - name: gateway\n    - name: voice-proxy\n",
        )
        .expect("values");
        let runner = FakeCommandRunner::new(|command: &Invocation| {
            if command.args.contains(&"deployment".to_string())
                && command.args.contains(&"hermes".to_string())
            {
                return Output {
                    stdout: format!(
                        r#"{{"spec":{{"template":{{"spec":{{"containers":[{{"name":"gateway","image":"{HERMES}"}},{{"name":"voice-proxy","image":"{VOICE}"}}]}}}}}}}}"#
                    ),
                    ..Output::default()
                };
            }
            Output {
                stdout: format!(
                    r#"{{"spec":{{"template":{{"spec":{{"containers":[{{"name":"health-exporter","image":"ghcr.io/proflyder/health-exporter@sha256:{}"}}]}}}}}}}}"#,
                    "c".repeat(64)
                ),
                ..Output::default()
            }
        });
        let options = Options {
            values_file: values.display().to_string(),
            hermes_digest: format!("sha256:{}", "a".repeat(64)),
            voice_proxy_digest: format!("sha256:{}", "b".repeat(64)),
            exporter_digest: format!("sha256:{}", "c".repeat(64)),
            smoke_image: "image".to_string(),
            chart: "chart".to_string(),
            state_dir: directory.path().display().to_string(),
            ..Options::default()
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(&runner, &options, &mut stdout, &mut stderr).expect("run");
        assert_eq!(code, 0);
        assert!(
            String::from_utf8(stdout)
                .expect("utf8")
                .contains("nothing to do")
        );
    }
}
