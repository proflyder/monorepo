//! `ha-control` process configuration, loaded 1:1 from the Go `cmd/ha-control`.
//!
//! * All three tokens (`HA_TOKEN`, `SCHEDULER_TOKEN`, `CONTROL_TOKEN`) are
//!   required and `*_FILE` wins over the direct variable, exactly like the Go
//!   control loader (the scheduler service uses the opposite priority).
//! * Only the documented `CONTROL_*` durations are configurable; batch size,
//!   lease and attempt limits stay at the Go defaults.
//! * `CONTROL_LABEL` was removed and is rejected with the migration hint.
//! * Durations accept Go syntax (`1s`, `5m`, `1h30m`).
//!
//! The token value never appears in an error message.

use std::path::PathBuf;
use std::time::Duration;

use home_config::parse_go_duration;
use home_secret::Token;
use thiserror::Error;

use crate::catalog::Rules;
use crate::config::{base_url, bool_value, non_empty, token};

/// Default queue carrying control commands.
pub const DEFAULT_QUEUE: &str = "control";
const DEFAULT_LISTEN: &str = "127.0.0.1:8099";

/// A fully validated `ha-control` configuration.
#[derive(Clone)]
pub struct ControlConfig {
    pub ha_url: String,
    pub ha_token: Token,
    pub ha_ca_file: Option<PathBuf>,
    pub scheduler_url: String,
    pub scheduler_token: Token,
    pub scheduler_ca_file: Option<PathBuf>,
    pub scenario_label: String,
    pub profile_label: String,
    pub exclude: Vec<String>,
    pub cache_ttl: Duration,
    pub listen: String,
    pub control_token: Token,
    pub consumer_id: String,
    pub queue: String,
    pub execute: bool,
    pub batch_size: i32,
    pub lease_seconds: i32,
    pub max_attempts: i32,
    pub poll_interval: Duration,
    pub max_backoff: Duration,
    pub ha_timeout: Duration,
    pub scheduler_timeout: Duration,
}

impl std::fmt::Debug for ControlConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ControlConfig")
            .field("ha_url", &self.ha_url)
            .field("scheduler_url", &self.scheduler_url)
            .field("scenario_label", &self.scenario_label)
            .field("profile_label", &self.profile_label)
            .field("exclude", &self.exclude)
            .field("cache_ttl", &self.cache_ttl)
            .field("listen", &self.listen)
            .field("consumer_id", &self.consumer_id)
            .field("queue", &self.queue)
            .field("execute", &self.execute)
            .field("batch_size", &self.batch_size)
            .field("lease_seconds", &self.lease_seconds)
            .field("max_attempts", &self.max_attempts)
            .field("poll_interval", &self.poll_interval)
            .field("max_backoff", &self.max_backoff)
            .field("ha_timeout", &self.ha_timeout)
            .field("scheduler_timeout", &self.scheduler_timeout)
            .field("ha_ca_file", &self.ha_ca_file)
            .field("scheduler_ca_file", &self.scheduler_ca_file)
            .finish_non_exhaustive()
    }
}

/// Why the environment was rejected. Messages never contain a secret value.
#[derive(Debug, Error)]
pub enum ControlConfigError {
    #[error("config: {0} is required")]
    Required(String),
    #[error("config: {name} (or {name}_FILE) is required")]
    TokenRequired { name: &'static str },
    #[error("config: read {name}_FILE: cannot read the file")]
    TokenFile { name: &'static str },
    #[error("config: {name}_FILE is empty")]
    EmptyTokenFile { name: &'static str },
    #[error("config: {0}")]
    Invalid(String),
}

impl ControlConfig {
    /// Read and validate the process environment.
    pub fn load() -> Result<Self, ControlConfigError> {
        Self::load_from(&|key| std::env::var(key).ok(), &|path| std::fs::read(path))
    }

    /// Load from injected environment and file readers (used by tests).
    pub fn load_from(
        getenv: &dyn Fn(&str) -> Option<String>,
        read_file: &dyn Fn(&str) -> std::io::Result<Vec<u8>>,
    ) -> Result<Self, ControlConfigError> {
        let ha_url = base_url("HA_URL", getenv("HA_URL").as_deref(), true)
            .map_err(|error| control_error(&error))?;
        let scheduler_url = base_url(
            "SCHEDULER_ENDPOINT",
            getenv("SCHEDULER_ENDPOINT").as_deref(),
            true,
        )
        .map_err(|error| control_error(&error))?;
        let scheduler_url = scheduler_url
            .strip_suffix("/home.v1.SchedulerService")
            .map_or(scheduler_url.clone(), str::to_owned);

        let ha_token = require_token(getenv, read_file, "HA_TOKEN")?;
        let scheduler_token = require_token(getenv, read_file, "SCHEDULER_TOKEN")?;
        let control_token = require_token(getenv, read_file, "CONTROL_TOKEN")?;

        let ha_ca_file = non_empty(getenv("HA_CA_FILE")).map(PathBuf::from);
        let scheduler_ca_file = non_empty(getenv("SCHEDULER_CA_FILE")).map(PathBuf::from);

        if non_empty(getenv("CONTROL_LABEL")).is_some() {
            return Err(ControlConfigError::Invalid(
                "CONTROL_LABEL was removed; label scenario scripts with the HA label \
                 \"scenario\" (or set CONTROL_SCENARIO_LABEL to another label id)"
                    .to_owned(),
            ));
        }
        let scenario_label =
            non_empty(getenv("CONTROL_SCENARIO_LABEL")).unwrap_or_else(|| "scenario".to_owned());
        let profile_label = non_empty(getenv("CONTROL_PROFILE_LABEL"))
            .unwrap_or_else(|| "voice_profile".to_owned());
        let exclude = getenv("CONTROL_EXCLUDE")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        Rules {
            scenario_label: scenario_label.clone(),
            profile_label: profile_label.clone(),
            exclude: exclude.clone(),
        }
        .validate()
        .map_err(|error| {
            ControlConfigError::Invalid(format!(
                "CONTROL_SCENARIO_LABEL/CONTROL_PROFILE_LABEL/CONTROL_EXCLUDE: {error}"
            ))
        })?;

        let listen =
            non_empty(getenv("CONTROL_LISTEN")).unwrap_or_else(|| DEFAULT_LISTEN.to_owned());
        if split_host_port(&listen).is_none() {
            return Err(ControlConfigError::Invalid(
                "CONTROL_LISTEN must be host:port".to_owned(),
            ));
        }

        let queue = non_empty(getenv("CONTROL_QUEUE")).unwrap_or_else(|| DEFAULT_QUEUE.to_owned());
        if !valid_queue(&queue) || queue == "home" || queue == "tasks" {
            return Err(ControlConfigError::Invalid(
                "CONTROL_QUEUE must be a dedicated queue (not home or tasks)".to_owned(),
            ));
        }
        let consumer_id = getenv("CONTROL_CONSUMER_ID")
            .unwrap_or_default()
            .trim()
            .to_owned();
        if !valid_consumer(&consumer_id) {
            return Err(ControlConfigError::Invalid(
                "CONTROL_CONSUMER_ID must match [A-Za-z0-9._-]{1,64}".to_owned(),
            ));
        }
        let execute =
            bool_value(getenv, "CONTROL_EXECUTE", false).map_err(|error| control_error(&error))?;

        let mut poll_interval = Duration::from_millis(500);
        let mut max_backoff = Duration::from_secs(2 * 60);
        let mut ha_timeout = Duration::from_secs(10);
        let mut scheduler_timeout = Duration::from_secs(10);
        let mut cache_ttl = Duration::from_secs(3);
        for (target, name) in [
            (&mut poll_interval, "CONTROL_POLL_INTERVAL"),
            (&mut max_backoff, "CONTROL_MAX_BACKOFF"),
            (&mut ha_timeout, "CONTROL_HA_TIMEOUT"),
            (&mut cache_ttl, "CONTROL_CACHE_TTL"),
            (&mut scheduler_timeout, "CONTROL_SCHEDULER_TIMEOUT"),
        ] {
            if let Some(raw) = non_empty(getenv(name)) {
                let value = parse_go_duration(&raw).map_err(|_| {
                    ControlConfigError::Invalid(format!("{name} must be a positive Go duration"))
                })?;
                if value.is_zero() {
                    return Err(ControlConfigError::Invalid(format!(
                        "{name} must be a positive Go duration"
                    )));
                }
                *target = value;
            }
        }

        Ok(Self {
            ha_url,
            ha_token,
            ha_ca_file,
            scheduler_url,
            scheduler_token,
            scheduler_ca_file,
            scenario_label,
            profile_label,
            exclude,
            cache_ttl,
            listen,
            control_token,
            consumer_id,
            queue,
            execute,
            batch_size: 10,
            lease_seconds: 300,
            max_attempts: 10,
            poll_interval,
            max_backoff,
            ha_timeout,
            scheduler_timeout,
        })
    }

    /// The rules discovery applies.
    #[must_use]
    pub fn rules(&self) -> Rules {
        Rules {
            scenario_label: self.scenario_label.clone(),
            profile_label: self.profile_label.clone(),
            exclude: self.exclude.clone(),
        }
    }
}

fn control_error(error: &crate::config::ConfigError) -> ControlConfigError {
    let text = error.to_string();
    let text = text.strip_prefix("config: ").unwrap_or(&text);
    ControlConfigError::Invalid(text.to_owned())
}

fn require_token(
    getenv: &dyn Fn(&str) -> Option<String>,
    read_file: &dyn Fn(&str) -> std::io::Result<Vec<u8>>,
    name: &'static str,
) -> Result<Token, ControlConfigError> {
    token(getenv, read_file, name)
        .map_err(|error| match error {
            crate::config::ConfigError::TokenFile { .. } => ControlConfigError::TokenFile { name },
            crate::config::ConfigError::EmptyTokenFile { .. } => {
                ControlConfigError::EmptyTokenFile { name }
            }
            other => control_error(&other),
        })?
        .filter(|token| !token.expose().is_empty())
        .ok_or(ControlConfigError::TokenRequired { name })
}

/// `^[a-z][a-z0-9._-]{0,31}$`
fn valid_queue(value: &str) -> bool {
    let mut bytes = value.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    value.len() <= 32
        && first.is_ascii_lowercase()
        && bytes.all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

/// `^[A-Za-z0-9._-]{1,64}$`
fn valid_consumer(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

/// Split `host:port` the way Go's `net.SplitHostPort` does for our addresses:
/// an optional bracketed IPv6 host plus a non-empty port.
#[must_use]
pub fn split_host_port(listen: &str) -> Option<(&str, &str)> {
    let (host, port) = if let Some(rest) = listen.strip_prefix('[') {
        let (host, rest) = rest.split_once(']')?;
        let port = rest.strip_prefix(':')?;
        (host, port)
    } else {
        let (host, port) = listen.rsplit_once(':')?;
        if host.contains(':') {
            return None;
        }
        (host, port)
    };
    if port.is_empty() || port.parse::<u16>().is_err() {
        return None;
    }
    Some((host, port))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(values: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| {
            values
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    fn no_file(_path: &str) -> std::io::Result<Vec<u8>> {
        Ok(Vec::new())
    }

    fn base() -> Vec<(&'static str, &'static str)> {
        vec![
            ("HA_URL", "http://192.0.2.28:8123"),
            ("HA_TOKEN", "h"),
            (
                "SCHEDULER_ENDPOINT",
                "http://192.0.2.27:8098/home.v1.SchedulerService",
            ),
            ("SCHEDULER_TOKEN", "s"),
            ("CONTROL_TOKEN", "c"),
            ("CONTROL_CONSUMER_ID", "ha-control-home"),
        ]
    }

    #[test]
    fn defaults_match_the_go_control() {
        let values = base();
        let config = ControlConfig::load_from(&env(&values), &no_file).expect("load");
        assert_eq!(config.queue, "control");
        assert_eq!(config.listen, "127.0.0.1:8099");
        assert!(!config.execute);
        assert_eq!(config.scenario_label, "scenario");
        assert_eq!(config.profile_label, "voice_profile");
        assert_eq!(config.scheduler_url, "http://192.0.2.27:8098");
        assert_eq!(config.batch_size, 10);
        assert_eq!(config.lease_seconds, 300);
        assert_eq!(config.max_attempts, 10);
        assert_eq!(config.poll_interval, Duration::from_millis(500));
        assert_eq!(config.max_backoff, Duration::from_secs(120));
        assert_eq!(config.cache_ttl, Duration::from_secs(3));
        assert!(!config.ha_token.expose().is_empty());
    }

    #[test]
    fn every_rejected_environment_matches_the_go_loader() {
        for mutate in [
            ("no control token", "CONTROL_TOKEN", None),
            ("bad label", "CONTROL_SCENARIO_LABEL", Some("My Label")),
            (
                "bad profile",
                "CONTROL_PROFILE_LABEL",
                Some("Voice Profile"),
            ),
            ("legacy label", "CONTROL_LABEL", Some("phone")),
            ("bad exclude", "CONTROL_EXCLUDE", Some("[")),
            ("home queue", "CONTROL_QUEUE", Some("home")),
            ("tasks queue", "CONTROL_QUEUE", Some("tasks")),
            ("bad consumer", "CONTROL_CONSUMER_ID", Some("a b")),
            ("creds in url", "HA_URL", Some("http://u:p@host")),
            ("bad listen", "CONTROL_LISTEN", Some("8099")),
            ("bad execute", "CONTROL_EXECUTE", Some("maybe")),
        ] {
            let (name, key, value) = mutate;
            let mut values = base();
            values.retain(|(current, _)| *current != key);
            if let Some(value) = value {
                values.push((key, value));
            }
            assert!(
                ControlConfig::load_from(&env(&values), &no_file).is_err(),
                "{name} was accepted"
            );
        }
    }

    #[test]
    fn durations_accept_go_syntax() {
        let mut values = base();
        values.push(("CONTROL_CACHE_TTL", "1s"));
        values.push(("CONTROL_POLL_INTERVAL", "1m30s"));
        let config = ControlConfig::load_from(&env(&values), &no_file).expect("load");
        assert_eq!(config.cache_ttl, Duration::from_secs(1));
        assert_eq!(config.poll_interval, Duration::from_secs(90));
    }

    #[test]
    fn token_file_wins_and_is_required() {
        let mut values = base();
        values.push(("CONTROL_TOKEN", "env-secret"));
        values.push(("CONTROL_TOKEN_FILE", "/run/token"));
        let config = ControlConfig::load_from(&env(&values), &|path| {
            assert_eq!(path, "/run/token");
            Ok(b" file-secret \n".to_vec())
        })
        .expect("load");
        assert_eq!(config.control_token.expose(), "file-secret");
    }

    #[test]
    fn host_port_split_matches_net_split_host_port() {
        assert_eq!(
            split_host_port("127.0.0.1:8099"),
            Some(("127.0.0.1", "8099"))
        );
        assert_eq!(split_host_port("0.0.0.0:8099"), Some(("0.0.0.0", "8099")));
        assert_eq!(split_host_port("[::]:8099"), Some(("::", "8099")));
        assert_eq!(split_host_port("8099"), None);
        assert_eq!(split_host_port("host:port"), None);
    }
}
