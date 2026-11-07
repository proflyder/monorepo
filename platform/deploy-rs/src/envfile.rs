//! Private `KEY=VALUE` files.
//!
//! Two readers are kept because the deploy scripts had two: the strict parser
//! used by `render` (a malformed line or a duplicate key is an error) and the
//! permissive lookup used by `env get` (the first match wins, malformed lines
//! are skipped). Both are byte-compatible with the Go implementation in
//! `platform/deploy/internal/envfile`.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Parses `path` and returns every `KEY=VALUE` pair, in file order.
///
/// Errors name the file and 1-based line number:
///
/// ```text
/// <path>:<line>: expected KEY=VALUE
/// <path>:<line>: duplicate key <key>
/// ```
pub fn parse(path: &Path) -> Result<BTreeMap<String, String>, String> {
    let raw = read(path)?;
    let mut values = BTreeMap::new();
    for (index, raw_line) in raw.split(|byte| *byte == b'\n').enumerate() {
        let line = String::from_utf8_lossy(raw_line);
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some(split) = line.find('=') else {
            return Err(format!(
                "{}:{}: expected KEY=VALUE",
                path.display(),
                index + 1
            ));
        };
        let key = normalize_key(&line[..split]);
        if values.contains_key(&key) {
            return Err(format!(
                "{}:{}: duplicate key {}",
                path.display(),
                index + 1,
                key
            ));
        }
        values.insert(key, unquote(&line[split + 1..]));
    }
    Ok(values)
}

/// Returns the value of `key`, or an empty string when absent.
///
/// Mirrors the shell helper: the first matching line wins, malformed lines are
/// ignored and a duplicate key is not an error.
pub fn lookup(path: &Path, key: &str) -> Result<String, String> {
    let raw = read(path)?;
    for raw_line in raw.split(|byte| *byte == b'\n') {
        let line = String::from_utf8_lossy(raw_line);
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some(split) = line.find('=') else {
            continue;
        };
        if normalize_key(&line[..split]) != key {
            continue;
        }
        return Ok(unquote(&line[split + 1..]));
    }
    Ok(String::new())
}

/// Reads the file with the Go `os.ReadFile` error shape (`open PATH: reason`).
fn read(path: &Path) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|error| {
        let reason = match error.kind() {
            std::io::ErrorKind::NotFound => "no such file or directory".to_string(),
            std::io::ErrorKind::PermissionDenied => "permission denied".to_string(),
            _ => error.to_string(),
        };
        format!("open {}: {reason}", path.display())
    })
}

/// Trims whitespace and an optional `export ` prefix.
fn normalize_key(prefix: &str) -> String {
    let key = prefix.trim();
    key.strip_prefix("export ")
        .map_or_else(|| key.to_string(), |rest| rest.trim().to_string())
}

/// Trims the value and removes every leading/trailing `"` and then `'`.
///
/// It reproduces the Go/Python helper exactly, including nested quotes.
fn unquote(value: &str) -> String {
    let value = value.trim();
    let value = value.trim_matches('"');
    value.trim_matches('\'').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(contents: &str) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().expect("temp file");
        file.write_all(contents.as_bytes()).expect("write");
        file
    }

    #[test]
    fn parse_and_lookup_share_the_format() {
        let file = write(
            "# comment\n\nKEY=value\nQUOTED=\"value with spaces\"\nexport EXPORTED=yes\nEMPTY=\n",
        );
        let values = parse(file.path()).expect("parse");
        assert_eq!(values.get("KEY").map(String::as_str), Some("value"));
        assert_eq!(
            values.get("QUOTED").map(String::as_str),
            Some("value with spaces")
        );
        assert_eq!(values.get("EXPORTED").map(String::as_str), Some("yes"));
        assert_eq!(values.get("EMPTY").map(String::as_str), Some(""));
        assert_eq!(lookup(file.path(), "KEY"), Ok("value".to_string()));
        assert_eq!(lookup(file.path(), "MISSING"), Ok(String::new()));
    }

    #[test]
    fn duplicate_and_malformed_lines_fail_the_strict_reader() {
        let duplicate = write("KEY=a\nKEY=b\n");
        assert!(
            parse(duplicate.path())
                .unwrap_err()
                .contains("duplicate key KEY")
        );
        let malformed = write("KEY\n");
        assert!(
            parse(malformed.path())
                .unwrap_err()
                .contains("expected KEY=VALUE")
        );
    }

    #[test]
    fn lookup_keeps_the_first_match_and_skips_malformed_lines() {
        let file = write("noise\nKEY=first\nKEY=second\n");
        assert_eq!(lookup(file.path(), "KEY"), Ok("first".to_string()));
    }

    #[test]
    fn unquote_matches_the_go_helper() {
        assert_eq!(unquote("  \"value\"  "), "value");
        assert_eq!(unquote("\"'\""), "");
        assert_eq!(unquote("\"  spaced  \""), "  spaced  ");
    }
}
