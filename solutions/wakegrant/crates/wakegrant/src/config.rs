use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use clap::Parser;
pub use home_config::LogLevel;
use home_config::{parse_listen, secret_from_env, u64_in_range};
use home_secret::Secret;
use thiserror::Error;
use wakegrant_protocol::{b64_decode, is_valid_machine};

pub const DEFAULT_LISTEN: &str = "127.0.0.1:8099";
pub const DEFAULT_DB_PATH: &str = "wakegrant.db";
pub const KEY_SIZE: usize = 32;

pub type Keys = BTreeMap<String, Vec<u8>>;

#[derive(Debug, Clone, Parser)]
#[command(name = "wakegrant", version, disable_help_subcommand = true)]
pub struct Settings {
    #[arg(
        long,
        env = "WAKE_GRANT_LISTEN",
        default_value = DEFAULT_LISTEN,
        value_parser = parse_listen
    )]
    pub listen: SocketAddr,
    #[arg(long, env = "WAKE_GRANT_DB_PATH", default_value = DEFAULT_DB_PATH)]
    pub db_path: PathBuf,
    #[arg(
        long,
        env = "WAKE_GRANT_MAX_TTL_SECONDS",
        default_value = "600",
        value_parser = u64_in_range(30, 600)
    )]
    pub max_ttl_seconds: u64,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub listen: SocketAddr,
    pub db_path: PathBuf,
    pub issuer_token: Secret,
    pub machines: Keys,
    pub max_ttl: std::time::Duration,
    pub log_level: LogLevel,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error(transparent)]
    Cli(#[from] clap::Error),
    #[error(transparent)]
    Secret(#[from] home_config::ConfigError),
    #[error("WAKE_GRANT_MACHINES_FILE is required")]
    MissingMachinesFile,
    #[error("WAKE_GRANT_MACHINES_FILE: cannot read machines file")]
    ReadMachines,
    #[error("WAKE_GRANT_MACHINES_FILE: {0}")]
    Machines(#[from] MachinesError),
}

#[derive(Debug, Error)]
pub enum MachinesError {
    #[error("machines file is empty")]
    Empty,
    #[error("machines file is not valid JSON")]
    Json,
    #[error("line {line}: expected machine=base64key")]
    Line { line: usize },
    #[error("invalid machine id {0:?}")]
    Machine(String),
    #[error("machine {0:?} key is not valid base64")]
    KeyEncoding(String),
    #[error("machine {0:?} key must be {KEY_SIZE} bytes")]
    KeySize(String),
    #[error("duplicate machine {0:?}")]
    Duplicate(String),
    #[error("machines file contains no machines")]
    NoMachines,
}

impl Config {
    pub fn load() -> Result<Self, ConfigError> {
        let program = std::env::args_os()
            .next()
            .unwrap_or_else(|| "wakegrant".into());
        let settings = Settings::try_parse_from([program])?;

        let issuer_token = secret_from_env("WAKE_GRANT_ISSUER_TOKEN")?;

        let machines_path =
            std::env::var_os("WAKE_GRANT_MACHINES_FILE").ok_or(ConfigError::MissingMachinesFile)?;
        let machines_path = PathBuf::from(machines_path);
        let machines = load_machines(&machines_path)?;

        let log_level = LogLevel::parse(&std::env::var("LOG_LEVEL").unwrap_or_default());
        Ok(Self {
            listen: settings.listen,
            db_path: settings.db_path,
            issuer_token,
            machines,
            max_ttl: std::time::Duration::from_secs(settings.max_ttl_seconds),
            log_level,
        })
    }

    #[must_use]
    pub fn healthcheck_url(&self) -> String {
        let mut address = self.listen;
        if address.ip().is_unspecified() {
            address.set_ip(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
        }
        format!("http://{address}")
    }
}

pub fn load_machines(path: &Path) -> Result<Keys, ConfigError> {
    let data = std::fs::read(path).map_err(|_| ConfigError::ReadMachines)?;
    Ok(parse_machines(&data)?)
}

pub fn parse_machines(data: &[u8]) -> Result<Keys, MachinesError> {
    let text = String::from_utf8_lossy(data);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(MachinesError::Empty);
    }
    let mut keys = Keys::new();
    if trimmed.starts_with('{') {
        let raw: BTreeMap<String, String> =
            serde_json::from_str(trimmed).map_err(|_| MachinesError::Json)?;
        for (machine, encoded) in raw {
            add_key(&mut keys, &machine, &encoded)?;
        }
    } else {
        for (index, line) in trimmed.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (machine, encoded) =
                split_record(line).ok_or(MachinesError::Line { line: index + 1 })?;
            add_key(&mut keys, machine, encoded)?;
        }
    }
    if keys.is_empty() {
        return Err(MachinesError::NoMachines);
    }
    Ok(keys)
}

fn split_record(line: &str) -> Option<(&str, &str)> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() == 2 && is_valid_machine(fields[0]) {
        return Some((fields[0], fields[1]));
    }
    let (machine, encoded) = line.split_once('=')?;
    Some((machine.trim(), encoded.trim()))
}

fn add_key(keys: &mut Keys, machine: &str, encoded: &str) -> Result<(), MachinesError> {
    if !is_valid_machine(machine) {
        return Err(MachinesError::Machine(machine.to_owned()));
    }
    let key =
        b64_decode(encoded.trim()).map_err(|_| MachinesError::KeyEncoding(machine.to_owned()))?;
    if key.len() != KEY_SIZE {
        return Err(MachinesError::KeySize(machine.to_owned()));
    }
    if keys.insert(machine.to_owned(), key).is_some() {
        return Err(MachinesError::Duplicate(machine.to_owned()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> String {
        wakegrant_protocol::b64_encode(&[7u8; KEY_SIZE])
    }

    #[test]
    fn machines_file_accepts_json_and_line_records() {
        let encoded = key();
        let json = format!("{{\"komp\":\"{encoded}\"}}");
        let keys = parse_machines(json.as_bytes()).expect("json");
        assert_eq!(keys["komp"], vec![7u8; KEY_SIZE]);

        let lines = format!("# comment\n\nkomp={encoded}\ndesk {encoded}\n");
        let keys = parse_machines(lines.as_bytes()).expect("lines");
        assert_eq!(keys.len(), 2);
        assert!(keys.contains_key("desk"));
    }

    #[test]
    fn truncated_or_invalid_keys_fail_before_start() {
        for body in [
            String::new(),
            "{}".to_owned(),
            "komp=c2hvcnQ=".to_owned(),
            "Komp=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".to_owned(),
            format!("komp={}\nkomp={}", key(), key()),
            "komp=!!!!".to_owned(),
        ] {
            assert!(parse_machines(body.as_bytes()).is_err(), "{body:?}");
        }
    }

    #[test]
    fn healthcheck_url_uses_loopback_for_wildcard_binds() {
        let config = Config {
            listen: "0.0.0.0:8099".parse().expect("addr"),
            db_path: PathBuf::from("x"),
            issuer_token: Secret::new("token"),
            machines: Keys::new(),
            max_ttl: std::time::Duration::from_secs(600),
            log_level: LogLevel::Info,
        };
        assert_eq!(config.healthcheck_url(), "http://127.0.0.1:8099");
        let mut ipv6 = config;
        ipv6.listen = "[::]:8099".parse().expect("addr");
        assert_eq!(ipv6.healthcheck_url(), "http://127.0.0.1:8099");
    }
}
