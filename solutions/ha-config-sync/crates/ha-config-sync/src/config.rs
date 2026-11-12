//! Environment configuration.
//!
//! Every deployment-specific value (URL, token file, SSH target, paths) comes
//! from the process environment; the solution itself has no project defaults.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use home_secret::Token;

/// Default period between sync cycles.
pub const DEFAULT_SYNC_INTERVAL: Duration = Duration::from_secs(300);
/// Default Prometheus listen address.
pub const DEFAULT_METRICS_ADDR: &str = "0.0.0.0:9119";
/// How many backups the agent keeps after a successful cycle.
pub const DEFAULT_PRUNE_KEEP: u32 = 10;

/// What a cycle may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Deliver changes and roll them back on failure.
    Apply,
    /// Only measure and report drift.
    Observe,
}

impl Mode {
    /// The metric label value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Apply => "apply",
            Self::Observe => "observe",
        }
    }
}

/// SSH parameters for the forced-command agent.
#[derive(Debug, Clone)]
pub struct SshSettings {
    /// `host:port` of the Home Assistant OS host.
    pub address: String,
    /// The user whose key is restricted to the agent.
    pub user: String,
    /// The unencrypted private key.
    pub key_file: PathBuf,
    /// Pinned host keys for `address`.
    pub known_hosts_file: PathBuf,
}

/// Validated process configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Home Assistant base URL, without a path.
    pub ha_url: String,
    /// Long-lived Home Assistant bearer token.
    pub ha_token: Token,
    /// SSH transport settings.
    pub ssh: SshSettings,
    /// Directory holding the exact files to deliver.
    pub desired_dir: PathBuf,
    /// YAML manifest binding globs to reload actions.
    pub manifest_file: PathBuf,
    /// Period between cycles.
    pub sync_interval: Duration,
    /// Apply or observe.
    pub mode: Mode,
    /// Prometheus/health listen address.
    pub metrics_addr: String,
    /// Optional entity that must be `off` before a restart.
    pub restart_guard_entity: Option<String>,
}

/// A configuration error; the message never contains the token.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    /// A required variable is absent or empty.
    #[error("missing required environment variable {0}")]
    Missing(&'static str),
    /// A variable has an unsupported value.
    #[error("invalid environment variable {name}: {reason}")]
    Invalid {
        /// Variable name.
        name: &'static str,
        /// Why the value was rejected.
        reason: String,
    },
    /// The token file could not be read.
    #[error("read HA_TOKEN_FILE: {0}")]
    TokenFile(String),
}

/// Read configuration from the process environment.
pub fn from_env() -> Result<Config, ConfigError> {
    let pairs: BTreeMap<String, String> = std::env::vars().collect();
    from_map(&pairs)
}

/// Validate configuration from a map. Used by tests and by [`from_env`].
pub fn from_map(pairs: &BTreeMap<String, String>) -> Result<Config, ConfigError> {
    let required = |name: &'static str| -> Result<String, ConfigError> {
        pairs
            .get(name)
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .ok_or(ConfigError::Missing(name))
    };
    let optional = |name: &str| -> Option<String> {
        pairs
            .get(name)
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    };

    let ha_url = required("HA_URL")?;
    if !(ha_url.starts_with("http://") || ha_url.starts_with("https://")) {
        return Err(ConfigError::Invalid {
            name: "HA_URL",
            reason: "must start with http:// or https://".to_owned(),
        });
    }
    let token_file = required("HA_TOKEN_FILE")?;
    let token = std::fs::read_to_string(&token_file)
        .map_err(|error| ConfigError::TokenFile(error.to_string()))?;
    let token = token.trim();
    if token.is_empty() {
        return Err(ConfigError::Invalid {
            name: "HA_TOKEN_FILE",
            reason: "file is empty".to_owned(),
        });
    }
    let token = Token::new(token);

    let ssh = SshSettings {
        address: required("SSH_ADDR")?,
        user: required("SSH_USER")?,
        key_file: PathBuf::from(required("SSH_KEY_FILE")?),
        known_hosts_file: PathBuf::from(required("SSH_KNOWN_HOSTS_FILE")?),
    };

    let mode = match optional("SYNC_MODE").as_deref().unwrap_or("apply") {
        "apply" => Mode::Apply,
        "observe" => Mode::Observe,
        other => {
            return Err(ConfigError::Invalid {
                name: "SYNC_MODE",
                reason: format!("unsupported mode {other}"),
            });
        }
    };

    let sync_interval = match optional("SYNC_INTERVAL") {
        None => DEFAULT_SYNC_INTERVAL,
        Some(value) => match value.parse::<u64>() {
            Ok(seconds) if seconds > 0 => Duration::from_secs(seconds),
            _ => {
                return Err(ConfigError::Invalid {
                    name: "SYNC_INTERVAL",
                    reason: "must be a positive number of seconds".to_owned(),
                });
            }
        },
    };

    let metrics_addr = optional("METRICS_ADDR").unwrap_or_else(|| DEFAULT_METRICS_ADDR.to_owned());
    let restart_guard_entity = optional("RESTART_GUARD_ENTITY");
    if let Some(entity) = restart_guard_entity.as_deref()
        && !valid_entity(entity)
    {
        return Err(ConfigError::Invalid {
            name: "RESTART_GUARD_ENTITY",
            reason: "must be a <domain>.<object_id> entity id".to_owned(),
        });
    }

    Ok(Config {
        ha_url,
        ha_token: token,
        ssh,
        desired_dir: PathBuf::from(required("DESIRED_DIR")?),
        manifest_file: PathBuf::from(required("MANIFEST_FILE")?),
        sync_interval,
        mode,
        metrics_addr,
        restart_guard_entity,
    })
}

fn valid_entity(entity: &str) -> bool {
    entity.split_once('.').is_some_and(|(domain, object)| {
        !domain.is_empty()
            && !object.is_empty()
            && domain
                .chars()
                .all(|character| character.is_ascii_lowercase() || character == '_')
            && object.chars().all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(dir: &std::path::Path) -> BTreeMap<String, String> {
        let token = dir.join("token");
        std::fs::write(&token, "secret-value\n").expect("write token");
        BTreeMap::from([
            ("HA_URL".to_owned(), "http://192.0.2.10:8123".to_owned()),
            ("HA_TOKEN_FILE".to_owned(), token.display().to_string()),
            ("SSH_ADDR".to_owned(), "192.0.2.10:22222".to_owned()),
            ("SSH_USER".to_owned(), "root".to_owned()),
            ("SSH_KEY_FILE".to_owned(), "/keys/id".to_owned()),
            (
                "SSH_KNOWN_HOSTS_FILE".to_owned(),
                "/keys/known_hosts".to_owned(),
            ),
            ("DESIRED_DIR".to_owned(), "/desired".to_owned()),
            ("MANIFEST_FILE".to_owned(), "/manifest.yaml".to_owned()),
        ])
    }

    #[test]
    fn defaults_apply() {
        let dir = tempfile::tempdir().expect("tmp");
        let config = from_map(&base(dir.path())).expect("config");
        assert_eq!(config.mode, Mode::Apply);
        assert_eq!(config.sync_interval, DEFAULT_SYNC_INTERVAL);
        assert_eq!(config.metrics_addr, DEFAULT_METRICS_ADDR);
        assert_eq!(config.restart_guard_entity, None);
        assert_eq!(config.ha_token.expose(), "secret-value");
    }

    #[test]
    fn observe_and_overrides_are_parsed() {
        let dir = tempfile::tempdir().expect("tmp");
        let mut pairs = base(dir.path());
        pairs.insert("SYNC_MODE".to_owned(), "observe".to_owned());
        pairs.insert("SYNC_INTERVAL".to_owned(), "60".to_owned());
        pairs.insert("METRICS_ADDR".to_owned(), "127.0.0.1:9999".to_owned());
        pairs.insert(
            "RESTART_GUARD_ENTITY".to_owned(),
            "input_boolean.alarm_ringing".to_owned(),
        );
        let config = from_map(&pairs).expect("config");
        assert_eq!(config.mode, Mode::Observe);
        assert_eq!(config.sync_interval, Duration::from_secs(60));
        assert_eq!(config.metrics_addr, "127.0.0.1:9999");
        assert_eq!(
            config.restart_guard_entity.as_deref(),
            Some("input_boolean.alarm_ringing")
        );
    }

    #[test]
    fn invalid_values_are_rejected_without_the_token() {
        let dir = tempfile::tempdir().expect("tmp");
        let mut pairs = base(dir.path());
        pairs.insert("SYNC_INTERVAL".to_owned(), "0".to_owned());
        let error = from_map(&pairs).expect_err("interval");
        assert!(!error.to_string().contains("secret-value"));

        let mut pairs = base(dir.path());
        pairs.insert("SYNC_MODE".to_owned(), "destroy".to_owned());
        assert!(from_map(&pairs).is_err());

        let mut pairs = base(dir.path());
        pairs.insert(
            "RESTART_GUARD_ENTITY".to_owned(),
            "not-an-entity".to_owned(),
        );
        assert!(from_map(&pairs).is_err());

        let mut pairs = base(dir.path());
        pairs.remove("SSH_ADDR");
        assert!(matches!(
            from_map(&pairs),
            Err(ConfigError::Missing("SSH_ADDR"))
        ));
    }
}
