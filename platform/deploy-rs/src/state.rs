//! Deploy rollback markers.
//!
//! `deploy.sh` records the live Deployment (`.before.json`) or its absence
//! (`.absent`) immediately before applying a change and writes `.applied` only
//! after the apply succeeded. Rollback restores from that record instead of
//! running `kubectl rollout undo`; the marker contract is byte-identical with
//! the Go implementation so a mixed run stays safe.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::gojson;
use crate::process::Runner;

/// Remembers the live Deployment name (or its absence) and clears the applied
/// marker. `state_dir` must already exist.
///
/// The probe is fail-closed: only a NotFound answer from the API server means
/// "the Deployment does not exist". A transient, authorization or transport
/// error aborts the record without writing a marker, because an `.absent`
/// marker written there would make a later rollback delete a live Deployment.
pub fn record(
    state_dir: &str,
    namespace: &str,
    name: &str,
    runner: &dyn Runner,
) -> Result<(), String> {
    let result = runner.run(&argv(&["-n", namespace, "get", "deployment", name]));
    if result.code != 0 {
        if !not_found(&result.stderr) {
            return Err(format!(
                "{name}: cannot record deployment: {}",
                strip(&result.stderr)
            ));
        }
        fs::write(marker(state_dir, name, ".absent"), []).map_err(|error| error.to_string())?;
    } else {
        let live = runner.run(&argv(&[
            "-n",
            namespace,
            "get",
            "deployment",
            name,
            "-o",
            "json",
        ]));
        if live.code != 0 {
            return Err(format!(
                "{name}: cannot record deployment: {}",
                strip(&live.stderr)
            ));
        }
        fs::write(
            marker(state_dir, name, ".before.json"),
            live.stdout.as_bytes(),
        )
        .map_err(|error| error.to_string())?;
    }
    remove_if_exists(&marker(state_dir, name, ".applied"))
}

/// Reports whether a failed `kubectl get` means the object is genuinely absent.
fn not_found(stderr: &str) -> bool {
    let lower = stderr.to_lowercase();
    lower.contains("notfound") || lower.contains("not found")
}

/// Records that the Deployment change was applied by this job.
pub fn mark(state_dir: &str, name: &str) -> Result<(), String> {
    fs::write(marker(state_dir, name, ".applied"), []).map_err(|error| error.to_string())
}

/// Restores every Deployment marked as applied and returns one message each.
pub fn rollback(state_dir: &str, namespace: &str, runner: &dyn Runner) -> Vec<String> {
    let directory = Path::new(state_dir);
    if !directory.is_dir() {
        return vec!["no deploy state recorded, nothing to roll back".to_string()];
    }
    let Ok(entries) = fs::read_dir(directory) else {
        return vec!["no deploy state recorded, nothing to roll back".to_string()];
    };
    let mut markers: Vec<PathBuf> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let is_applied = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".applied"));
            if is_applied && path.is_file() {
                Some(path)
            } else {
                None
            }
        })
        .collect();
    if markers.is_empty() {
        return vec!["no deployment was applied by this job, nothing to roll back".to_string()];
    }
    markers.sort();
    markers
        .iter()
        .map(|path| {
            let file_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            let name = file_name.strip_suffix(".applied").unwrap_or(file_name);
            restore_one(namespace, state_dir, name, runner)
        })
        .collect()
}

fn restore_one(namespace: &str, state_dir: &str, name: &str, runner: &dyn Runner) -> String {
    let applied = marker(state_dir, name, ".applied");
    let before_path = marker(state_dir, name, ".before.json");
    let absent = marker(state_dir, name, ".absent");

    if !Path::new(&applied).exists() {
        return format!("{name}: not applied by this job, skip");
    }

    if Path::new(&absent).exists() {
        let result = runner.run(&argv(&[
            "-n",
            namespace,
            "delete",
            "deployment",
            name,
            "--ignore-not-found",
        ]));
        if result.code != 0 {
            return format!("{name}: delete failed: {}", strip(&result.stderr));
        }
        return format!("{name}: was absent before, deleted");
    }

    let Ok(raw) = fs::read(&before_path) else {
        return format!("{name}: applied but no recorded state, skip");
    };
    let Ok(before) = serde_json::from_slice::<Value>(&raw) else {
        return format!("{name}: recorded state has no generation, skip");
    };
    if !before.is_object() {
        return format!("{name}: recorded state has no generation, skip");
    }
    let Some(before_generation) = generation_field(&before) else {
        return format!("{name}: recorded state has no generation, skip");
    };

    let Some(current) = generation(namespace, name, runner) else {
        return format!("{name}: deployment not found, skip");
    };
    if current <= before_generation {
        return format!(
            "{name}: generation {current} did not change (was {before_generation}), skip"
        );
    }

    let patch = restore_object(&before);
    let payload = gojson::marshal(&patch);
    let Ok((path, _guard)) = temp_file("json", payload.as_bytes()) else {
        return format!("{name}: restore replace failed: cannot create a temporary file");
    };
    let result = runner.run(&argv(&[
        "-n",
        namespace,
        "replace",
        "-f",
        &path.display().to_string(),
    ]));
    if result.code != 0 {
        return format!("{name}: restore replace failed: {}", strip(&result.stderr));
    }

    let rollout = runner.run(&argv(&[
        "-n",
        namespace,
        "rollout",
        "status",
        &format!("deployment/{name}"),
        "--timeout=180s",
    ]));
    if rollout.code != 0 {
        return format!(
            "{name}: restored to generation {before_generation}, rollout not ready: {}",
            strip(&rollout.stderr)
        );
    }
    format!("{name}: restored to generation {before_generation}")
}

/// Strips read-only fields so the recorded object can be PUT back as-is.
#[must_use]
pub fn restore_object(before: &Value) -> Value {
    let mut object = before.clone();
    if let Value::Object(map) = &mut object {
        map.remove("status");
        if let Some(Value::Object(metadata)) = map.get_mut("metadata") {
            for key in [
                "managedFields",
                "generation",
                "creationTimestamp",
                "selfLink",
                "uid",
                "resourceVersion",
            ] {
                metadata.remove(key);
            }
        }
    }
    object
}

fn generation(namespace: &str, name: &str, runner: &dyn Runner) -> Option<i64> {
    let result = runner.run(&argv(&[
        "-n",
        namespace,
        "get",
        "deployment",
        name,
        "-o",
        "jsonpath={.metadata.generation}",
    ]));
    if result.code != 0 {
        return None;
    }
    result.stdout.trim().parse().ok()
}

fn generation_field(before: &Value) -> Option<i64> {
    let value = before.get("metadata")?.get("generation")?;
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
        .or_else(|| value.as_f64().map(|value| value as i64))
}

fn marker(state_dir: &str, name: &str, suffix: &str) -> PathBuf {
    Path::new(state_dir).join(format!("{name}{suffix}"))
}

fn remove_if_exists(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn argv(args: &[&str]) -> Vec<String> {
    args.iter()
        .map(|argument| (*argument).to_string())
        .collect()
}

fn strip(value: &str) -> String {
    value.trim_end_matches([' ', '\n', '\r', '\t']).to_string()
}

static TEMP_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

struct TempFile(PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn temp_file(extension: &str, contents: &[u8]) -> Result<(PathBuf, TempFile), String> {
    let index = TEMP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "home-deploy-{}-{index}.{extension}",
        std::process::id()
    ));
    fs::write(&path, contents).map_err(|error| error.to_string())?;
    Ok((path.clone(), TempFile(path)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::Output;
    use crate::process::testing::FakeRunner;

    fn before_object(generation: i64, image: &str) -> String {
        format!(
            r#"{{"metadata":{{"name":"scheduler","generation":{generation}}},"spec":{{"replicas":1,"template":{{"spec":{{"containers":[{{"name":"scheduler","image":"{image}"}}]}}}}}}}}"#
        )
    }

    #[test]
    fn no_markers_means_no_calls() {
        let directory = tempfile::tempdir().expect("tempdir");
        let runner = FakeRunner::new(|_args: &[String]| Output::default());
        let messages = rollback(
            &directory.path().display().to_string(),
            "home-assistant",
            &runner,
        );
        assert!(runner.calls().is_empty());
        assert_eq!(messages.len(), 1);
        assert!(messages[0].contains("nothing to roll back"));
    }

    #[test]
    fn apply_failure_before_the_marker_does_not_roll_back() {
        let directory = tempfile::tempdir().expect("tempdir");
        fs::write(
            directory.path().join("scheduler.before.json"),
            before_object(3, "old"),
        )
        .expect("write");
        let runner = FakeRunner::new(|_args: &[String]| Output::default());
        rollback(
            &directory.path().display().to_string(),
            "home-assistant",
            &runner,
        );
        assert!(runner.calls().is_empty());
    }

    #[test]
    fn unchanged_generation_is_not_rolled_back() {
        let directory = tempfile::tempdir().expect("tempdir");
        fs::write(
            directory.path().join("scheduler.before.json"),
            before_object(3, "old"),
        )
        .expect("write");
        fs::write(directory.path().join("scheduler.applied"), []).expect("write");
        let runner = FakeRunner::new(|_args: &[String]| Output {
            stdout: "3".to_string(),
            ..Output::default()
        });
        let messages = rollback(
            &directory.path().display().to_string(),
            "home-assistant",
            &runner,
        );
        assert!(!runner.verbs().contains(&"replace".to_string()));
        assert!(messages[0].contains("did not change"));
    }

    #[test]
    fn applied_change_is_restored_to_the_recorded_object() {
        let directory = tempfile::tempdir().expect("tempdir");
        let before = r#"{"metadata":{"name":"scheduler","generation":7,"resourceVersion":"42","uid":"uid-1"},"spec":{"replicas":1},"status":{"replicas":1}}"#;
        fs::write(directory.path().join("scheduler.before.json"), before).expect("write");
        fs::write(directory.path().join("scheduler.applied"), []).expect("write");
        let runner = FakeRunner::new(|args: &[String]| {
            if args.contains(&"replace".to_string()) {
                let path = &args[args.iter().position(|arg| arg == "-f").expect("file") + 1];
                let payload = fs::read_to_string(path).expect("payload");
                assert_eq!(
                    payload,
                    r#"{"metadata":{"name":"scheduler"},"spec":{"replicas":1}}"#
                );
                return Output::default();
            }
            if args.contains(&"rollout".to_string()) {
                return Output::default();
            }
            Output {
                stdout: "8".to_string(),
                ..Output::default()
            }
        });
        let messages = rollback(
            &directory.path().display().to_string(),
            "home-assistant",
            &runner,
        );
        let verbs = runner.verbs().join(",");
        assert!(verbs.contains("replace"));
        assert!(verbs.contains("rollout"));
        assert_eq!(messages[0], "scheduler: restored to generation 7");
    }

    #[test]
    fn first_install_is_deleted() {
        let directory = tempfile::tempdir().expect("tempdir");
        for name in ["ha-bridge.absent", "ha-bridge.applied"] {
            fs::write(directory.path().join(name), []).expect("write");
        }
        let runner = FakeRunner::new(|_args: &[String]| Output::default());
        let messages = rollback(
            &directory.path().display().to_string(),
            "home-assistant",
            &runner,
        );
        assert!(runner.verbs().contains(&"delete".to_string()));
        assert!(messages[0].contains("deleted"));
    }

    #[test]
    fn marker_without_state_is_skipped() {
        let directory = tempfile::tempdir().expect("tempdir");
        fs::write(directory.path().join("scheduler.applied"), []).expect("write");
        let runner = FakeRunner::new(|_args: &[String]| Output {
            stdout: "4".to_string(),
            ..Output::default()
        });
        let messages = rollback(
            &directory.path().display().to_string(),
            "home-assistant",
            &runner,
        );
        assert!(!runner.verbs().contains(&"replace".to_string()));
        assert!(messages[0].contains("no recorded state"));
    }

    #[test]
    fn record_and_mark_keep_the_marker_contract() {
        let directory = tempfile::tempdir().expect("tempdir");
        let live = r#"{"metadata":{"name":"scheduler","generation":3},"spec":{}}"#;
        let runner = FakeRunner::new(|args: &[String]| {
            if args.contains(&"-o".to_string()) {
                Output {
                    stdout: live.to_string(),
                    ..Output::default()
                }
            } else {
                Output::default()
            }
        });
        record(
            &directory.path().display().to_string(),
            "home-assistant",
            "scheduler",
            &runner,
        )
        .expect("record");
        assert_eq!(
            fs::read_to_string(directory.path().join("scheduler.before.json")).expect("before"),
            live
        );
        mark(&directory.path().display().to_string(), "scheduler").expect("mark");
        assert!(directory.path().join("scheduler.applied").exists());
    }

    #[test]
    fn record_absent_writes_the_absent_marker() {
        let directory = tempfile::tempdir().expect("tempdir");
        let runner = FakeRunner::new(|_args: &[String]| Output {
            code: 1,
            stderr: "Error from server (NotFound): deployments.apps \"scheduler\" not found"
                .to_string(),
            ..Output::default()
        });
        record(
            &directory.path().display().to_string(),
            "home-assistant",
            "scheduler",
            &runner,
        )
        .expect("record");
        assert!(directory.path().join("scheduler.absent").exists());
        assert!(!directory.path().join("scheduler.before.json").exists());
    }

    #[test]
    fn transient_get_errors_are_fail_closed() {
        let directory = tempfile::tempdir().expect("tempdir");
        let runner = FakeRunner::new(|_args: &[String]| Output {
            code: 1,
            stderr: "Error from server (Forbidden): deployments.apps \"scheduler\" is forbidden"
                .to_string(),
            ..Output::default()
        });
        let error = record(
            &directory.path().display().to_string(),
            "home-assistant",
            "scheduler",
            &runner,
        )
        .expect_err("record must fail on a non-NotFound get error");
        assert!(error.contains("cannot record deployment"));
        assert_eq!(runner.calls().len(), 1);
        for suffix in [".absent", ".before.json", ".applied"] {
            assert!(!directory.path().join(format!("scheduler{suffix}")).exists());
        }
    }
}
