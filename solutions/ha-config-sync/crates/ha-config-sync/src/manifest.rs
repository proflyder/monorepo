//! The project-owned manifest: which desired globs are delivered and what Home
//! Assistant must reload (or restart) after they change.
//!
//! ```yaml
//! files:
//!   - glob: "scripts.yaml"
//!     reload: ["script.reload"]
//!   - glob: "custom_sentences/ru/*.yaml"
//!     reload: ["conversation.reload"]
//!   - glob: "shell_commands.yaml"
//!     restart: true
//! ```
//!
//! Every desired file must be covered by exactly one entry; overlapping globs,
//! an empty manifest, an unmanaged path, a denied file and an empty match are
//! errors.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Deserialize;

use crate::protocol::DesiredFile;

/// One manifest line after validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The glob exactly as written.
    pub glob: String,
    /// Desired paths this glob resolves to, sorted.
    pub paths: Vec<String>,
    /// Services called after a drifted file from this entry changes.
    pub reload: Vec<String>,
    /// Whether a change needs a Home Assistant restart after the reloads.
    pub restart: bool,
}

/// A validated manifest bound to one desired file set.
#[derive(Debug, Clone)]
pub struct Plan {
    entries: Vec<Entry>,
    by_path: BTreeMap<String, usize>,
}

/// A manifest validation failure.
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    /// The file could not be read.
    #[error("read manifest {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    /// The YAML is malformed.
    #[error("parse manifest: {0}")]
    Parse(String),
    /// `files` is missing or empty.
    #[error("manifest contains no file entries")]
    Empty,
    /// A glob is malformed.
    #[error("invalid glob {glob}: {reason}")]
    InvalidGlob { glob: String, reason: String },
    /// A glob matched nothing in the desired directory.
    #[error("glob {0} matched no desired file")]
    EmptyGlob(String),
    /// No entry covers a desired file.
    #[error("desired file {0} is not covered by the manifest")]
    Uncovered(String),
    /// More than one entry covers a desired file.
    #[error("desired file {0} is covered by more than one manifest entry")]
    Overlapping(String),
    /// A reload service is not `<domain>.<service>`.
    #[error("invalid reload service {0}")]
    InvalidReload(String),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
    files: Vec<RawEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEntry {
    glob: String,
    #[serde(default)]
    reload: Vec<String>,
    #[serde(default)]
    restart: bool,
}

impl Plan {
    /// Read, parse and validate a manifest against the desired files.
    pub fn load(
        path: &Path,
        desired: &BTreeMap<String, DesiredFile>,
    ) -> Result<Self, ManifestError> {
        let content = std::fs::read_to_string(path).map_err(|source| ManifestError::Read {
            path: path.display().to_string(),
            source,
        })?;
        Self::parse(&content, desired)
    }

    /// Validate manifest YAML against the desired files.
    pub fn parse(
        content: &str,
        desired: &BTreeMap<String, DesiredFile>,
    ) -> Result<Self, ManifestError> {
        let raw: RawManifest = serde_saphyr::from_str(content)
            .map_err(|error| ManifestError::Parse(error.to_string()))?;
        if raw.files.is_empty() {
            return Err(ManifestError::Empty);
        }
        let mut entries = Vec::with_capacity(raw.files.len());
        let mut by_path: BTreeMap<String, usize> = BTreeMap::new();
        for (index, raw_entry) in raw.files.into_iter().enumerate() {
            validate_glob(&raw_entry.glob)?;
            for service in &raw_entry.reload {
                validate_reload(service)?;
            }
            let paths: Vec<String> = desired
                .keys()
                .filter(|path| glob_matches(&raw_entry.glob, path))
                .cloned()
                .collect();
            if paths.is_empty() {
                return Err(ManifestError::EmptyGlob(raw_entry.glob));
            }
            for path in &paths {
                if by_path.insert(path.clone(), index).is_some() {
                    return Err(ManifestError::Overlapping(path.clone()));
                }
            }
            entries.push(Entry {
                glob: raw_entry.glob,
                paths,
                reload: raw_entry.reload,
                restart: raw_entry.restart,
            });
        }
        for path in desired.keys() {
            if !by_path.contains_key(path) {
                return Err(ManifestError::Uncovered(path.clone()));
            }
        }
        Ok(Self { entries, by_path })
    }

    /// Every entry, in manifest order.
    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The entry covering `path`.
    #[must_use]
    pub fn entry_for(&self, path: &str) -> Option<&Entry> {
        self.by_path.get(path).map(|index| &self.entries[*index])
    }

    /// Distinct reload services for a set of changed paths, in manifest order.
    ///
    /// A service needed by several files is called once; ordering is stable so
    /// a log and a rollback repeat the same plan.
    #[must_use]
    pub fn reload_services(&self, changed: &BTreeSet<String>) -> Vec<String> {
        let mut services = Vec::new();
        for entry in &self.entries {
            if !entry.paths.iter().any(|path| changed.contains(path)) {
                continue;
            }
            for service in &entry.reload {
                if !services.contains(service) {
                    services.push(service.clone());
                }
            }
        }
        services
    }

    /// Whether any changed file requires a restart.
    #[must_use]
    pub fn needs_restart(&self, changed: &BTreeSet<String>) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.restart && entry.paths.iter().any(|path| changed.contains(path)))
    }
}

fn validate_glob(glob: &str) -> Result<(), ManifestError> {
    let invalid = |reason: &str| ManifestError::InvalidGlob {
        glob: glob.to_owned(),
        reason: reason.to_owned(),
    };
    if glob.is_empty() {
        return Err(invalid("empty"));
    }
    if glob.starts_with('/') || glob.contains("..") || glob.contains('\\') {
        return Err(invalid("must be a relative glob without .."));
    }
    if glob.chars().any(|character| {
        !(character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || matches!(character, '_' | '.' | '/' | '*' | '?'))
    }) {
        return Err(invalid("unexpected character"));
    }
    Ok(())
}

fn validate_reload(service: &str) -> Result<(), ManifestError> {
    let valid = service.split_once('.').is_some_and(|(domain, action)| {
        !domain.is_empty()
            && !action.is_empty()
            && domain.chars().all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
            })
            && action.chars().all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
            })
    });
    if valid {
        Ok(())
    } else {
        Err(ManifestError::InvalidReload(service.to_owned()))
    }
}

/// Segment-aware glob: `*` and `?` never cross `/`.
fn glob_matches(glob: &str, path: &str) -> bool {
    let glob_segments: Vec<&str> = glob.split('/').collect();
    let path_segments: Vec<&str> = path.split('/').collect();
    glob_segments.len() == path_segments.len()
        && glob_segments
            .iter()
            .zip(&path_segments)
            .all(|(glob, path)| segment_matches(glob, path))
}

fn segment_matches(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    segment_matches_chars(&pattern, &text)
}

fn segment_matches_chars(pattern: &[char], text: &[char]) -> bool {
    if pattern.is_empty() {
        return text.is_empty();
    }
    if text.is_empty() {
        return pattern.iter().all(|character| *character == '*');
    }
    match pattern[0] {
        '*' => {
            if pattern.len() == 1 {
                return true;
            }
            (0..=text.len()).any(|index| segment_matches_chars(&pattern[1..], &text[index..]))
        }
        '?' => segment_matches_chars(&pattern[1..], &text[1..]),
        character => character == text[0] && segment_matches_chars(&pattern[1..], &text[1..]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::sha256_hex;

    fn desired(paths: &[&str]) -> BTreeMap<String, DesiredFile> {
        paths
            .iter()
            .map(|path| {
                let content = path.as_bytes().to_vec();
                (
                    (*path).to_owned(),
                    DesiredFile {
                        sha256: sha256_hex(&content),
                        content,
                    },
                )
            })
            .collect()
    }

    const MANIFEST: &str = r#"
files:
  - glob: "scripts.yaml"
    reload: ["script.reload"]
  - glob: "custom_sentences/ru/*.yaml"
    reload: ["conversation.reload"]
  - glob: "shell_commands.yaml"
    restart: true
"#;

    #[test]
    fn a_manifest_binds_every_desired_file() {
        let desired = desired(&[
            "custom_sentences/ru/station_alarm.yaml",
            "shell_commands.yaml",
            "scripts.yaml",
        ]);
        let plan = Plan::parse(MANIFEST, &desired).expect("plan");
        assert_eq!(plan.entries().len(), 3);
        assert_eq!(
            plan.entries()[1].paths,
            vec!["custom_sentences/ru/station_alarm.yaml".to_owned()]
        );
        assert!(plan.needs_restart(&BTreeSet::from(["shell_commands.yaml".to_owned()])));
        assert!(!plan.needs_restart(&BTreeSet::from(["scripts.yaml".to_owned()])));
    }

    #[test]
    fn reload_services_are_distinct_and_ordered() {
        let desired = desired(&["scripts.yaml", "automations.yaml", "shell_commands.yaml"]);
        let plan = Plan::parse(
            r#"
files:
  - glob: "scripts.yaml"
    reload: ["script.reload", "automation.reload"]
  - glob: "automations.yaml"
    reload: ["automation.reload"]
  - glob: "shell_commands.yaml"
    restart: true
"#,
            &desired,
        )
        .expect("plan");
        let changed = BTreeSet::from(["automations.yaml".to_owned(), "scripts.yaml".to_owned()]);
        assert_eq!(
            plan.reload_services(&changed),
            vec!["script.reload".to_owned(), "automation.reload".to_owned()]
        );
    }

    #[test]
    fn an_empty_manifest_is_rejected() {
        assert!(matches!(
            Plan::parse("files: []\n", &desired(&["scripts.yaml"])),
            Err(ManifestError::Empty)
        ));
    }

    #[test]
    fn an_uncovered_or_overlapping_file_is_rejected() {
        let uncovered = Plan::parse(
            r#"
files:
  - glob: "scripts.yaml"
"#,
            &desired(&["scripts.yaml", "shell_commands.yaml"]),
        );
        assert!(
            matches!(uncovered, Err(ManifestError::Uncovered(path)) if path == "shell_commands.yaml")
        );

        let overlapping = Plan::parse(
            r#"
files:
  - glob: "*.yaml"
  - glob: "scripts.yaml"
"#,
            &desired(&["scripts.yaml"]),
        );
        assert!(
            matches!(overlapping, Err(ManifestError::Overlapping(path)) if path == "scripts.yaml")
        );
    }

    #[test]
    fn denied_and_escaping_globs_are_rejected() {
        for glob in [
            "../scripts.yaml",
            "/etc/passwd",
            "configuration.yaml",
            "custom_sentences/ru/../x.yaml",
        ] {
            let result = Plan::parse(
                &format!("files:\n  - glob: \"{glob}\"\n"),
                &desired(&["scripts.yaml"]),
            );
            assert!(result.is_err(), "{glob} must be rejected");
        }
    }

    #[test]
    fn a_denied_file_can_never_be_desired() {
        // `load_desired` already refuses it; a manifest can only see managed
        // paths, so a glob for a denied path matches nothing.
        let result = Plan::parse(
            "files:\n  - glob: \"secrets.yaml\"\n",
            &desired(&["scripts.yaml"]),
        );
        assert!(matches!(result, Err(ManifestError::EmptyGlob(_))));
    }

    #[test]
    fn invalid_reload_services_are_rejected() {
        let result = Plan::parse(
            "files:\n  - glob: \"scripts.yaml\"\n    reload: [\"reload\"]\n",
            &desired(&["scripts.yaml"]),
        );
        assert!(matches!(result, Err(ManifestError::InvalidReload(_))));
    }

    #[test]
    fn globs_do_not_cross_segments() {
        assert!(glob_matches(
            "custom_sentences/ru/*.yaml",
            "custom_sentences/ru/a.yaml"
        ));
        assert!(!glob_matches(
            "custom_sentences/*/a.yaml",
            "custom_sentences/ru/sub/a.yaml"
        ));
        assert!(!glob_matches("*.yaml", "custom_sentences/ru/a.yaml"));
        assert!(glob_matches("station_*.yaml", "station_alarm.yaml"));
    }
}
