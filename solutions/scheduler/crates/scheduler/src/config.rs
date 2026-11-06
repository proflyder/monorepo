//! Process configuration, loaded 1:1 from the Go scheduler's environment.
//!
//! `SCHEDULER_*` names take precedence; the legacy `ALARM_*` names remain
//! accepted as fallbacks so an older deployment keeps working. The token is a
//! mounted Secret: `SCHEDULER_TOKEN` wins over `SCHEDULER_TOKEN_FILE`, and its
//! value never appears in an error message.
//!
//! Duration variables keep the Go service's whole-second (and millisecond for
//! the tick) values; they are parsed by [`home_config`] instead of duplicating
//! Go's integer-to-`time.Duration` conversion.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use home_config::parse_duration_seconds;
use home_secret::Secret;
use thiserror::Error;

/// Default listen address.
pub const DEFAULT_LISTEN: &str = "127.0.0.1:8098";
/// Default SQLite file name.
pub const DEFAULT_DB_PATH: &str = "scheduler.db";
/// Default time zone.
pub const DEFAULT_TIMEZONE: &str = "UTC";
/// Default grace for a missed occurrence.
pub const DEFAULT_MISSED_GRACE: Duration = Duration::from_secs(600);
/// Default ringing window.
pub const DEFAULT_RING_WINDOW: Duration = Duration::from_secs(900);
/// Default terminal-event retention.
pub const DEFAULT_RETENTION: Duration = Duration::from_secs(30 * 24 * 60 * 60);
/// Default number of live timers.
pub const DEFAULT_MAX_TIMERS: i64 = 64;
/// Default longest timer.
pub const DEFAULT_MAX_TIMER_DURATION: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// Default payload cap.
pub const DEFAULT_MAX_PAYLOAD_BYTES: usize = 16 * 1024;
/// Default delivery attempts.
pub const DEFAULT_MAX_ATTEMPTS: i64 = 5;
/// Default engine tick.
pub const DEFAULT_TICK_INTERVAL: Duration = Duration::from_secs(1);

/// A fully validated process configuration.
#[derive(Clone)]
pub struct Config {
    pub db_path: PathBuf,
    pub listen: String,
    pub timezone: String,
    pub token: Secret,
    pub missed_grace: Duration,
    pub ring_window: Duration,
    pub event_retention: Duration,
    pub max_timers: i64,
    pub max_timer_duration: Duration,
    pub max_payload_bytes: usize,
    pub max_attempts: i64,
    pub tick_interval: Duration,
    pub allowed_consumers: Vec<String>,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Config")
            .field("db_path", &self.db_path)
            .field("listen", &self.listen)
            .field("timezone", &self.timezone)
            .field("missed_grace", &self.missed_grace)
            .field("ring_window", &self.ring_window)
            .field("event_retention", &self.event_retention)
            .field("max_timers", &self.max_timers)
            .field("max_timer_duration", &self.max_timer_duration)
            .field("max_payload_bytes", &self.max_payload_bytes)
            .field("max_attempts", &self.max_attempts)
            .field("tick_interval", &self.tick_interval)
            .field("allowed_consumers", &self.allowed_consumers)
            .finish_non_exhaustive()
    }
}

/// Why the environment was rejected. Messages match the Go loader and never
/// contain a secret value.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("SCHEDULER_TIMEZONE: unknown IANA timezone {0:?}")]
    Timezone(String),
    #[error("SCHEDULER_TOKEN or SCHEDULER_TOKEN_FILE is required")]
    TokenRequired,
    #[error("SCHEDULER_TOKEN_FILE: cannot read token file")]
    TokenFile,
    #[error("{0}")]
    Invalid(String),
}

impl Config {
    /// Reads and validates the process environment.
    pub fn load() -> Result<Self, ConfigError> {
        Self::load_from(&|key| std::env::var(key).ok(), &|path| std::fs::read(path))
    }

    /// Loads from injected environment and file readers (used by tests).
    pub fn load_from(
        getenv: &dyn Fn(&str) -> Option<String>,
        read_file: &dyn Fn(&str) -> std::io::Result<Vec<u8>>,
    ) -> Result<Self, ConfigError> {
        let db_path = first(getenv, &["SCHEDULER_DB_PATH", "ALARM_DB_PATH"])
            .unwrap_or_else(|| DEFAULT_DB_PATH.to_owned());
        let listen = first(getenv, &["SCHEDULER_LISTEN", "ALARM_LISTEN"])
            .unwrap_or_else(|| DEFAULT_LISTEN.to_owned());
        let timezone = first(getenv, &["SCHEDULER_TIMEZONE", "ALARM_TIMEZONE"])
            .unwrap_or_else(|| DEFAULT_TIMEZONE.to_owned());
        if jiff::tz::TimeZone::get(&timezone).is_err() {
            return Err(ConfigError::Timezone(timezone));
        }

        let token = load_token(getenv, read_file)?;

        let missed_grace = duration_seconds(
            getenv,
            "SCHEDULER_MISSED_GRACE_SECONDS",
            DEFAULT_MISSED_GRACE,
        )?;
        let ring_window =
            duration_seconds(getenv, "SCHEDULER_RING_WINDOW_SECONDS", DEFAULT_RING_WINDOW)?;
        let event_retention = duration_seconds(
            getenv,
            "SCHEDULER_EVENT_RETENTION_SECONDS",
            DEFAULT_RETENTION,
        )?;
        let max_timer_duration = duration_seconds(
            getenv,
            "SCHEDULER_MAX_TIMER_SECONDS",
            DEFAULT_MAX_TIMER_DURATION,
        )?;
        let tick_interval = duration_millis(
            getenv,
            "SCHEDULER_TICK_MS",
            DEFAULT_TICK_INTERVAL.as_millis(),
        )?;
        let max_timers = integer(getenv, "SCHEDULER_MAX_TIMERS", DEFAULT_MAX_TIMERS)?;
        let max_payload_bytes =
            usize::try_from(integer(getenv, "SCHEDULER_MAX_PAYLOAD_BYTES", 16 * 1024)?).map_err(
                |_| ConfigError::Invalid("SCHEDULER_MAX_PAYLOAD_BYTES must be an integer".into()),
            )?;
        let max_attempts = integer(getenv, "SCHEDULER_EVENT_MAX_ATTEMPTS", DEFAULT_MAX_ATTEMPTS)?;

        let mut allowed_consumers = Vec::new();
        if let Some(raw) = first(getenv, &["SCHEDULER_CONSUMERS"]) {
            for part in raw.split(',') {
                let consumer = part.trim();
                if !consumer.is_empty() {
                    allowed_consumers.push(consumer.to_owned());
                }
            }
        }

        let config = Self {
            db_path: PathBuf::from(db_path),
            listen,
            timezone,
            token,
            missed_grace,
            ring_window,
            event_retention,
            max_timers,
            max_timer_duration,
            max_payload_bytes,
            max_attempts,
            tick_interval,
            allowed_consumers,
        };
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        let day = Duration::from_secs(24 * 60 * 60);
        if self.missed_grace.is_zero() || self.missed_grace > day {
            return Err(ConfigError::Invalid(
                "SCHEDULER_MISSED_GRACE_SECONDS must be within 1s..24h".into(),
            ));
        }
        if self.ring_window.is_zero() || self.ring_window > day {
            return Err(ConfigError::Invalid(
                "SCHEDULER_RING_WINDOW_SECONDS must be within 1s..24h".into(),
            ));
        }
        let retention_max = Duration::from_secs(10 * 365 * 24 * 60 * 60);
        if self.event_retention < Duration::from_secs(60 * 60)
            || self.event_retention > retention_max
        {
            return Err(ConfigError::Invalid(
                "SCHEDULER_EVENT_RETENTION_SECONDS must be within 1h..10y".into(),
            ));
        }
        if self.max_timer_duration.is_zero()
            || self.max_timer_duration > Duration::from_secs(30 * 86_400)
        {
            return Err(ConfigError::Invalid(
                "SCHEDULER_MAX_TIMER_SECONDS must be within 1s..30d".into(),
            ));
        }
        if !(1..=4096).contains(&self.max_timers) {
            return Err(ConfigError::Invalid(
                "SCHEDULER_MAX_TIMERS must be within 1..4096".into(),
            ));
        }
        if !(1024..=1 << 20).contains(&self.max_payload_bytes) {
            return Err(ConfigError::Invalid(
                "SCHEDULER_MAX_PAYLOAD_BYTES must be within 1024..1048576".into(),
            ));
        }
        if !(1..=100).contains(&self.max_attempts) {
            return Err(ConfigError::Invalid(
                "SCHEDULER_EVENT_MAX_ATTEMPTS must be within 1..100".into(),
            ));
        }
        if self.tick_interval < Duration::from_millis(50)
            || self.tick_interval > Duration::from_secs(60)
        {
            return Err(ConfigError::Invalid(
                "SCHEDULER_TICK_MS must be within 50..60000".into(),
            ));
        }
        Ok(())
    }

    /// Loopback address for the probe: a wildcard bind becomes `127.0.0.1`.
    pub fn healthcheck_address(&self) -> Result<SocketAddr, String> {
        healthcheck_address(&self.listen)
    }
}

/// Turns a listen address into a loopback probe address. An empty host, an
/// unspecified IPv4 address or an unspecified IPv6 address becomes
/// `127.0.0.1`, matching `scheduler -healthcheck` in Go.
pub fn healthcheck_address(listen: &str) -> Result<SocketAddr, String> {
    let Some((host, port)) = listen.rsplit_once(':') else {
        return Err(format!("invalid listen address: {listen}"));
    };
    if port.parse::<u16>().is_err() {
        return Err(format!("invalid listen address: {listen}"));
    }
    let host = match host.trim() {
        "" | "0.0.0.0" | "[::]" | "::" => "127.0.0.1".to_owned(),
        other => other.to_owned(),
    };
    format!("{host}:{port}")
        .parse()
        .map_err(|_| format!("invalid listen address: {listen}"))
}

fn load_token(
    getenv: &dyn Fn(&str) -> Option<String>,
    read_file: &dyn Fn(&str) -> std::io::Result<Vec<u8>>,
) -> Result<Secret, ConfigError> {
    let direct = first(getenv, &["SCHEDULER_TOKEN"]);
    if let Some(token) = direct {
        return Ok(Secret::new(token));
    }
    let path = first(getenv, &["SCHEDULER_TOKEN_FILE"]);
    let Some(path) = path else {
        return Err(ConfigError::TokenRequired);
    };
    let data = read_file(&path).map_err(|_| ConfigError::TokenFile)?;
    let token = String::from_utf8_lossy(&data);
    let token = token.trim();
    if token.is_empty() {
        return Err(ConfigError::TokenRequired);
    }
    Ok(Secret::new(token))
}

/// The first non-blank value of `keys`, trimmed.
fn first(getenv: &dyn Fn(&str) -> Option<String>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        let value = getenv(key)?;
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        }
    })
}

fn integer(
    getenv: &dyn Fn(&str) -> Option<String>,
    key: &str,
    default: i64,
) -> Result<i64, ConfigError> {
    let Some(raw) = first(getenv, &[key]) else {
        return Ok(default);
    };
    raw.parse::<i64>()
        .map_err(|_| ConfigError::Invalid(format!("{key} must be an integer")))
}

fn duration_seconds(
    getenv: &dyn Fn(&str) -> Option<String>,
    key: &str,
    default: Duration,
) -> Result<Duration, ConfigError> {
    let Some(raw) = first(getenv, &[key]) else {
        return Ok(default);
    };
    parse_duration_seconds(&raw)
        .map_err(|_| ConfigError::Invalid(format!("{key} must be an integer")))
}

fn duration_millis(
    getenv: &dyn Fn(&str) -> Option<String>,
    key: &str,
    default: u128,
) -> Result<Duration, ConfigError> {
    let Some(raw) = first(getenv, &[key]) else {
        return Ok(Duration::from_millis(
            u64::try_from(default).unwrap_or(1000),
        ));
    };
    let millis = raw
        .parse::<u64>()
        .map_err(|_| ConfigError::Invalid(format!("{key} must be an integer")))?;
    Ok(Duration::from_millis(millis))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(values: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
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

    #[test]
    fn defaults_and_token() {
        let config =
            Config::load_from(&env(&[("SCHEDULER_TOKEN", "secret")]), &no_file).expect("load");
        assert_eq!(config.listen, "127.0.0.1:8098");
        assert_eq!(config.db_path, PathBuf::from("scheduler.db"));
        assert_eq!(config.timezone, "UTC");
        assert_eq!(config.missed_grace, Duration::from_secs(600));
        assert_eq!(config.ring_window, Duration::from_secs(900));
        assert_eq!(config.tick_interval, Duration::from_secs(1));
        assert_eq!(config.token.expose(), "secret");
        assert!(
            !format!("{config:?}").contains("secret"),
            "debug must not print the token"
        );
    }

    #[test]
    fn token_is_required() {
        let error = Config::load_from(&env(&[]), &no_file).expect_err("required");
        assert!(error.to_string().contains("SCHEDULER_TOKEN"));
    }

    #[test]
    fn token_file_is_trimmed() {
        let config = Config::load_from(&env(&[("SCHEDULER_TOKEN_FILE", "/run/token")]), &|path| {
            assert_eq!(path, "/run/token");
            Ok(b"  from-file\n".to_vec())
        })
        .expect("load");
        assert_eq!(config.token.expose(), "from-file");
    }

    #[test]
    fn environment_token_wins_over_file() {
        let config = Config::load_from(
            &env(&[
                ("SCHEDULER_TOKEN", "direct"),
                ("SCHEDULER_TOKEN_FILE", "/run/token"),
            ]),
            &|_path| Ok(b"from-file\n".to_vec()),
        )
        .expect("load");
        assert_eq!(config.token.expose(), "direct");
    }

    #[test]
    fn legacy_alarm_fallbacks_and_precedence() {
        let config = Config::load_from(
            &env(&[
                ("SCHEDULER_TOKEN", "t"),
                ("ALARM_DB_PATH", "/legacy/alarm.db"),
                ("ALARM_LISTEN", "127.0.0.1:9000"),
                ("ALARM_TIMEZONE", "Asia/Almaty"),
                ("SCHEDULER_DB_PATH", "/new/scheduler.db"),
            ]),
            &no_file,
        )
        .expect("load");
        assert_eq!(config.db_path, PathBuf::from("/new/scheduler.db"));
        assert_eq!(config.listen, "127.0.0.1:9000");
        assert_eq!(config.timezone, "Asia/Almaty");
    }

    #[test]
    fn consumers_and_bounds() {
        let config = Config::load_from(
            &env(&[
                ("SCHEDULER_TOKEN", "t"),
                ("SCHEDULER_CONSUMERS", "home-1, android, ,"),
                ("SCHEDULER_MAX_TIMERS", "8"),
            ]),
            &no_file,
        )
        .expect("load");
        assert_eq!(config.allowed_consumers, ["home-1", "android"]);
        assert_eq!(config.max_timers, 8);
    }

    #[test]
    fn invalid_configuration_is_rejected() {
        let cases: &[&[(&str, &str)]] = &[
            &[
                ("SCHEDULER_TOKEN", "t"),
                ("SCHEDULER_TIMEZONE", "Mars/Olympus"),
            ],
            &[
                ("SCHEDULER_TOKEN", "t"),
                ("SCHEDULER_MISSED_GRACE_SECONDS", "-5"),
            ],
            &[
                ("SCHEDULER_TOKEN", "t"),
                ("SCHEDULER_RING_WINDOW_SECONDS", "999999"),
            ],
            &[("SCHEDULER_TOKEN", "t"), ("SCHEDULER_TICK_MS", "10")],
            &[("SCHEDULER_TOKEN", "t"), ("SCHEDULER_MAX_TIMERS", "many")],
            &[("SCHEDULER_TOKEN", "t"), ("SCHEDULER_MAX_TIMERS", "100000")],
            &[
                ("SCHEDULER_TOKEN", "t"),
                ("SCHEDULER_MAX_TIMER_SECONDS", "99999999"),
            ],
            &[
                ("SCHEDULER_TOKEN", "t"),
                ("SCHEDULER_MAX_PAYLOAD_BYTES", "10"),
            ],
        ];
        for values in cases {
            assert!(
                Config::load_from(&env(values), &no_file).is_err(),
                "accepted {values:?}"
            );
        }
    }

    #[test]
    fn healthcheck_address_uses_loopback() {
        for (input, want) in [
            ("127.0.0.1:8098", "127.0.0.1:8098"),
            ("0.0.0.0:8098", "127.0.0.1:8098"),
            (":8098", "127.0.0.1:8098"),
            ("[::]:8098", "127.0.0.1:8098"),
        ] {
            assert_eq!(
                healthcheck_address(input).expect(input).to_string(),
                want,
                "{input}"
            );
        }
        assert!(healthcheck_address("not-an-address").is_err());
    }
}
