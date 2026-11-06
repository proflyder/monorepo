//! Typed service configuration built on `clap` derive with the `env` feature.
//!
//! Services derive their configuration from [`clap::Parser`]; this crate adds
//! the value parsers that every home service needs and keeps the error
//! messages safe:
//!
//! * [`secret_from_env`] reads a secret from `NAME`, falling back to
//!   `NAME_FILE` (a mounted Secret). [`secret_from_env_file_first`] is the
//!   opposite priority used by ha-bridge/ha-control: `NAME_FILE` wins when it
//!   is set. Errors name the variable and never contain the value.
//! * [`secret_file`] is the `clap` value parser for a `*_FILE` path.
//! * [`parse_listen`], [`parse_duration_seconds`], [`parse_go_duration`] and
//!   [`u64_in_range`] bound an address, a duration or an integer before the
//!   service starts.
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
    secret_from_env_with(name, SecretPriority::EnvFirst)
}

/// Reads a secret with the `NAME_FILE`-first priority of ha-bridge and
/// ha-control.
///
/// If `NAME_FILE` is set (and not blank), its contents are authoritative: a
/// read or empty-file error is returned instead of falling back to `NAME`, so
/// a broken mounted Secret never silently uses an older env value. When
/// `NAME_FILE` is unset, this behaves like [`secret_from_env`].
pub fn secret_from_env_file_first(name: &str) -> Result<Secret, ConfigError> {
    secret_from_env_with(name, SecretPriority::FileFirst)
}

/// Which variable wins when both `NAME` and `NAME_FILE` are set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretPriority {
    /// The directly set variable wins over the mounted file.
    EnvFirst,
    /// The mounted file wins over the directly set variable.
    FileFirst,
}

/// Reads a required secret with an explicit priority. See
/// [`secret_from_env`] and [`secret_from_env_file_first`].
pub fn secret_from_env_with(name: &str, priority: SecretPriority) -> Result<Secret, ConfigError> {
    let env = std::env::var_os(name);
    let file = std::env::var_os(format!("{name}_FILE"));
    if priority == SecretPriority::FileFirst
        && let Some(path) = &file
    {
        let path = path.to_string_lossy();
        if path.trim().is_empty() {
            return Err(ConfigError::SecretFile {
                name: name.to_owned(),
            });
        }
        return secret_file(&path).map_err(|error| match error {
            ConfigError::Invalid { .. } => ConfigError::SecretFile {
                name: name.to_owned(),
            },
            other => other,
        });
    }
    if let Some(value) = env {
        let value = value.to_string_lossy();
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(ConfigError::EmptySecret {
                name: name.to_owned(),
            });
        }
        return Ok(Secret::new(trimmed));
    }
    if let Some(path) = file {
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

/// `clap` value parser for a Go-style duration (`1s`, `5m`, `1h30m`,
/// `250ms`, `1.5s`).
///
/// The units are the ones accepted by Go's `time.ParseDuration`: `ns`, `us`,
/// `µs`, `μs`, `ms`, `s`, `m` and `h`, combined in any order without
/// separators. `0` is the only unit-less value. Negative durations are
/// rejected because [`Duration`] is non-negative.
pub fn parse_go_duration(value: &str) -> Result<Duration, String> {
    let text = value.trim();
    if text.is_empty() {
        return Err("expected a duration such as 1s or 1h30m".to_owned());
    }
    let unsigned = match text.strip_prefix(['+', '-']) {
        Some(rest) => rest,
        None => text,
    };
    if text.starts_with('-') && !unsigned.is_empty() {
        return Err("duration must not be negative".to_owned());
    }
    if unsigned.is_empty() {
        return Err("expected a duration such as 1s or 1h30m".to_owned());
    }
    if unsigned == "0" {
        return Ok(Duration::ZERO);
    }
    let mut rest = unsigned;
    let mut nanos: u128 = 0;
    while !rest.is_empty() {
        let (int, fraction, tail) =
            split_number(rest).ok_or_else(|| "invalid duration: expected a number".to_owned())?;
        let (unit, tail) = split_unit(tail)
            .ok_or_else(|| "invalid duration: expected a unit (ns, us, ms, s, m, h)".to_owned())?;
        let scale = 10_u128
            .checked_pow(u32::try_from(fraction.len()).map_err(|_| "duration is too large")?)
            .ok_or("duration is too large")?;
        let fract = if fraction.is_empty() {
            0
        } else {
            fraction
                .parse::<u128>()
                .map_err(|_| "invalid duration: bad fraction".to_owned())?
        };
        let part = int
            .checked_mul(unit)
            .and_then(|value| value.checked_add(fract.checked_mul(unit)? / scale))
            .ok_or("duration is too large")?;
        nanos = nanos.checked_add(part).ok_or("duration is too large")?;
        rest = tail;
    }
    let seconds = u64::try_from(nanos / 1_000_000_000).map_err(|_| "duration is too large")?;
    let subsecond = u32::try_from(nanos % 1_000_000_000).expect("nanoseconds fit u32");
    Ok(Duration::new(seconds, subsecond))
}

/// Split the decimal prefix of `text` into its integer and fraction digits.
fn split_number(text: &str) -> Option<(u128, &str, &str)> {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    let int_end = index;
    let fraction = if index < bytes.len() && bytes[index] == b'.' {
        index += 1;
        let start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        &text[start..index]
    } else {
        ""
    };
    if int_end == 0 && fraction.is_empty() {
        return None;
    }
    let int = if int_end == 0 {
        0
    } else {
        text[..int_end].parse().ok()?
    };
    Some((int, fraction, &text[index..]))
}

/// Return the nanosecond multiplier of the Go duration unit at the start of
/// `text` and the remaining text.
fn split_unit(text: &str) -> Option<(u128, &str)> {
    for (unit, nanos) in [
        ("ns", 1_u128),
        ("us", 1_000),
        ("µs", 1_000),
        ("μs", 1_000),
        ("ms", 1_000_000),
        ("s", 1_000_000_000),
        ("m", 60_000_000_000),
        ("h", 3_600_000_000_000),
    ] {
        if let Some(rest) = text.strip_prefix(unit) {
            return Some((nanos, rest));
        }
    }
    None
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
    fn go_durations_match_time_parse_duration() {
        for (text, nanos) in [
            ("0", 0_u64),
            ("1ns", 1),
            ("250ms", 250_000_000),
            ("1s", 1_000_000_000),
            ("5m", 300_000_000_000),
            ("1h30m", 5_400_000_000_000),
            ("1.5s", 1_500_000_000),
            ("2h45m10.5s", 9_910_500_000_000),
            ("1us", 1_000),
            ("1µs", 1_000),
            ("1μs", 1_000),
            ("+30s", 30_000_000_000),
            ("+0", 0),
        ] {
            assert_eq!(
                parse_go_duration(text).expect(text),
                Duration::from_nanos(nanos),
                "{text}"
            );
        }
        for text in [
            "", "+", "-", "1", "-1s", "1x", "1.2.3s", "1sfoo", "ms", ".s",
        ] {
            assert!(parse_go_duration(text).is_err(), "{text:?} must fail");
        }
    }

    #[test]
    fn secret_priority_can_prefer_the_mounted_file() {
        let suffix = std::process::id();
        let env_only = format!("HOME_CONFIG_TEST_ENV_ONLY_{suffix}");
        let env_first = format!("HOME_CONFIG_TEST_ENV_FIRST_{suffix}");
        let file_first = format!("HOME_CONFIG_TEST_FILE_FIRST_{suffix}");
        let path = temp_file("from-file\n");
        let path_text = path.to_str().expect("path").to_owned();

        // SAFETY: the names are unique per test, so no other test reads them.
        unsafe {
            std::env::set_var(&env_only, "from-env");
            std::env::set_var(&env_first, "from-env");
            std::env::set_var(format!("{env_first}_FILE"), &path_text);
            std::env::set_var(&file_first, "from-env");
            std::env::set_var(format!("{file_first}_FILE"), &path_text);
        }

        assert_eq!(
            secret_from_env(&env_only).expect("env").expose(),
            "from-env"
        );
        assert_eq!(
            secret_from_env(&env_first).expect("env first").expose(),
            "from-env"
        );
        assert_eq!(
            secret_from_env_file_first(&file_first)
                .expect("file first")
                .expose(),
            "from-file"
        );

        // A broken file must not fall back to a stale env value.
        unsafe {
            std::env::set_var(format!("{file_first}_FILE"), "/definitely/not/a/secret");
        }
        assert!(matches!(
            secret_from_env_file_first(&file_first),
            Err(ConfigError::SecretFile { .. })
        ));

        for name in [&env_only, &env_first, &file_first] {
            unsafe {
                std::env::remove_var(name);
                std::env::remove_var(format!("{name}_FILE"));
            }
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn log_level_is_case_insensitive_with_info_default() {
        assert_eq!(LogLevel::parse("DEBUG"), LogLevel::Debug);
        assert_eq!(LogLevel::parse("warn"), LogLevel::Warn);
        assert_eq!(LogLevel::parse("nonsense"), LogLevel::Info);
    }
}
