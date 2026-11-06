//! Process configuration loaded 1:1 from the Go service's environment.
//!
//! Required values are never empty; the process fails fast otherwise. Secrets
//! use `home_secret` so `Debug` never prints them.

use std::net::SocketAddr;
use std::path::PathBuf;

use home_config::LogLevel;
use home_secret::{Secret, Token};
use jiff::tz::TimeZone;
use thiserror::Error;

/// Default SQLite path.
pub const DEFAULT_DB_PATH: &str = "/data/currency-bot.db";
/// Default API listener.
pub const DEFAULT_HTTP_ADDR: &str = ":8080";
/// Default webhook listener.
pub const DEFAULT_WEBHOOK_ADDR: &str = ":8081";
/// Default time zone.
pub const DEFAULT_TIMEZONE: &str = "UTC";
/// Default rates page.
pub const DEFAULT_KURS_URL: &str = "https://kurs.kz";
/// Default Bot API root.
pub const DEFAULT_TELEGRAM_API: &str = "https://api.telegram.org";
/// Retention is hard-coded in the Go service and logged at start-up.
pub const RETENTION_DAYS: i64 = 35;

/// A fully validated process configuration.
#[derive(Debug, Clone)]
pub struct Config {
    pub bot_token: Token,
    pub allowed_chats: Vec<String>,
    pub webhook_secret: Secret,
    pub api_token: Token,
    pub webhook_url: Option<String>,

    pub db_path: PathBuf,
    pub http_addr: String,
    pub webhook_addr: String,
    pub kurs_url: String,
    pub telegram_api: String,
    pub timezone: String,
    pub location: TimeZone,

    pub hourly_minute: u8,
    /// Go `time.Weekday`: 0 = Sunday.
    pub cleanup_weekday: i8,
    pub cleanup_hour: u8,
    pub retention_days: i64,
    pub log_level: LogLevel,
}

impl Config {
    /// Read and validate the process environment.
    pub fn load() -> Result<Self, ConfigError> {
        Self::load_from(&|key| std::env::var(key).ok())
    }

    /// Load from an injected environment (tests).
    pub fn load_from(getenv: &dyn Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let trimmed = |key: &str| getenv(key).map(|value| value.trim().to_owned());
        let value_or = |key: &str, default: &str| {
            trimmed(key)
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| default.to_owned())
        };

        let bot_token = trimmed("BOT_TOKEN").unwrap_or_default();
        let chat_raw = trimmed("CHAT_ID").unwrap_or_default();
        let webhook_secret = trimmed("WEBHOOK_SECRET").unwrap_or_default();
        let api_token = trimmed("API_TOKEN").unwrap_or_default();

        let mut missing = Vec::new();
        for (name, value) in [
            ("BOT_TOKEN", bot_token.as_str()),
            ("CHAT_ID", chat_raw.as_str()),
            ("WEBHOOK_SECRET", webhook_secret.as_str()),
            ("API_TOKEN", api_token.as_str()),
        ] {
            if value.is_empty() {
                missing.push(name);
            }
        }
        if !missing.is_empty() {
            missing.sort_unstable();
            return Err(ConfigError::Missing(missing.join(", ")));
        }

        let allowed_chats: Vec<String> = chat_raw
            .split(',')
            .map(str::trim)
            .filter(|chat| !chat.is_empty())
            .map(str::to_owned)
            .collect();
        if allowed_chats.is_empty() {
            return Err(ConfigError::EmptyChats);
        }

        let timezone = value_or("TZ", DEFAULT_TIMEZONE);
        let location =
            TimeZone::get(&timezone).map_err(|_| ConfigError::Timezone(timezone.clone()))?;

        let hourly_minute = int_in_range(getenv, "HOURLY_MINUTE", 0, 0, 59)?;
        let cleanup_hour = int_in_range(getenv, "CLEANUP_HOUR", 0, 0, 23)?;
        let cleanup_weekday = match trimmed("CLEANUP_WEEKDAY") {
            Some(raw) if !raw.is_empty() => parse_weekday(&raw)?,
            _ => 0,
        };

        Ok(Self {
            bot_token: Token::new(bot_token),
            allowed_chats,
            webhook_secret: Secret::new(webhook_secret),
            api_token: Token::new(api_token),
            webhook_url: trimmed("WEBHOOK_URL").filter(|value| !value.is_empty()),
            db_path: PathBuf::from(value_or("DB_PATH", DEFAULT_DB_PATH)),
            http_addr: value_or("HTTP_ADDR", DEFAULT_HTTP_ADDR),
            webhook_addr: value_or("WEBHOOK_ADDR", DEFAULT_WEBHOOK_ADDR),
            kurs_url: value_or("KURS_URL", DEFAULT_KURS_URL),
            telegram_api: value_or("TELEGRAM_API_URL", DEFAULT_TELEGRAM_API),
            timezone,
            location,
            hourly_minute: u8::try_from(hourly_minute).unwrap_or(0),
            cleanup_weekday,
            cleanup_hour: u8::try_from(cleanup_hour).unwrap_or(0),
            retention_days: RETENTION_DAYS,
            log_level: LogLevel::parse(&trimmed("LOG_LEVEL").unwrap_or_default()),
        })
    }

    /// Loopback address for `-healthcheck`: a wildcard bind becomes
    /// `127.0.0.1`.
    pub fn healthcheck_address(&self) -> Result<SocketAddr, String> {
        healthcheck_address(&self.http_addr)
    }
}

/// Why the environment was rejected. Messages match the Go loader and never
/// contain a secret value.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required environment variables: {0}")]
    Missing(String),
    #[error("CHAT_ID must contain at least one chat id")]
    EmptyChats,
    #[error("TZ: unknown IANA timezone {0:?}")]
    Timezone(String),
    #[error("{0}")]
    Invalid(String),
}

fn int_in_range(
    getenv: &dyn Fn(&str) -> Option<String>,
    key: &str,
    default: i64,
    min: i64,
    max: i64,
) -> Result<i64, ConfigError> {
    let Some(raw) = getenv(key).map(|value| value.trim().to_owned()) else {
        return Ok(default);
    };
    if raw.is_empty() {
        return Ok(default);
    }
    let value = raw
        .parse::<i64>()
        .map_err(|_| ConfigError::Invalid(format!("{key} must be an integer in {min}..{max}")))?;
    if !(min..=max).contains(&value) {
        return Err(ConfigError::Invalid(format!(
            "{key} must be an integer in {min}..{max}"
        )));
    }
    Ok(value)
}

fn parse_weekday(raw: &str) -> Result<i8, ConfigError> {
    let value = raw.trim().to_ascii_lowercase();
    let day = match value.as_str() {
        "0" | "sun" | "sunday" => 0,
        "1" | "mon" | "monday" => 1,
        "2" | "tue" | "tuesday" => 2,
        "3" | "wed" | "wednesday" => 3,
        "4" | "thu" | "thursday" => 4,
        "5" | "fri" | "friday" => 5,
        "6" | "sat" | "saturday" => 6,
        _ => {
            return Err(ConfigError::Invalid(
                "CLEANUP_WEEKDAY must be 0..6 or a weekday name".to_owned(),
            ));
        }
    };
    Ok(day)
}

/// Turns a listen address into a loopback probe address.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn env(values: Vec<(&'static str, &'static str)>) -> impl Fn(&str) -> Option<String> {
        move |key| {
            values
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    fn base() -> &'static [(&'static str, &'static str)] {
        &[
            ("BOT_TOKEN", "bot"),
            ("CHAT_ID", "123, -100999"),
            ("WEBHOOK_SECRET", "secret"),
            ("API_TOKEN", "api"),
        ]
    }

    #[test]
    fn defaults_and_chat_split() {
        let config = Config::load_from(&env(base().to_vec())).expect("load");
        assert_eq!(config.db_path, PathBuf::from(DEFAULT_DB_PATH));
        assert_eq!(config.http_addr, ":8080");
        assert_eq!(config.webhook_addr, ":8081");
        assert_eq!(config.timezone, "UTC");
        assert_eq!(config.allowed_chats, ["123", "-100999"]);
        assert_eq!(config.hourly_minute, 0);
        assert_eq!(config.cleanup_weekday, 0);
        assert_eq!(config.retention_days, 35);
        assert_eq!(config.webhook_url, None);
        assert_eq!(format!("{:?}", config.bot_token), "Token(***)");
        assert_eq!(format!("{:?}", config.webhook_secret), "Secret(***)");
    }

    #[test]
    fn missing_variables_are_sorted() {
        let error = Config::load_from(&env(vec![("API_TOKEN", "api")])).expect_err("missing");
        assert_eq!(
            error.to_string(),
            "missing required environment variables: BOT_TOKEN, CHAT_ID, WEBHOOK_SECRET"
        );
    }

    #[test]
    fn chat_allowlist_cannot_be_empty() {
        let error = Config::load_from(&env(vec![
            ("BOT_TOKEN", "bot"),
            ("CHAT_ID", " , "),
            ("WEBHOOK_SECRET", "secret"),
            ("API_TOKEN", "api"),
        ]))
        .expect_err("chats");
        assert_eq!(
            error.to_string(),
            "CHAT_ID must contain at least one chat id"
        );
    }

    #[test]
    fn bounds_and_weekday_names() {
        let config = Config::load_from(&env(vec![
            ("BOT_TOKEN", "bot"),
            ("CHAT_ID", "1"),
            ("WEBHOOK_SECRET", "secret"),
            ("API_TOKEN", "api"),
            ("HOURLY_MINUTE", "45"),
            ("CLEANUP_HOUR", "3"),
            ("CLEANUP_WEEKDAY", "Wednesday"),
            ("TZ", "Asia/Almaty"),
        ]))
        .expect("load");
        assert_eq!(config.hourly_minute, 45);
        assert_eq!(config.cleanup_hour, 3);
        assert_eq!(config.cleanup_weekday, 3);
        assert_eq!(config.timezone, "Asia/Almaty");

        for (key, value) in [
            ("HOURLY_MINUTE", "60"),
            ("HOURLY_MINUTE", "many"),
            ("CLEANUP_HOUR", "24"),
            ("CLEANUP_WEEKDAY", "funday"),
            ("TZ", "Mars/Olympus"),
        ] {
            let mut values = base().to_vec();
            values.push((key, value));
            assert!(
                Config::load_from(&env(values.clone())).is_err(),
                "{key}={value}"
            );
        }
    }

    #[test]
    fn healthcheck_uses_loopback() {
        for (input, want) in [
            (":8080", "127.0.0.1:8080"),
            ("0.0.0.0:8080", "127.0.0.1:8080"),
            ("[::]:8080", "127.0.0.1:8080"),
            ("127.0.0.1:9090", "127.0.0.1:9090"),
        ] {
            assert_eq!(healthcheck_address(input).expect(input).to_string(), want);
        }
        assert!(healthcheck_address("nope").is_err());
    }
}
