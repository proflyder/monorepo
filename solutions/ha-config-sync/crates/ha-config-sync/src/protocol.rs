//! Wire protocol shared with `agent/ha-config-agent.sh`.
//!
//! The agent is a forced command on the Home Assistant OS host. The controller
//! and the agent agree on one protocol version, one path allowlist and the
//! `hashes`/`apply`/`restore`/`prune` verbs. Any change here must change the
//! script too; [`tests`] asserts that the protocol constant is present in the
//! shell source.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};

/// The protocol both sides speak. Bump when a verb or a line format changes.
pub const PROTOCOL_VERSION: u32 = 1;

/// First token of the agent `version` answer.
pub const AGENT_NAME: &str = "ha-config-agent";

/// The only relative paths that may be delivered.
///
/// Root-level `*.yaml` and `custom_sentences/<lang>/*.yaml` only. A path is
/// always relative to the Home Assistant config directory and to
/// `DESIRED_DIR`.
pub const MANAGED_PATH_PATTERN: &str = r"^(custom_sentences/[a-z]{2}/)?[a-z0-9_]+\.yaml$";

/// Files that must never be written, even if the pattern would allow them.
pub const DENIED_PATHS: [&str; 3] = ["configuration.yaml", "secrets.yaml", "scenes.yaml"];

/// Why a path or an agent answer was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProtocolError {
    /// A path escapes the allowlist or names a denied file.
    #[error("path is not managed: {0}")]
    Path(String),
    /// The agent answered with an unknown or malformed line.
    #[error("invalid agent answer: {0}")]
    Answer(String),
    /// The agent speaks a different protocol version.
    #[error("agent protocol {found}, expected {expected}")]
    Version { found: String, expected: u32 },
}

/// Whether `path` is a relative managed Home Assistant YAML path.
#[must_use]
pub fn is_managed_path(path: &str) -> bool {
    if DENIED_PATHS.contains(&path) {
        return false;
    }
    managed_path_regex().is_match(path)
}

fn managed_path_regex() -> &'static regex::Regex {
    static REGEX: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    REGEX.get_or_init(|| {
        regex::Regex::new(MANAGED_PATH_PATTERN).expect("managed path regex is valid")
    })
}

/// Whether `id` is a timestamp backup id produced by the agent.
///
/// `YYYYMMDDTHHMMSSZ` with an optional `-N` suffix used when two applies land in
/// the same second. No slash, dot or whitespace can appear in an id.
#[must_use]
pub fn is_backup_id(id: &str) -> bool {
    let (stamp, suffix) = match id.split_once('-') {
        Some((stamp, suffix)) => (stamp, Some(suffix)),
        None => (id, None),
    };
    if !is_stamp(stamp) {
        return false;
    }
    suffix
        .is_none_or(|suffix| !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit()))
}

fn is_stamp(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 16
        && bytes[8] == b'T'
        && bytes[15] == b'Z'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 8 || index == 15 || byte.is_ascii_digit())
}

/// Parse the `version` answer: `ha-config-agent <n>`.
pub fn parse_version(stdout: &[u8]) -> Result<u32, ProtocolError> {
    let text = std::str::from_utf8(stdout)
        .map_err(|_| ProtocolError::Answer("version is not UTF-8".to_owned()))?;
    let mut parts = text.split_whitespace();
    if parts.next() != Some(AGENT_NAME) {
        return Err(ProtocolError::Answer("not ha-config-agent".to_owned()));
    }
    let version = parts
        .next()
        .ok_or_else(|| ProtocolError::Answer("missing version".to_owned()))?;
    version.parse().map_err(|_| ProtocolError::Version {
        found: version.to_owned(),
        expected: PROTOCOL_VERSION,
    })
}

/// One managed file as the controller wants it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesiredFile {
    /// Lowercase hex SHA-256 of [`Self::content`].
    pub sha256: String,
    /// Exact bytes to deliver.
    pub content: Vec<u8>,
}

/// Why a desired directory could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum DesiredError {
    /// A directory could not be read.
    #[error("read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// A relative path is outside the managed allowlist.
    #[error("desired file is not a managed path: {0}")]
    Unmanaged(String),
    /// The directory contains no managed files.
    #[error("desired directory {0} contains no managed YAML files")]
    Empty(PathBuf),
}

/// Load every managed file under `directory`.
///
/// Only regular files are accepted. A symlink, an unexpected directory entry or
/// a file outside [`is_managed_path`] is an error: the image must bake exactly
/// the files this service may write.
pub fn load_desired(directory: &Path) -> Result<BTreeMap<String, DesiredFile>, DesiredError> {
    let mut files = BTreeMap::new();
    walk(directory, "", &mut files)?;
    if files.is_empty() {
        return Err(DesiredError::Empty(directory.to_path_buf()));
    }
    Ok(files)
}

fn walk(
    root: &Path,
    relative: &str,
    files: &mut BTreeMap<String, DesiredFile>,
) -> Result<(), DesiredError> {
    let directory = if relative.is_empty() {
        root.to_path_buf()
    } else {
        root.join(relative)
    };
    let entries = std::fs::read_dir(&directory).map_err(|source| DesiredError::Read {
        path: directory.clone(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| DesiredError::Read {
            path: directory.clone(),
            source,
        })?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let child = if relative.is_empty() {
            name.clone()
        } else {
            format!("{relative}/{name}")
        };
        let metadata =
            std::fs::symlink_metadata(entry.path()).map_err(|source| DesiredError::Read {
                path: entry.path(),
                source,
            })?;
        if metadata.file_type().is_symlink() {
            return Err(DesiredError::Unmanaged(child));
        }
        if metadata.is_dir() {
            walk(root, &child, files)?;
            continue;
        }
        if !metadata.is_file() || !is_managed_path(&child) {
            return Err(DesiredError::Unmanaged(child));
        }
        let content = std::fs::read(entry.path()).map_err(|source| DesiredError::Read {
            path: entry.path(),
            source,
        })?;
        files.insert(
            child,
            DesiredFile {
                sha256: sha256_hex(&content),
                content,
            },
        );
    }
    Ok(())
}

/// Lowercase hex SHA-256 of `bytes`.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Canonical digest of a desired set, used for metrics and retry backoff.
///
/// The digest covers sorted `path`/`sha256` pairs, so changing one byte or one
/// path changes it and a new desired state resets the failure backoff.
#[must_use]
pub fn desired_digest(files: &BTreeMap<String, DesiredFile>) -> String {
    let mut hasher = Sha256::new();
    for (path, file) in files {
        hasher.update(path.as_bytes());
        hasher.update(b"\0");
        hasher.update(file.sha256.as_bytes());
        hasher.update(b"\n");
    }
    hex::encode(hasher.finalize())
}

/// One path/hash pair read from the agent `hashes` answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentHash {
    /// The file exists with this SHA-256.
    Present(String),
    /// The file does not exist on the host.
    Missing,
}

/// Parse the `hashes` answer: one `<sha256|MISSING> <path>` line per file.
pub fn parse_hashes(
    stdout: &[u8],
    expected: &[String],
) -> Result<BTreeMap<String, AgentHash>, ProtocolError> {
    let text = std::str::from_utf8(stdout)
        .map_err(|_| ProtocolError::Answer("hashes are not UTF-8".to_owned()))?;
    let mut hashes = BTreeMap::new();
    for line in text.lines().filter(|line| !line.is_empty()) {
        let (hash, path) = line.split_once(' ').ok_or_else(|| {
            ProtocolError::Answer(format!("malformed hash line: {}", truncate(line, 64)))
        })?;
        if !is_managed_path(path) {
            return Err(ProtocolError::Path(path.to_owned()));
        }
        let value = if hash == "MISSING" {
            AgentHash::Missing
        } else if is_sha256(hash) {
            AgentHash::Present(hash.to_owned())
        } else {
            return Err(ProtocolError::Answer(format!(
                "invalid hash for {path}: {}",
                truncate(hash, 16)
            )));
        };
        if hashes.insert(path.to_owned(), value).is_some() {
            return Err(ProtocolError::Answer(format!("duplicate hash for {path}")));
        }
    }
    for path in expected {
        if !hashes.contains_key(path) {
            return Err(ProtocolError::Answer(format!("missing hash for {path}")));
        }
    }
    Ok(hashes)
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn truncate(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

impl fmt::Display for AgentHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Present(hash) => formatter.write_str(hash),
            Self::Missing => formatter.write_str("MISSING"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AGENT_SCRIPT: &str = include_str!("../../../agent/ha-config-agent.sh");

    #[test]
    fn the_agent_speaks_the_same_protocol_version() {
        let expected = format!("PROTOCOL_VERSION={PROTOCOL_VERSION}");
        assert!(
            AGENT_SCRIPT.contains(&expected),
            "agent script must contain {expected}"
        );
    }

    #[test]
    fn the_agent_uses_the_same_allowlist() {
        assert!(AGENT_SCRIPT.contains(MANAGED_PATH_PATTERN));
        for denied in DENIED_PATHS {
            assert!(AGENT_SCRIPT.contains(denied), "agent must deny {denied}");
        }
    }

    #[test]
    fn managed_paths_follow_the_allowlist() {
        assert!(is_managed_path("scripts.yaml"));
        assert!(is_managed_path("shell_commands.yaml"));
        assert!(is_managed_path("custom_sentences/ru/station_alarm.yaml"));
        assert!(!is_managed_path("configuration.yaml"));
        assert!(!is_managed_path("secrets.yaml"));
        assert!(!is_managed_path("scenes.yaml"));
        assert!(!is_managed_path("../scripts.yaml"));
        assert!(!is_managed_path("/etc/passwd"));
        assert!(!is_managed_path("custom_sentences/ru/sub/scripts.yaml"));
        assert!(!is_managed_path("custom_sentences/RU/scripts.yaml"));
        assert!(!is_managed_path("README.md"));
        assert!(!is_managed_path("scripts.yml"));
        assert!(!is_managed_path("scripts.yaml\n"));
    }

    #[test]
    fn version_answers_are_parsed_strictly() {
        assert_eq!(parse_version(b"ha-config-agent 1\n").expect("version"), 1);
        assert_eq!(parse_version(b"ha-config-agent 2\n").expect("version"), 2);
        assert!(parse_version(b"other 1\n").is_err());
        assert!(parse_version(b"ha-config-agent\n").is_err());
    }

    #[test]
    fn hashes_are_parsed_and_ordered() {
        let sha = "a".repeat(64);
        let expected = vec!["scripts.yaml".to_owned(), "shell_commands.yaml".to_owned()];
        let map = parse_hashes(
            format!("{sha} scripts.yaml\nMISSING shell_commands.yaml\n").as_bytes(),
            &expected,
        )
        .expect("hashes");
        assert_eq!(map.get("scripts.yaml"), Some(&AgentHash::Present(sha)));
        assert_eq!(map.get("shell_commands.yaml"), Some(&AgentHash::Missing));
    }

    #[test]
    fn hashes_reject_unknown_paths_and_short_digests() {
        let sha = "a".repeat(64);
        let expected = vec!["scripts.yaml".to_owned()];
        assert!(parse_hashes(format!("{sha} ../secrets.yaml\n").as_bytes(), &expected).is_err());
        assert!(parse_hashes(b"abc scripts.yaml\n", &expected).is_err());
        assert!(parse_hashes(b"", &expected).is_err());
    }

    #[test]
    fn desired_digest_tracks_paths_and_contents() {
        let mut files = BTreeMap::new();
        files.insert(
            "scripts.yaml".to_owned(),
            DesiredFile {
                sha256: sha256_hex(b"a"),
                content: b"a".to_vec(),
            },
        );
        let first = desired_digest(&files);
        files.insert(
            "shell_commands.yaml".to_owned(),
            DesiredFile {
                sha256: sha256_hex(b"b"),
                content: b"b".to_vec(),
            },
        );
        assert_ne!(first, desired_digest(&files));
        let copy = files.clone();
        assert_eq!(desired_digest(&files), desired_digest(&copy));
    }

    #[test]
    fn load_desired_reads_only_managed_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("scripts.yaml"), b"a: 1\n").expect("write");
        std::fs::create_dir_all(dir.path().join("custom_sentences/ru")).expect("mkdir");
        std::fs::write(
            dir.path().join("custom_sentences/ru/station_alarm.yaml"),
            b"b: 2\n",
        )
        .expect("write");
        let files = load_desired(dir.path()).expect("desired");
        assert_eq!(
            files.keys().cloned().collect::<Vec<_>>(),
            vec![
                "custom_sentences/ru/station_alarm.yaml".to_owned(),
                "scripts.yaml".to_owned()
            ]
        );
        assert_eq!(files["scripts.yaml"].sha256, sha256_hex(b"a: 1\n"));
    }

    #[test]
    fn load_desired_rejects_extra_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("README.md"), b"x").expect("write");
        assert!(matches!(
            load_desired(dir.path()),
            Err(DesiredError::Unmanaged(path)) if path == "README.md"
        ));
    }

    #[cfg(unix)]
    #[test]
    fn load_desired_rejects_symlinks() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("target.yaml"), b"a: 1\n").expect("write");
        symlink(
            dir.path().join("target.yaml"),
            dir.path().join("scripts.yaml"),
        )
        .expect("symlink");
        assert!(matches!(
            load_desired(dir.path()),
            Err(DesiredError::Unmanaged(path)) if path == "scripts.yaml"
        ));
    }
}
