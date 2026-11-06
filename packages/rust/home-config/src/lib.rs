//! Typed service configuration built on `clap` derive with the `env` feature.
//!
//! Services derive their configuration from [`clap::Parser`]; this crate adds
//! the value parsers that every home service needs and keeps the error
//! messages safe:
//!
//! * [`secret_from_env`] reads a secret from `NAME`, falling back to
//!   `NAME_FILE` (a mounted Secret). Errors name the variable and never
//!   contain the value.
//! * [`secret_file`] is the `clap` value parser for a `*_FILE` path.
//! * [`parse_listen`], [`parse_duration_seconds`] and [`u64_in_range`] bound
//!   an address or an integer before the service starts.
//!
//! There is no project-specific path or endpoint here: a solution supplies
//! its own variable names and defaults.

use std::fmt;
use std::net::SocketAddr;
use std::time::Duration;

use clap::builder::RangedU64ValueParser;
use home_secret::Secret;
use thiserror::Error;

/// Why a configuration value was rejected. Every variant names the variable
/// (or file) and never echoes a secret.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("{name}: {detail}")]
    Invalid { name: String, detail: String },
    #[error("{name}: cannot read the secret file")]
    SecretFile { name: String },
    #[error("{name}: secret is empty")]
    EmptySecret { name: String },
    #[error("{name}: is required")]
    Missing { name: String },
}

/// Reads a required secret from `NAME`, or from the file named by
/// `NAME_FILE`. A directly set variable wins. Surrounding whitespace is
/// trimmed, matching the Go services' `TrimSpace` on mounted Secret files.
pub fn secret_from_env(name: &str) -> Result<Secret, ConfigError> {
    if let Some(value) = std::env::var_os(name) {
        let value = value.to_string_lossy();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(ConfigError::EmptySecret {
                name: name.to_owned(),
            });
        }
        return Ok(Secret::new(trimmed));
    }
    if let Some(path) = std::env::var_os(format!("{name}_FILE")) {
        return secret_file(&path.to_string_lossy()).map_err(|error| match error {
            ConfigError::Invalid { .. } => ConfigError::SecretFile {
                name: name.to_owned(),
            },
            other => other,
        });
    }
    Err(ConfigError::Missing {
        name: name.to_owned(),
    })
}

/// `clap` value parser: reads a secret from the given file path.
pub fn secret_file(path: &str) -> Result<Secret, ConfigError> {
    let name = path.to_owned();
    let bytes = std::fs::read(path).map_err(|_| ConfigError::SecretFile { name: name.clone() })?;
    let text = String::from_utf8_lossy(&bytes);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(ConfigError::EmptySecret { name });
    }
    Ok(Secret::new(trimmed))
}

/// `clap` value parser for `host:port`, including `0.0.0.0` and `[::]`.
pub fn parse_listen(value: &str) -> Result<SocketAddr, String> {
    value
        .parse()
        .map_err(|_| "expected a numeric host:port listen address".to_owned())
}

/// `clap` value parser for whole seconds, returned as a [`Duration`].
pub fn parse_duration_seconds(value: &str) -> Result<Duration, String> {
    let seconds: u64 = value
        .parse()
        .map_err(|_| "expected a whole number of seconds".to_owned())?;
    Ok(Duration::from_secs(seconds))
}

/// `clap` value parser for an inclusive integer range.
#[must_use]
pub fn u64_in_range(min: u64, max: u64) -> RangedU64ValueParser<u64> {
    clap::value_parser!(u64).range(min..=max)
}

/// A log level parsed from `LOG_LEVEL`, defaulting to `info`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    /// Case-insensitive parse; unknown values fall back to `info`, matching
    /// the Go services.
    #[must_use]
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "debug" => Self::Debug,
            "warn" => Self::Warn,
            "error" => Self::Error,
            _ => Self::Info,
        }
    }

    /// Values accepted by [`LogLevel::parse`].
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

impl fmt::Display for LogLevel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::*;

    fn temp_file(contents: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "home-config-{}-{}.secret",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let mut file = std::fs::File::create(&path).expect("create");
        file.write_all(contents.as_bytes()).expect("write");
        path
    }

    #[test]
    fn secret_file_trims_and_rejects_empty() {
        let path = temp_file("  hunter2\n");
        let secret = secret_file(path.to_str().expect("path")).expect("secret");
        assert_eq!(secret.expose(), "hunter2");
        let _ = std::fs::remove_file(path);

        let empty = temp_file(" \n");
        assert!(matches!(
            secret_file(empty.to_str().expect("path")),
            Err(ConfigError::EmptySecret { .. })
        ));
        let _ = std::fs::remove_file(empty);

        let error = secret_file("/definitely/not/a/secret").expect_err("missing file");
        assert!(!format!("{error}").contains("hunter2"));
    }

    #[test]
    fn value_parsers_are_bounded() {
        assert_eq!(
            parse_listen("127.0.0.1:8099").expect("listen"),
            "127.0.0.1:8099".parse().expect("addr")
        );
        assert!(parse_listen("nope").is_err());
        assert_eq!(
            parse_duration_seconds("180").expect("duration"),
            Duration::from_secs(180)
        );
        assert!(parse_duration_seconds("-1").is_err());
        assert!(parse_duration_seconds("1.5").is_err());
    }

    #[test]
    fn log_level_is_case_insensitive_with_info_default() {
        assert_eq!(LogLevel::parse("DEBUG"), LogLevel::Debug);
        assert_eq!(LogLevel::parse("warn"), LogLevel::Warn);
        assert_eq!(LogLevel::parse("nonsense"), LogLevel::Info);
    }
}
