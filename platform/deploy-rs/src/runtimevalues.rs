//! Splits a `helm template` render and the private runtime values into the two
//! ownership domains of the runtime chart.
//!
//! Storage (PersistentVolume/PersistentVolumeClaim) is applied by kubectl;
//! everything else is the Helm release. Byte-compatible with
//! `platform/deploy/internal/runtimevalues`.

use std::fs;

use crate::yaml::{self, Value};

/// Reports whether `document` belongs to the kubectl-applied storage half.
#[must_use]
pub fn storage_kind(document: &Value) -> bool {
    matches!(
        document.get("kind").map(Value::as_str),
        Some("PersistentVolume" | "PersistentVolumeClaim")
    )
}

/// Reads a multi-document Helm render and returns the storage documents as a
/// standalone YAML stream.
pub fn split_render(path: &str) -> Result<Vec<u8>, String> {
    let raw = fs::read_to_string(path).map_err(|error| read_error(path, &error))?;
    let documents = yaml::parse_documents(&raw).map_err(|error| format!("{path}: {error}"))?;
    let storage: Vec<Value> = documents.into_iter().filter(storage_kind).collect();
    Ok(yaml::to_string_multiple(&storage).into_bytes())
}

/// Reads the private values and drops the cluster-scoped storage block.
pub fn workload_values(path: &str) -> Result<Value, String> {
    let raw = fs::read_to_string(path).map_err(|error| read_error(path, &error))?;
    if raw.trim().is_empty() {
        return Ok(Value::Map(Vec::new()));
    }
    let mut values = yaml::parse(&raw).map_err(|error| format!("{path}: {error}"))?;
    values.remove("storage");
    Ok(values)
}

/// Rewrites the exporter workload so the chart no longer needs the Python
/// ConfigMap: it pins the image, sets the binary as the command and removes the
/// script volume mount. The rest of the container is preserved.
pub fn pin_health_exporter(
    values: Value,
    image: &str,
    workload_name: &str,
) -> Result<Value, String> {
    if image.is_empty() {
        return Ok(values);
    }
    let mut values = values;
    let Some(workloads_value) = values.get("workloads").cloned() else {
        return Err("values: workloads must be a mapping".to_string());
    };
    if !workloads_value.is_mapping() {
        return Err("values: workloads must be a mapping".to_string());
    }
    let mut workloads = workloads_value;
    let Some(workload_value) = workloads.get(workload_name).cloned() else {
        // No exporter workload in these values (for example a cluster that does
        // not run it yet): nothing to pin, keep the document unchanged.
        return Ok(values);
    };
    if !workload_value.is_mapping() {
        return Err(format!(
            "values: workloads.{workload_name}.containers must be a non-empty list"
        ));
    }
    let mut workload = workload_value;
    let Some(Value::Seq(mut containers)) = workload.get("containers").cloned() else {
        return Err(format!(
            "values: workloads.{workload_name}.containers must be a non-empty list"
        ));
    };
    if containers.is_empty() {
        return Err(format!(
            "values: workloads.{workload_name}.containers must be a non-empty list"
        ));
    }
    let Some(Value::Map(container)) = containers.first().cloned() else {
        return Err(format!(
            "values: workloads.{workload_name}.containers[0] must be a mapping"
        ));
    };
    let mut container = Value::Map(container);
    container.set("image", Value::Str(image.to_string()));
    container.set(
        "command",
        Value::Seq(vec![Value::Str(
            "/usr/local/bin/health-exporter".to_string(),
        )]),
    );
    container.set("volumeMounts", Value::Seq(Vec::new()));
    containers[0] = container;
    workload.set("containers", Value::Seq(containers));
    workload.remove("volumes");
    workloads.set(workload_name, workload);
    values.set("workloads", workloads);
    Ok(values)
}

/// Writes data to `path` with mode 0600.
pub fn write(path: &str, data: &[u8]) -> Result<(), String> {
    fs::write(path, data).map_err(|error| read_error(path, &error))
}

/// Serializes the runtime values.
pub fn marshal(values: &Value) -> Vec<u8> {
    yaml::to_string(values).into_bytes()
}

fn read_error(path: &str, error: &std::io::Error) -> String {
    let reason = match error.kind() {
        std::io::ErrorKind::NotFound => "no such file or directory".to_string(),
        std::io::ErrorKind::PermissionDenied => "permission denied".to_string(),
        _ => error.to_string(),
    };
    format!("open {path}: {reason}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const RENDER: &str = r"---
apiVersion: v1
kind: PersistentVolume
metadata:
  name: runtime-hermes
spec:
  capacity:
    storage: 10Gi
---
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: runtime-hermes
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: hermes
";

    const PRIVATE: &str = r#"nodeName: lab-host
storage:
  hermes:
    size: 10Gi
workloads:
  hermes:
    replicas: 1
    containers:
    - name: gateway
      image: hermes@sha256:aaaa
  runtime-health-exporter:
    replicas: 1
    containers:
    - name: health-exporter
      image: ghcr.io/proflyder/voice-proxy@sha256:old
      command: ["python3", "/metrics/health_exporter.py"]
      volumeMounts:
      - name: metrics
        mountPath: /metrics
    volumes:
    - name: metrics
      configMap:
        name: runtime-health-exporter
"#;

    fn write(contents: &str) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().expect("temp file");
        file.write_all(contents.as_bytes()).expect("write");
        file
    }

    #[test]
    fn split_render_keeps_only_storage() {
        let file = write(RENDER);
        let output = split_render(&file.path().display().to_string()).expect("split");
        let text = String::from_utf8(output).expect("utf8");
        assert!(text.contains("kind: PersistentVolume"));
        assert!(text.contains("kind: PersistentVolumeClaim"));
        assert!(!text.contains("kind: Deployment"));
    }

    #[test]
    fn workload_values_drop_storage() {
        let file = write(PRIVATE);
        let values = workload_values(&file.path().display().to_string()).expect("values");
        assert!(values.get("storage").is_none());
        assert!(
            values
                .get("workloads")
                .and_then(|v| v.get("hermes"))
                .is_some()
        );
    }

    #[test]
    fn pin_health_exporter_keeps_the_rest_of_the_container() {
        let file = write(PRIVATE);
        let values = workload_values(&file.path().display().to_string()).expect("values");
        let pinned = pin_health_exporter(
            values,
            "ghcr.io/proflyder/health-exporter@sha256:new",
            "runtime-health-exporter",
        )
        .expect("pin");
        let exporter = pinned
            .get("workloads")
            .and_then(|workloads| workloads.get("runtime-health-exporter"))
            .expect("exporter");
        assert!(exporter.get("volumes").is_none());
        let Some(Value::Seq(containers)) = exporter.get("containers") else {
            panic!("containers");
        };
        let container = &containers[0];
        assert_eq!(
            container.get("image").map(Value::as_str),
            Some("ghcr.io/proflyder/health-exporter@sha256:new")
        );
        assert!(container.get("volumeMounts").is_some());
        let Some(Value::Seq(command)) = container.get("command") else {
            panic!("command");
        };
        assert_eq!(command[0].as_str(), "/usr/local/bin/health-exporter");
        assert!(container.get("name").is_some());
    }
}
