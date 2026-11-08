//! `ha-scenario` process configuration.
//!
//! Same environment shape and secret priority as `ha-control`: HA and scheduler
//! endpoints/tokens, a stable consumer id, a dedicated queue and the execute
//! switch. On top of that it needs the condition timezone fallback and the
//! scenario ledger path. Tokens never appear in an error message.

use std::path::PathBuf;
use std::time::Duration;

use home_config::parse_go_duration;
use home_secret::Token;
use thiserror::Error;

use crate::config::{base_url, bool_value, non_empty, token};

/// The dedicated queue carrying `scenario.fire` / `scenario.stop`.
pub const DEFAULT_QUEUE: &str = "scenario";
/// Fallback timezone for time-window conditions without a time trigger.
pub const DEFAULT_TIMEZONE: &str = "Asia/Almaty";
/// A scenario run must finish inside one lease to be acknowledged.
const DEFAULT_LEASE_SECONDS: i32 = 3600;
const DEFAULT_BATCH_SIZE: i32 = 10;
const DEFAULT_MAX_ATTEMPTS: i32 = 10;

/// A fully validated `ha-scenario` configuration.
#[derive(Clone)]
pub struct ScenarioConfig {
    pub ha_url: String,
    pub ha_token: Token,
    pub ha_ca_file: Option<PathBuf>,
    pub scheduler_url: String,
    pub scheduler_token: Token,
    pub scheduler_ca_file: Option<PathBuf>,
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
    pub catalog_ttl: Duration,
    pub timezone: String,
    pub ledger_path: Option<PathBuf>,
}

impl std::fmt::Debug for ScenarioConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ScenarioConfig")
            .field("ha_url", &self.ha_url)
            .field("scheduler_url", &self.scheduler_url)
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
            .field("catalog_ttl", &self.catalog_ttl)
            .field("timezone", &self.timezone)
            .field("ledger_path", &self.ledger_path)
            .field("ha_ca_file", &self.ha_ca_file)
            .field("scheduler_ca_file", &self.scheduler_ca_file)
            .finish_non_exhaustive()
    }
}

/// Why the environment was rejected. Messages never contain a secret value.
#[derive(Debug, Error)]
pub enum ScenarioConfigError {
    #[error("config: {name} (or {name}_FILE) is required")]
    TokenRequired { name: &'static str },
    #[error("config: read {name}_FILE: cannot read the file")]
    TokenFile { name: &'static str },
    #[error("config: {name}_FILE is empty")]
    EmptyTokenFile { name: &'static str },
    #[error("config: {0}")]
    Invalid(String),
}

impl ScenarioConfig {
    /// Catalog discovery rules. The scenario executor uses the same catalog as
    /// `ha-control`, so a scenario can only reference entries that the phone
    /// catalog also exposes. The label defaults match the home deployment.
    #[must_use]
    pub fn rules(&self) -> crate::catalog::Rules {
        crate::catalog::Rules {
            scenario_label: "scenario".to_owned(),
            profile_label: "voice_profile".to_owned(),
            exclude: Vec::new(),
        }
    }

    /// Read and validate the process environment.
    pub fn load() -> Result<Self, ScenarioConfigError> {
        Self::load_from(&|key| std::env::var(key).ok(), &|path| std::fs::read(path))
    }

    /// Load from injected environment and file readers (used by tests).
    pub fn load_from(
        getenv: &dyn Fn(&str) -> Option<String>,
        read_file: &dyn Fn(&str) -> std::io::Result<Vec<u8>>,
    ) -> Result<Self, ScenarioConfigError> {
        let ha_url = base_url("HA_URL", getenv("HA_URL").as_deref(), true)
            .map_err(|error| scenario_error(&error))?;
        let scheduler_url = base_url(
            "SCHEDULER_ENDPOINT",
            getenv("SCHEDULER_ENDPOINT").as_deref(),
            true,
        )
        .map_err(|error| scenario_error(&error))?;
        let scheduler_url = scheduler_url
            .strip_suffix("/home.v1.SchedulerService")
            .map_or(scheduler_url.clone(), str::to_owned);

        let ha_token = require_token(getenv, read_file, "HA_TOKEN")?;
        let scheduler_token = require_token(getenv, read_file, "SCHEDULER_TOKEN")?;

        let ha_ca_file = non_empty(getenv("HA_CA_FILE")).map(PathBuf::from);
        let scheduler_ca_file = non_empty(getenv("SCHEDULER_CA_FILE")).map(PathBuf::from);

        let queue = non_empty(getenv("SCENARIO_QUEUE")).unwrap_or_else(|| DEFAULT_QUEUE.to_owned());
        if !crate::control_config::valid_queue(&queue) || queue == "home" || queue == "tasks" {
            return Err(ScenarioConfigError::Invalid(
                "SCENARIO_QUEUE must be a dedicated queue (not home or tasks)".to_owned(),
            ));
        }
        let consumer_id = getenv("SCENARIO_CONSUMER_ID")
            .unwrap_or_default()
            .trim()
            .to_owned();
        if !crate::control_config::valid_consumer(&consumer_id) {
            return Err(ScenarioConfigError::Invalid(
                "SCENARIO_CONSUMER_ID must match [A-Za-z0-9._-]{1,64}".to_owned(),
            ));
        }
        let execute = bool_value(getenv, "SCENARIO_EXECUTE", false)
            .map_err(|error| scenario_error(&error))?;

        let timezone =
            non_empty(getenv("SCENARIO_TIMEZONE")).unwrap_or_else(|| DEFAULT_TIMEZONE.to_owned());
        if jiff::tz::TimeZone::get(&timezone).is_err() {
            return Err(ScenarioConfigError::Invalid(
                "SCENARIO_TIMEZONE must be an IANA timezone".to_owned(),
            ));
        }

        let ledger_path = non_empty(getenv("SCENARIO_LEDGER_PATH")).map(PathBuf::from);

        let mut poll_interval = Duration::from_millis(500);
        let mut max_backoff = Duration::from_secs(2 * 60);
        let mut ha_timeout = Duration::from_secs(10);
        let mut scheduler_timeout = Duration::from_secs(10);
        let mut catalog_ttl = Duration::from_secs(3);
        for (target, name) in [
            (&mut poll_interval, "SCENARIO_POLL_INTERVAL"),
            (&mut max_backoff, "SCENARIO_MAX_BACKOFF"),
            (&mut ha_timeout, "SCENARIO_HA_TIMEOUT"),
            (&mut catalog_ttl, "SCENARIO_CATALOG_TTL"),
            (&mut scheduler_timeout, "SCENARIO_SCHEDULER_TIMEOUT"),
        ] {
            if let Some(raw) = non_empty(getenv(name)) {
                let value = parse_go_duration(&raw).map_err(|_| {
                    ScenarioConfigError::Invalid(format!("{name} must be a positive Go duration"))
                })?;
                if value.is_zero() {
                    return Err(ScenarioConfigError::Invalid(format!(
                        "{name} must be a positive Go duration"
                    )));
                }
                *target = value;
            }
        }

        let mut batch_size = DEFAULT_BATCH_SIZE;
        let mut lease_seconds = DEFAULT_LEASE_SECONDS;
        let mut max_attempts = DEFAULT_MAX_ATTEMPTS;
        for (target, name) in [
            (&mut batch_size, "SCENARIO_BATCH_SIZE"),
            (&mut lease_seconds, "SCENARIO_LEASE_SECONDS"),
            (&mut max_attempts, "SCENARIO_MAX_ATTEMPTS"),
        ] {
            if let Some(raw) = non_empty(getenv(name)) {
                let value = raw.parse::<i32>().map_err(|_| {
                    ScenarioConfigError::Invalid(format!("{name} must be an integer"))
                })?;
                *target = value;
            }
        }
        if !(1..=200).contains(&batch_size) {
            return Err(ScenarioConfigError::Invalid(
                "SCENARIO_BATCH_SIZE must be within 1..200".to_owned(),
            ));
        }
        if !(1..=86_400).contains(&lease_seconds) {
            return Err(ScenarioConfigError::Invalid(
                "SCENARIO_LEASE_SECONDS must be within 1..86400".to_owned(),
            ));
        }
        if !(1..=1000).contains(&max_attempts) {
            return Err(ScenarioConfigError::Invalid(
                "SCENARIO_MAX_ATTEMPTS must be within 1..1000".to_owned(),
            ));
        }

        Ok(Self {
            ha_url,
            ha_token,
            ha_ca_file,
            scheduler_url,
            scheduler_token,
            scheduler_ca_file,
            consumer_id,
            queue,
            execute,
            batch_size,
            lease_seconds,
            max_attempts,
            poll_interval,
            max_backoff,
            ha_timeout,
            scheduler_timeout,
            catalog_ttl,
            timezone,
            ledger_path,
        })
    }
}

fn scenario_error(error: &crate::config::ConfigError) -> ScenarioConfigError {
    let text = error.to_string();
    let text = text.strip_prefix("config: ").unwrap_or(&text);
    ScenarioConfigError::Invalid(text.to_owned())
}

fn require_token(
    getenv: &dyn Fn(&str) -> Option<String>,
    read_file: &dyn Fn(&str) -> std::io::Result<Vec<u8>>,
    name: &'static str,
) -> Result<Token, ScenarioConfigError> {
    token(getenv, read_file, name)
        .map_err(|error| match error {
            crate::config::ConfigError::TokenFile { .. } => ScenarioConfigError::TokenFile { name },
            crate::config::ConfigError::EmptyTokenFile { .. } => {
                ScenarioConfigError::EmptyTokenFile { name }
            }
            other => scenario_error(&other),
        })?
        .filter(|token| !token.expose().is_empty())
        .ok_or(ScenarioConfigError::TokenRequired { name })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

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
            ("SCENARIO_CONSUMER_ID", "ha-scenario-home"),
        ]
    }

    #[test]
    fn defaults_are_the_documented_ones() {
        let values = base();
        let config = ScenarioConfig::load_from(&env(&values), &no_file).expect("load");
        assert_eq!(config.queue, "scenario");
        assert!(!config.execute);
        assert_eq!(config.timezone, "Asia/Almaty");
        assert_eq!(config.scheduler_url, "http://192.0.2.27:8098");
        assert_eq!(config.batch_size, 10);
        assert_eq!(config.lease_seconds, 3600);
        assert_eq!(config.max_attempts, 10);
        assert_eq!(config.poll_interval, Duration::from_millis(500));
        assert_eq!(config.catalog_ttl, Duration::from_secs(3));
        assert_eq!(config.ledger_path, None);
        assert!(!config.ha_token.expose().is_empty());
    }

    #[test]
    fn rejected_environments_never_leak_a_token() {
        for mutate in [
            ("no ha token", "HA_TOKEN", None),
            ("no scheduler token", "SCHEDULER_TOKEN", None),
            ("home queue", "SCENARIO_QUEUE", Some("home")),
            ("tasks queue", "SCENARIO_QUEUE", Some("tasks")),
            ("bad consumer", "SCENARIO_CONSUMER_ID", Some("a b")),
            ("bad execute", "SCENARIO_EXECUTE", Some("maybe")),
            ("bad timezone", "SCENARIO_TIMEZONE", Some("Mars/Olympus")),
            ("bad lease", "SCENARIO_LEASE_SECONDS", Some("0")),
            ("bad batch", "SCENARIO_BATCH_SIZE", Some("201")),
            ("creds in url", "HA_URL", Some("http://u:p@host")),
        ] {
            let (name, key, value) = mutate;
            let mut values = base();
            values.retain(|(current, _)| *current != key);
            if let Some(value) = value {
                values.push((key, value));
            }
            let error = ScenarioConfig::load_from(&env(&values), &no_file).expect_err(name);
            assert!(!error.to_string().contains("scheduler-secret"));
        }
    }

    #[test]
    fn execute_and_timezone_are_configurable() {
        let mut values = base();
        values.push(("SCENARIO_EXECUTE", "true"));
        values.push(("SCENARIO_TIMEZONE", "Europe/Berlin"));
        values.push(("SCENARIO_LEDGER_PATH", "/data/scenario-ledger.json"));
        let config = ScenarioConfig::load_from(&env(&values), &no_file).expect("load");
        assert!(config.execute);
        assert_eq!(config.timezone, "Europe/Berlin");
        assert_eq!(
            config.ledger_path.as_deref(),
            Some(std::path::Path::new("/data/scenario-ledger.json"))
        );
    }

    #[test]
    fn token_file_wins() {
        let mut values = base();
        values.push(("SCHEDULER_TOKEN", "env-secret"));
        values.push(("SCHEDULER_TOKEN_FILE", "/run/token"));
        let config = ScenarioConfig::load_from(&env(&values), &|path| {
            assert_eq!(path, "/run/token");
            Ok(b" file-secret \n".to_vec())
        })
        .expect("load");
        assert_eq!(config.scheduler_token.expose(), "file-secret");
    }
}
