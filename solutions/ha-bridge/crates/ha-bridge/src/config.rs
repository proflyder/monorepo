//! Process configuration, loaded 1:1 from the Go bridge's environment.
//!
//! * `HA_TOKEN_FILE` / `SCHEDULER_TOKEN_FILE` win over the direct variables
//!   (the Go bridge's file-first priority; the scheduler service is the
//!   opposite, so the two loaders must not be unified silently).
//! * Durations accept Go syntax (`1s`, `5m`, `1h30m`) through
//!   [`home_config::parse_go_duration`].
//! * Defaults, bounds and the lease budget match `internal/config/config.go`.
//!
//! The token value never appears in an error message.

use std::path::PathBuf;
use std::time::Duration;

use home_config::parse_go_duration;
use home_secret::Token;
use thiserror::Error;

/// The queue carrying alarms, reminders and timers.
pub const QUEUE_HOME: &str = "home";
/// The scheduler's generic task queue: the HA executor must never lease it.
pub const QUEUE_TASKS: &str = "tasks";

const MIN_BATCH_SIZE: u32 = 1;
const MAX_BATCH_SIZE: u32 = 200;
const MIN_LEASE_SECONDS: u32 = 1;
const MAX_LEASE_SECONDS: u32 = 3600;
const MAX_ATTEMPTS_MIN: u32 = 1;
const MAX_ATTEMPTS_MAX: u32 = 1000;

/// Worst-case time spent writing the receipt ledger for one event.
const LEDGER_BUDGET: Duration = Duration::from_secs(1);
/// Keeps the lease valid a little beyond the modelled processing time.
const LEASE_CUSHION: Duration = Duration::from_secs(15);

/// A fully validated bridge configuration.
#[derive(Clone)]
pub struct Config {
    pub ha_url: String,
    pub scheduler_url: String,
    pub ha_token: Token,
    pub scheduler_token: Token,
    pub consumer_id: String,
    pub queue: String,
    pub execute: bool,
    pub batch_size: i32,
    pub lease_seconds: i32,
    pub poll_interval: Duration,
    pub max_backoff: Duration,
    pub ha_timeout: Duration,
    pub scheduler_timeout: Duration,
    pub max_attempts: i32,
    pub alarm_script: String,
    pub announce_script: String,
    pub stop_script: String,
    pub ledger_path: Option<PathBuf>,
    pub ha_ca_file: Option<PathBuf>,
    pub scheduler_ca_file: Option<PathBuf>,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Config")
            .field("ha_url", &self.ha_url)
            .field("scheduler_url", &self.scheduler_url)
            .field("consumer_id", &self.consumer_id)
            .field("queue", &self.queue)
            .field("execute", &self.execute)
            .field("batch_size", &self.batch_size)
            .field("lease_seconds", &self.lease_seconds)
            .field("poll_interval", &self.poll_interval)
            .field("max_backoff", &self.max_backoff)
            .field("ha_timeout", &self.ha_timeout)
            .field("scheduler_timeout", &self.scheduler_timeout)
            .field("max_attempts", &self.max_attempts)
            .field("alarm_script", &self.alarm_script)
            .field("announce_script", &self.announce_script)
            .field("stop_script", &self.stop_script)
            .field("ledger_path", &self.ledger_path)
            .field("ha_ca_file", &self.ha_ca_file)
            .field("scheduler_ca_file", &self.scheduler_ca_file)
            .finish_non_exhaustive()
    }
}

/// Why the environment was rejected. Messages keep the Go loader's shape and
/// never contain a secret value.
#[derive(Debug, Error)]
pub enum ConfigError {
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

impl Config {
    /// Read and validate the process environment.
    pub fn load() -> Result<Self, ConfigError> {
        Self::load_from(&|key| std::env::var(key).ok(), &|path| std::fs::read(path))
    }

    /// Load from injected environment and file readers (used by tests).
    pub fn load_from(
        getenv: &dyn Fn(&str) -> Option<String>,
        read_file: &dyn Fn(&str) -> std::io::Result<Vec<u8>>,
    ) -> Result<Self, ConfigError> {
        let scheduler_url = base_url(
            "SCHEDULER_ENDPOINT",
            getenv("SCHEDULER_ENDPOINT").as_deref(),
            true,
        )?;
        let scheduler_url = scheduler_url
            .strip_suffix("/home.v1.SchedulerService")
            .map_or(scheduler_url.clone(), str::to_owned);
        let ha_url = base_url("HA_URL", getenv("HA_URL").as_deref(), true)?;

        let scheduler_token =
            token(getenv, read_file, "SCHEDULER_TOKEN")?.ok_or(ConfigError::TokenRequired {
                name: "SCHEDULER_TOKEN",
            })?;
        let ha_token =
            token(getenv, read_file, "HA_TOKEN")?.unwrap_or_else(|| Token::new(String::new()));

        let execute = bool_value(getenv, "BRIDGE_EXECUTE", false)?;
        if execute && ha_token.expose().is_empty() {
            return Err(ConfigError::TokenRequired { name: "HA_TOKEN" });
        }

        let consumer_id = getenv("BRIDGE_CONSUMER_ID")
            .unwrap_or_default()
            .trim()
            .to_owned();
        if consumer_id.is_empty() {
            return Err(ConfigError::Required("BRIDGE_CONSUMER_ID".to_owned()));
        }
        if !valid_identifier(&consumer_id) {
            return Err(ConfigError::Invalid(
                "BRIDGE_CONSUMER_ID must match [A-Za-z0-9._-]{1,64}".to_owned(),
            ));
        }

        let mut queue = getenv("BRIDGE_QUEUE").unwrap_or_default().trim().to_owned();
        if queue.is_empty() {
            queue.clear();
            queue.push_str(QUEUE_HOME);
        }
        if !valid_identifier(&queue) {
            return Err(ConfigError::Invalid(
                "BRIDGE_QUEUE must match [A-Za-z0-9._-]{1,64}".to_owned(),
            ));
        }
        if queue == QUEUE_TASKS {
            return Err(ConfigError::Invalid(
                "BRIDGE_QUEUE=\"tasks\" is reserved for generic tasks and must not be leased by the HA executor"
                    .to_owned(),
            ));
        }

        let batch_size = integer(
            getenv,
            "BRIDGE_BATCH_SIZE",
            10,
            MIN_BATCH_SIZE,
            MAX_BATCH_SIZE,
        )?;
        let lease_seconds = integer(
            getenv,
            "BRIDGE_LEASE_SECONDS",
            300,
            MIN_LEASE_SECONDS,
            MAX_LEASE_SECONDS,
        )?;
        let max_attempts = integer(
            getenv,
            "BRIDGE_MAX_ATTEMPTS",
            10,
            MAX_ATTEMPTS_MIN,
            MAX_ATTEMPTS_MAX,
        )?;

        let ha_timeout = duration_value(getenv, "BRIDGE_HA_TIMEOUT", Duration::from_secs(10))?;
        let scheduler_timeout =
            duration_value(getenv, "BRIDGE_SCHEDULER_TIMEOUT", Duration::from_secs(10))?;
        let poll_interval = duration_value(getenv, "BRIDGE_POLL_INTERVAL", Duration::from_secs(1))?;
        let max_backoff =
            duration_value(getenv, "BRIDGE_MAX_BACKOFF", Duration::from_secs(5 * 60))?;
        if max_backoff < poll_interval {
            return Err(ConfigError::Invalid(format!(
                "BRIDGE_MAX_BACKOFF ({:?}) must be >= BRIDGE_POLL_INTERVAL ({:?})",
                max_backoff, poll_interval
            )));
        }

        let required = required_lease_seconds(batch_size as u32, ha_timeout, scheduler_timeout);
        if lease_seconds < required as i32 {
            return Err(ConfigError::Invalid(format!(
                "BRIDGE_LEASE_SECONDS={lease_seconds} is too short for BRIDGE_BATCH_SIZE={batch_size} with BRIDGE_HA_TIMEOUT={ha_timeout:?} and BRIDGE_SCHEDULER_TIMEOUT={scheduler_timeout:?}; need at least {required} s (raise the lease, lower the batch or shorten the timeouts)"
            )));
        }

        let alarm_script = script_value(getenv, "BRIDGE_ALARM_SCRIPT", "alarm_start")?;
        let announce_script = script_value(getenv, "BRIDGE_ANNOUNCE_SCRIPT", "schedule_announce")?;
        let stop_script = script_value(getenv, "BRIDGE_STOP_SCRIPT", "alarm_stop")?;

        Ok(Self {
            ha_url,
            scheduler_url,
            ha_token,
            scheduler_token,
            consumer_id,
            queue,
            execute,
            batch_size,
            lease_seconds,
            poll_interval,
            max_backoff,
            ha_timeout,
            scheduler_timeout,
            max_attempts,
            alarm_script,
            announce_script,
            stop_script,
            ledger_path: path_value(getenv, "BRIDGE_LEDGER_PATH"),
            ha_ca_file: path_value(getenv, "HA_CA_FILE"),
            scheduler_ca_file: path_value(getenv, "SCHEDULER_CA_FILE"),
        })
    }

    /// Smallest lease that still covers a full batch of sequential executions
    /// with the configured per-request timeouts.
    #[must_use]
    pub fn required_lease_seconds(&self) -> u32 {
        required_lease_seconds(
            u32::try_from(self.batch_size).unwrap_or(1),
            self.ha_timeout,
            self.scheduler_timeout,
        )
    }
}

fn required_lease_seconds(
    batch_size: u32,
    ha_timeout: Duration,
    scheduler_timeout: Duration,
) -> u32 {
    let per_event = ha_timeout + scheduler_timeout + LEDGER_BUDGET;
    let total = per_event * batch_size + LEASE_CUSHION;
    u32::try_from(total.as_secs() + u64::from(total.subsec_nanos() > 0)).unwrap_or(u32::MAX)
}

/// Resolve `NAME_FILE` first, then `NAME`. `Ok(None)` means neither is set.
pub(crate) fn token(
    getenv: &dyn Fn(&str) -> Option<String>,
    read_file: &dyn Fn(&str) -> std::io::Result<Vec<u8>>,
    name: &'static str,
) -> Result<Option<Token>, ConfigError> {
    let file_key = format!("{name}_FILE");
    if let Some(path) = non_empty(getenv(&file_key)) {
        let data = read_file(&path).map_err(|_| ConfigError::TokenFile { name })?;
        let value = String::from_utf8_lossy(&data);
        let value = value.trim();
        if value.is_empty() {
            return Err(ConfigError::EmptyTokenFile { name });
        }
        return Ok(Some(Token::new(value)));
    }
    Ok(non_empty(getenv(name)).map(Token::new))
}

pub(crate) fn base_url(
    name: &str,
    raw: Option<&str>,
    required: bool,
) -> Result<String, ConfigError> {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        if required {
            return Err(ConfigError::Required(name.to_owned()));
        }
        return Ok(String::new());
    };
    let Some((scheme, rest)) = raw.split_once("://") else {
        return Err(ConfigError::Invalid(format!(
            "{name} must use http or https"
        )));
    };
    if scheme != "http" && scheme != "https" {
        return Err(ConfigError::Invalid(format!(
            "{name} must use http or https"
        )));
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty() {
        return Err(ConfigError::Invalid(format!("{name} has no host")));
    }
    if authority.contains('@') {
        return Err(ConfigError::Invalid(format!(
            "{name} must not embed credentials; use a token file instead"
        )));
    }
    if raw.contains('?') || raw.contains('#') {
        return Err(ConfigError::Invalid(format!(
            "{name} must not contain a query or fragment"
        )));
    }
    Ok(raw.trim_end_matches('/').to_owned())
}

fn script_value(
    getenv: &dyn Fn(&str) -> Option<String>,
    name: &str,
    default: &str,
) -> Result<String, ConfigError> {
    let raw = getenv(name)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_owned());
    if valid_identifier(&raw) {
        Ok(raw)
    } else {
        Err(ConfigError::Invalid(format!(
            "{name} must match [A-Za-z0-9_.-]{{1,64}}"
        )))
    }
}

pub(crate) fn bool_value(
    getenv: &dyn Fn(&str) -> Option<String>,
    name: &str,
    default: bool,
) -> Result<bool, ConfigError> {
    let Some(raw) = non_empty(getenv(name)) else {
        return Ok(default);
    };
    // The Go loader uses strconv.ParseBool, which also accepts 1/t/T/0/f/F.
    match raw.as_str() {
        "1" | "t" | "T" | "TRUE" | "true" | "True" => Ok(true),
        "0" | "f" | "F" | "FALSE" | "false" | "False" => Ok(false),
        _ => Err(ConfigError::Invalid(format!("{name} must be true/false"))),
    }
}

fn integer(
    getenv: &dyn Fn(&str) -> Option<String>,
    name: &str,
    default: i32,
    min: u32,
    max: u32,
) -> Result<i32, ConfigError> {
    let Some(raw) = non_empty(getenv(name)) else {
        return Ok(default);
    };
    let value: i64 = raw
        .parse()
        .map_err(|_| ConfigError::Invalid(format!("{name} must be an integer")))?;
    if value < i64::from(min) || value > i64::from(max) {
        return Err(ConfigError::Invalid(format!(
            "{name} must be within {min}..{max}"
        )));
    }
    i32::try_from(value)
        .map_err(|_| ConfigError::Invalid(format!("{name} must be within {min}..{max}")))
}

pub(crate) fn duration_value(
    getenv: &dyn Fn(&str) -> Option<String>,
    name: &str,
    default: Duration,
) -> Result<Duration, ConfigError> {
    let Some(raw) = non_empty(getenv(name)) else {
        return Ok(default);
    };
    let value = parse_go_duration(&raw).map_err(|_| {
        ConfigError::Invalid(format!("{name} must be a Go duration such as 10s or 2m"))
    })?;
    if value.is_zero() {
        return Err(ConfigError::Invalid(format!("{name} must be positive")));
    }
    Ok(value)
}

fn path_value(getenv: &dyn Fn(&str) -> Option<String>, name: &str) -> Option<PathBuf> {
    non_empty(getenv(name)).map(PathBuf::from)
}

pub(crate) fn non_empty(raw: Option<String>) -> Option<String> {
    raw.map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// `[A-Za-z0-9._-]{1,64}`, matching the Go patterns for consumer, queue and
/// script names.
fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
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
            ("HA_URL", "http://ha.example:8123"),
            ("SCHEDULER_ENDPOINT", "http://scheduler.example:8098"),
            ("SCHEDULER_TOKEN", "scheduler-secret"),
            ("BRIDGE_CONSUMER_ID", "ha-bridge-home"),
        ]
    }

    #[test]
    fn defaults_match_the_go_bridge() {
        let values = base();
        let config = Config::load_from(&env(&values), &no_file).expect("load");
        assert!(!config.execute, "dry-run is the default");
        assert_eq!(config.queue, "home");
        assert_eq!(config.batch_size, 10);
        assert_eq!(config.lease_seconds, 300);
        assert_eq!(config.poll_interval, Duration::from_secs(1));
        assert_eq!(config.max_backoff, Duration::from_secs(300));
        assert_eq!(config.max_attempts, 10);
        assert_eq!(config.alarm_script, "alarm_start");
        assert_eq!(config.announce_script, "schedule_announce");
        assert_eq!(config.stop_script, "alarm_stop");
        assert!(config.ha_token.expose().is_empty());
        assert!(!format!("{config:?}").contains("scheduler-secret"));
    }

    #[test]
    fn execute_requires_an_ha_token() {
        let mut values = base();
        values.push(("BRIDGE_EXECUTE", "true"));
        let error = Config::load_from(&env(&values), &no_file).expect_err("required");
        assert!(error.to_string().contains("HA_TOKEN"));
    }

    #[test]
    fn token_file_wins_over_the_environment() {
        let mut values = base();
        values.push(("HA_TOKEN", "env-secret"));
        values.push(("HA_TOKEN_FILE", "/run/token"));
        let config = Config::load_from(&env(&values), &|path| {
            assert_eq!(path, "/run/token");
            Ok(b"  file-secret\n".to_vec())
        })
        .expect("load");
        assert_eq!(config.ha_token.expose(), "file-secret");
    }

    #[test]
    fn empty_token_file_fails_instead_of_falling_back() {
        let mut values = base();
        values.push(("HA_TOKEN", "env-secret"));
        values.push(("HA_TOKEN_FILE", "/run/token"));
        let error = Config::load_from(&env(&values), &|_path| Ok(b"  \n".to_vec()))
            .expect_err("empty file");
        assert!(error.to_string().contains("empty"));
    }

    #[test]
    fn queue_tasks_is_reserved() {
        let mut values = base();
        values.push(("BRIDGE_QUEUE", "tasks"));
        let error = Config::load_from(&env(&values), &no_file).expect_err("tasks");
        assert!(error.to_string().contains("reserved"));
    }

    #[test]
    fn urls_reject_credentials_queries_and_other_schemes() {
        for (name, value) in [
            ("HA_URL", "http://user:pass@ha.example:8123"),
            ("HA_URL", "http://ha.example:8123?token=secret"),
            ("HA_URL", "ftp://ha.example:8123"),
            ("SCHEDULER_ENDPOINT", "https://token@example.test"),
        ] {
            let mut values = base();
            values.retain(|(key, _)| *key != name);
            values.push((name, value));
            assert!(
                Config::load_from(&env(&values), &no_file).is_err(),
                "accepted {name}={value}"
            );
        }
    }

    #[test]
    fn lease_must_cover_the_batch_budget() {
        let mut values = base();
        values.push(("BRIDGE_LEASE_SECONDS", "5"));
        let error = Config::load_from(&env(&values), &no_file).expect_err("short lease");
        assert!(error.to_string().contains("too short"));
    }

    #[test]
    fn go_durations_are_parsed() {
        let mut values = base();
        values.push(("BRIDGE_POLL_INTERVAL", "1h30m"));
        values.push(("BRIDGE_MAX_BACKOFF", "2h"));
        let config = Config::load_from(&env(&values), &no_file).expect("load");
        assert_eq!(config.poll_interval, Duration::from_secs(5400));
        assert_eq!(config.max_backoff, Duration::from_secs(7200));
    }

    #[test]
    fn required_lease_is_the_go_budget() {
        let values = base();
        let config = Config::load_from(&env(&values), &no_file).expect("load");
        assert_eq!(config.required_lease_seconds(), 225);
    }
}
