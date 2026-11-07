//! Manifest helpers used by the deploy scripts.
//!
//! `eso names` lists the ExternalSecrets of multi-document manifests so
//! `external-secrets/deploy.sh` can `kubectl wait` for them, and
//! `secret opaque` builds the one-key bootstrap Secret from a private dotenv
//! file. Both are byte-compatible with `platform/deploy/internal/manifests`.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use base64::Engine;

use crate::envfile;
use crate::yaml::{self, Value};

/// Reads multi-document manifests and returns every ExternalSecret as
/// `<namespace>/<name>`, in file and document order.
pub fn external_secret_names(paths: &[String]) -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    for path in paths {
        let raw = fs::read_to_string(path).map_err(|error| read_error(path, &error))?;
        let documents = yaml::parse_documents(&raw).map_err(|error| format!("{path}: {error}"))?;
        for document in &documents {
            if document.get("kind").map(Value::as_str) != Some("ExternalSecret") {
                continue;
            }
            let Some(metadata) = document.get("metadata") else {
                return Err(format!("{path}: ExternalSecret without metadata"));
            };
            if !metadata.is_mapping() {
                return Err(format!("{path}: ExternalSecret without metadata"));
            }
            let namespace = metadata.get("namespace").map_or("", Value::as_str);
            let name = metadata.get("name").map_or("", Value::as_str);
            if namespace.is_empty() || name.is_empty() {
                return Err(format!(
                    "{path}: ExternalSecret needs metadata.namespace and metadata.name"
                ));
            }
            names.push(format!("{namespace}/{name}"));
        }
    }
    Ok(names)
}

/// Writes one name per line to `path` (mode 0644).
pub fn write_names(path: &str, names: &[String]) -> Result<(), String> {
    let mut output = names.join("\n");
    output.push('\n');
    fs::write(path, output).map_err(|error| write_error(path, &error))
}

/// Builds a `type: Opaque` Secret JSON document with one key whose value is
/// read from a dotenv-style file.
///
/// `encoding/json` sorts map keys and HTML-escapes `<`, `>` and `&`, so the
/// document is assembled field by field in that order instead of using a map.
pub fn opaque_secret(
    name: &str,
    namespace: &str,
    secret_key: &str,
    env_file: &str,
    env_key: &str,
) -> Result<Vec<u8>, String> {
    let value = envfile::lookup(path(env_file), env_key)?;
    if value.is_empty() {
        return Err(format!("{env_key} is missing in {env_file}"));
    }
    let encoded = base64::engine::general_purpose::STANDARD.encode(value.as_bytes());
    let document = format!(
        "{{\"apiVersion\":\"v1\",\"data\":{{{}}},\"kind\":\"Secret\",\"metadata\":{{\"name\":{},\"namespace\":{}}},\"type\":\"Opaque\"}}",
        json_member(secret_key, &encoded),
        json_string(name),
        json_string(namespace),
    );
    Ok(document.into_bytes())
}

fn path(value: &str) -> &Path {
    Path::new(value)
}

fn json_member(key: &str, value: &str) -> String {
    format!("{}:{}", json_string(key), json_string(value))
}

/// Encodes a string exactly like Go's `encoding/json` (HTML-safe escaping).
fn json_string(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 2);
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            '<' => output.push_str("\\u003c"),
            '>' => output.push_str("\\u003e"),
            '&' => output.push_str("\\u0026"),
            '\u{2028}' => output.push_str("\\u2028"),
            '\u{2029}' => output.push_str("\\u2029"),
            character if character < '\u{20}' => {
                let _ = write!(output, "\\u{:04x}", u32::from(character));
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

fn read_error(path: &str, error: &std::io::Error) -> String {
    format!("open {path}: {}", reason(error))
}

fn write_error(path: &str, error: &std::io::Error) -> String {
    format!("open {path}: {}", reason(error))
}

fn reason(error: &std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::NotFound => "no such file or directory".to_string(),
        std::io::ErrorKind::PermissionDenied => "permission denied".to_string(),
        _ => error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const EXTERNAL_SECRETS: &str = r"apiVersion: external-secrets.io/v1beta1
kind: ExternalSecret
metadata:
  name: hermes-environment
  namespace: home-assistant
---
apiVersion: v1
kind: ConfigMap
metadata:
  name: ignored
  namespace: home-assistant
---
apiVersion: external-secrets.io/v1beta1
kind: ExternalSecret
metadata:
  name: scheduler-auth
  namespace: home-assistant
";

    fn write(contents: &str) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().expect("temp file");
        file.write_all(contents.as_bytes()).expect("write");
        file
    }

    #[test]
    fn external_secret_names_keep_document_order() {
        let file = write(EXTERNAL_SECRETS);
        let names = external_secret_names(&[file.path().display().to_string()]).expect("names");
        assert_eq!(
            names.join(","),
            "home-assistant/hermes-environment,home-assistant/scheduler-auth"
        );
    }

    #[test]
    fn external_secret_names_reject_unnamed_secrets() {
        let file = write("kind: ExternalSecret\nmetadata:\n  namespace: home-assistant\n");
        let path = file.path().display().to_string();
        let error =
            external_secret_names(&[path]).expect_err("a nameless ExternalSecret must fail");
        assert!(error.contains("metadata.namespace and metadata.name"));
    }

    #[test]
    fn opaque_secret_reads_a_quoted_value() {
        let file = write("# comment\nCLOUDFLARE_API_TOKEN=\"token-value\"\n");
        let document = opaque_secret(
            "cloudflare-api-token",
            "cert-manager",
            "api-token",
            &file.path().display().to_string(),
            "CLOUDFLARE_API_TOKEN",
        )
        .expect("secret");
        let text = String::from_utf8(document).expect("utf8");
        assert_eq!(
            text,
            "{\"apiVersion\":\"v1\",\"data\":{\"api-token\":\"dG9rZW4tdmFsdWU=\"},\"kind\":\"Secret\",\"metadata\":{\"name\":\"cloudflare-api-token\",\"namespace\":\"cert-manager\"},\"type\":\"Opaque\"}"
        );
    }

    #[test]
    fn opaque_secret_fails_closed_without_a_value() {
        let file = write("OTHER=1\n");
        let error = opaque_secret(
            "name",
            "ns",
            "key",
            &file.path().display().to_string(),
            "CLOUDFLARE_API_TOKEN",
        )
        .expect_err("a missing token must fail");
        assert!(error.contains("CLOUDFLARE_API_TOKEN is missing in"));
    }
}
