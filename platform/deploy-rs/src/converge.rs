//! Replaces a live Deployment with its rendered manifest.
//!
//! `kubectl apply`/`patch` only add or update fields, so drift added by an
//! earlier ad-hoc patch stays forever. This helper renders the desired object
//! and replaces the live Deployment with it, keeping the same argv, carry-over
//! rules and messages as `platform/deploy/internal/converge`.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Map, Value};

use crate::gojson;
use crate::process::Runner;
use crate::yaml;

/// One converge request.
pub struct Options<'a> {
    /// Kubernetes namespace.
    pub namespace: &'a str,
    /// Deployment name.
    pub name: &'a str,
    /// Rendered multi-document manifest.
    pub file: &'a str,
    /// Keep the live replica count.
    pub preserve_replicas: bool,
    /// Keep the live container env/envFrom.
    pub preserve_env: bool,
    /// Append `<name>\t<status>` after the Deployment was applied.
    pub summary_file: &'a str,
}

/// Parses every non-empty YAML document in the rendered file into JSON objects.
pub fn desired_objects(path: &str) -> Result<Vec<Map<String, Value>>, String> {
    let raw =
        fs::read_to_string(path).map_err(|error| format!("open {path}: {}", reason(&error)))?;
    let documents = yaml::parse_documents(&raw).map_err(|error| format!("{path}: {error}"))?;
    let mut objects = Vec::new();
    for document in &documents {
        let json = document.to_json();
        let Value::Object(object) = json else {
            continue;
        };
        if object.is_empty() {
            continue;
        }
        objects.push(object);
    }
    Ok(objects)
}

/// Returns the single Deployment document.
pub fn desired_deployment(objects: &[Map<String, Value>], path: &str) -> Result<Value, String> {
    let deployments: Vec<&Map<String, Value>> = objects
        .iter()
        .filter(|object| object.get("kind").and_then(Value::as_str) == Some("Deployment"))
        .collect();
    if deployments.len() != 1 {
        return Err(format!(
            "{path}: expected exactly one Deployment, found {}",
            deployments.len()
        ));
    }
    Ok(Value::Object(deployments[0].clone()))
}

/// Drives kubectl and returns the human-readable result line.
pub fn converge(options: &Options, runner: &dyn Runner) -> Result<String, String> {
    let objects = desired_objects(options.file)?;
    let mut desired = desired_deployment(&objects, options.file)?;

    let support: Vec<Map<String, Value>> = objects
        .iter()
        .filter(|object| object.get("kind").and_then(Value::as_str) != Some("Deployment"))
        .cloned()
        .collect();
    apply_support_objects(options, &support, runner)?;

    let Some(current) = live_deployment(options, runner) else {
        let path = temp_file("yaml", serialize_yaml(&desired).as_bytes())?;
        let result = runner.run(&argv(&[
            "-n",
            options.namespace,
            "create",
            "-f",
            &path.path().display().to_string(),
        ]));
        if result.code != 0 {
            return Err(format!(
                "{}: create failed: {}",
                options.name,
                strip(&result.stderr)
            ));
        }
        record_summary(options, "created")?;
        return Ok(format!("{}: created from manifest", options.name));
    };

    let before = generation(&current);
    merge_runtime_state(
        &mut desired,
        &current,
        options.preserve_replicas,
        options.preserve_env,
    );

    let payload = gojson::marshal(&desired);
    let path = temp_file("json", payload.as_bytes())?;
    let result = runner.run(&argv(&[
        "-n",
        options.namespace,
        "replace",
        "-f",
        &path.path().display().to_string(),
    ]));
    if result.code != 0 {
        return Err(format!(
            "{}: replace failed: {}",
            options.name,
            strip(&result.stderr)
        ));
    }
    record_summary(options, &rollout_status(options, &before, runner))?;
    Ok(format!("{}: converged to manifest", options.name))
}

/// Reads `metadata.generation`. The API server increments it exactly when the
/// desired state changes, which is the same signal `kubectl apply` uses.
fn generation(object: &Value) -> String {
    let Some(value) = object
        .get("metadata")
        .and_then(|metadata| metadata.get("generation"))
    else {
        return String::new();
    };
    match value {
        Value::Number(number) => number_text(number),
        Value::String(text) => text.clone(),
        Value::Bool(value) => value.to_string(),
        _ => String::new(),
    }
}

/// Compares the generation before the replace with the one reported afterwards.
fn rollout_status(options: &Options, before: &str, runner: &dyn Runner) -> String {
    if options.summary_file.is_empty() {
        return String::new();
    }
    let result = runner.run(&argv(&[
        "-n",
        options.namespace,
        "get",
        "deployment",
        options.name,
        "-o",
        "jsonpath={.metadata.generation}",
    ]));
    if result.code != 0 {
        return "unknown".to_string();
    }
    let after = result.stdout.trim();
    if before.is_empty() || after.is_empty() {
        "unknown".to_string()
    } else if before == after {
        "unchanged".to_string()
    } else {
        "rolled-out".to_string()
    }
}

fn record_summary(options: &Options, status: &str) -> Result<(), String> {
    if options.summary_file.is_empty() || status.is_empty() {
        return Ok(());
    }
    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(options.summary_file)
        .map_err(|error| format!("{}: open summary: {error}", options.name))?;
    writeln!(file, "{}\t{status}", options.name)
        .map_err(|error| format!("{}: write summary: {error}", options.name))
}

fn apply_support_objects(
    options: &Options,
    objects: &[Map<String, Value>],
    runner: &dyn Runner,
) -> Result<(), String> {
    if objects.is_empty() {
        return Ok(());
    }
    let mut documents = String::new();
    for (index, object) in objects.iter().enumerate() {
        if index > 0 {
            documents.push_str("---\n");
        }
        documents.push_str(&serialize_yaml(&Value::Object(object.clone())));
    }
    let path = temp_file("yaml", documents.as_bytes())?;
    let result = runner.run(&argv(&[
        "-n",
        options.namespace,
        "apply",
        "-f",
        &path.path().display().to_string(),
    ]));
    if result.code != 0 {
        return Err(format!(
            "support objects: apply failed: {}",
            strip(&result.stderr)
        ));
    }
    Ok(())
}

fn live_deployment(options: &Options, runner: &dyn Runner) -> Option<Value> {
    let result = runner.run(&argv(&[
        "-n",
        options.namespace,
        "get",
        "deployment",
        options.name,
        "-o",
        "json",
    ]));
    if result.code != 0 {
        return None;
    }
    serde_json::from_str(&result.stdout).ok()
}

fn number_text(number: &serde_json::Number) -> String {
    if let Some(value) = number.as_i64() {
        return value.to_string();
    }
    if let Some(value) = number.as_u64() {
        return value.to_string();
    }
    number
        .as_f64()
        .map_or_else(String::new, |value| format!("{value}"))
}

fn merge_runtime_state(
    desired: &mut Value,
    current: &Value,
    preserve_replicas: bool,
    preserve_env: bool,
) {
    if !desired.get("spec").is_some_and(Value::is_object) {
        desired["spec"] = Value::Object(Map::new());
    }
    let current_spec = current.get("spec").filter(|value| value.is_object());

    if preserve_replicas
        && let (Some(current_spec), Some(desired_spec)) = (current_spec, desired.get_mut("spec"))
        && let Some(replicas) = current_spec.get("replicas")
    {
        desired_spec["replicas"] = replicas.clone();
    }

    if preserve_env
        && let Some(current_spec) = current_spec
        && let Some(desired_pod) = desired
            .get_mut("spec")
            .and_then(|spec| spec.get_mut("template"))
            .and_then(|template| template.get_mut("spec"))
        && let Some(Value::Array(list)) = desired_pod.get_mut("containers")
    {
        let current_containers = containers(current_spec);
        for item in list {
            let Value::Object(container) = item else {
                continue;
            };
            let Some(name) = container.get("name").and_then(Value::as_str) else {
                continue;
            };
            let Some(live) = current_containers.get(name) else {
                continue;
            };
            for key in ["env", "envFrom"] {
                if let Some(value) = live.get(key)
                    && truthy(value)
                {
                    container.insert(key.to_string(), value.clone());
                }
            }
        }
    }

    if let Some(version) = nested_string(current, &["metadata", "resourceVersion"])
        && !version.is_empty()
    {
        if !desired.get("metadata").is_some_and(Value::is_object) {
            desired["metadata"] = Value::Object(Map::new());
        }
        desired["metadata"]["resourceVersion"] = Value::String(version);
    }
}

fn containers(spec: &Value) -> Map<String, Value> {
    let mut result = Map::new();
    let Some(list) = spec
        .get("template")
        .and_then(|template| template.get("spec"))
        .and_then(|pod| pod.get("containers"))
        .and_then(Value::as_array)
    else {
        return result;
    };
    for item in list {
        let Value::Object(container) = item else {
            continue;
        };
        if let Some(name) = container.get("name").and_then(Value::as_str) {
            result.insert(name.to_string(), item.clone());
        }
    }
    result
}

fn nested_string(object: &Value, keys: &[&str]) -> Option<String> {
    let mut value = object;
    for key in keys {
        value = value.get(key)?;
    }
    value.as_str().map(str::to_string)
}

/// Python's truth test for the values handled here.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(number) => number.as_f64().is_some_and(|value| value != 0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(entries) => !entries.is_empty(),
    }
}

fn serialize_yaml(object: &Value) -> String {
    yaml::to_string(&yaml::Value::from_json(object))
}

fn argv(args: &[&str]) -> Vec<String> {
    args.iter()
        .map(|argument| (*argument).to_string())
        .collect()
}

fn strip(value: &str) -> String {
    value.trim_end_matches([' ', '\n', '\r', '\t']).to_string()
}

fn reason(error: &std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::NotFound => "no such file or directory".to_string(),
        std::io::ErrorKind::PermissionDenied => "permission denied".to_string(),
        _ => error.to_string(),
    }
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Writes a temporary file and removes it on drop.
struct TempFile(PathBuf);

impl TempFile {
    fn path(&self) -> &PathBuf {
        &self.0
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn temp_file(extension: &str, contents: &[u8]) -> Result<TempFile, String> {
    let index = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "home-deploy-{}-{index}.{extension}",
        std::process::id()
    ));
    let mut file = fs::File::create(&path).map_err(|error| error.to_string())?;
    file.write_all(contents)
        .map_err(|error| error.to_string())?;
    drop(file);
    Ok(TempFile(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::Output;
    use crate::process::testing::FakeRunner;

    const MANIFEST: &str = r#"apiVersion: apps/v1
kind: Deployment
metadata:
  name: ha-bridge
  namespace: home-assistant
  labels:
    app.kubernetes.io/name: ha-bridge
spec:
  replicas: 0
  strategy:
    type: Recreate
  selector:
    matchLabels:
      app.kubernetes.io/name: ha-bridge
  template:
    spec:
      containers:
        - name: ha-bridge
          image: ghcr.io/proflyder/ha-bridge@sha256:aaa
          env:
            - name: BRIDGE_EXECUTE
              value: "false"
"#;

    const LIVE: &str = r#"{"metadata":{"name":"ha-bridge","generation":18,"resourceVersion":"12345"},"spec":{"replicas":1,"template":{"spec":{"containers":[{"name":"ha-bridge","env":[{"name":"BRIDGE_EXECUTE","value":"true"}],"volumeMounts":[{"name":"bin","mountPath":"/opt/home/bin"}]}]}}}}"#;

    fn manifest_file() -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().expect("temp file");
        file.write_all(MANIFEST.as_bytes()).expect("write");
        file
    }

    #[test]
    fn desired_objects_and_deployment_are_selected() {
        let file = manifest_file();
        let path = file.path().display().to_string();
        let objects = desired_objects(&path).expect("objects");
        assert_eq!(objects.len(), 1);
        let deployment = desired_deployment(&objects, &path).expect("deployment");
        assert_eq!(deployment["spec"]["replicas"], 0);
    }

    #[test]
    fn exactly_one_deployment_is_required() {
        let objects = Vec::new();
        let error = desired_deployment(&objects, "manifest.yaml").expect_err("no deployment");
        assert!(error.contains("expected exactly one Deployment, found 0"));
    }

    #[test]
    fn preserve_replicas_and_env_keep_runtime_state() {
        let file = manifest_file();
        let path = file.path().display().to_string();
        let objects = desired_objects(&path).expect("objects");
        let mut desired = desired_deployment(&objects, &path).expect("deployment");
        let current: Value = serde_json::from_str(LIVE).expect("live");
        merge_runtime_state(&mut desired, &current, true, true);
        assert_eq!(desired["spec"]["replicas"], 1);
        assert_eq!(
            desired["spec"]["template"]["spec"]["containers"][0]["env"][0]["value"],
            "true"
        );
        // Manifest drift still wins: the live bin mount is not carried over.
        assert!(
            desired["spec"]["template"]["spec"]["containers"][0]
                .get("volumeMounts")
                .is_none()
        );
        assert_eq!(desired["metadata"]["resourceVersion"], "12345");
    }

    #[test]
    fn manifest_wins_without_preserve_flags() {
        let file = manifest_file();
        let path = file.path().display().to_string();
        let objects = desired_objects(&path).expect("objects");
        let mut desired = desired_deployment(&objects, &path).expect("deployment");
        let current: Value = serde_json::from_str(LIVE).expect("live");
        merge_runtime_state(&mut desired, &current, false, false);
        assert_eq!(desired["spec"]["replicas"], 0);
        assert_eq!(
            desired["spec"]["template"]["spec"]["containers"][0]["env"][0]["value"],
            "false"
        );
    }

    #[test]
    fn create_path_uses_the_manifest_and_summary() {
        let file = manifest_file();
        let path = file.path().display().to_string();
        let summary = tempfile::NamedTempFile::new().expect("summary");
        let summary_path = summary.path().display().to_string();
        let runner = FakeRunner::new(|args: &[String]| {
            if args.contains(&"create".to_string()) {
                Output::default()
            } else {
                Output {
                    code: 1,
                    stderr: "Error from server (NotFound): deployments.apps not found".to_string(),
                    ..Output::default()
                }
            }
        });
        let message = converge(
            &Options {
                namespace: "home-assistant",
                name: "ha-bridge",
                file: &path,
                preserve_replicas: false,
                preserve_env: false,
                summary_file: &summary_path,
            },
            &runner,
        )
        .expect("converge");
        assert_eq!(message, "ha-bridge: created from manifest");
        assert_eq!(
            fs::read_to_string(&summary_path).expect("summary"),
            "ha-bridge\tcreated\n"
        );
        let calls = runner.calls();
        assert_eq!(calls[0][2], "get");
        assert_eq!(calls[1][2], "create");
    }

    #[test]
    fn replace_failure_names_the_deployment_and_stderr() {
        let file = manifest_file();
        let path = file.path().display().to_string();
        let runner = FakeRunner::new(|args: &[String]| {
            if args.contains(&"replace".to_string()) {
                Output {
                    code: 1,
                    stderr: "boom".to_string(),
                    ..Output::default()
                }
            } else {
                Output {
                    stdout: LIVE.to_string(),
                    ..Output::default()
                }
            }
        });
        let error = converge(
            &Options {
                namespace: "home-assistant",
                name: "ha-bridge",
                file: &path,
                preserve_replicas: false,
                preserve_env: false,
                summary_file: "",
            },
            &runner,
        )
        .expect_err("replace must fail");
        assert_eq!(error, "ha-bridge: replace failed: boom");
    }
}
