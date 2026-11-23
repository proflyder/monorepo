use std::net::SocketAddr;
use std::time::Duration;

use home_config::parse_duration_seconds;
use home_secret::Secret;
use thiserror::Error;

pub const DEFAULT_LISTEN: &str = "127.0.0.1:8099";
pub const DEFAULT_PHONE_ENDPOINT: &str = "phone";
pub const DEFAULT_STATION_ENDPOINT: &str = "station";
pub const DEFAULT_RING_TIMEOUT: Duration = Duration::from_secs(30);
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
pub const DEFAULT_ENDED_RETENTION: Duration = Duration::from_secs(60);
pub const DEFAULT_MAX_CALL: Duration = Duration::from_secs(60 * 60);

#[derive(Clone)]
pub struct EndpointConfig {
    pub id: String,
    pub display_name: String,
    pub capabilities: Vec<String>,
}

impl std::fmt::Debug for EndpointConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EndpointConfig")
            .field("id", &self.id)
            .field("display_name", &self.display_name)
            .field("capabilities", &self.capabilities)
            .finish()
    }
}

#[derive(Clone)]
pub struct IceServerConfig {
    pub urls: Vec<String>,
    pub username: String,
    pub credential: Secret,
}

impl std::fmt::Debug for IceServerConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("IceServerConfig")
            .field("urls", &self.urls)
            .field("username", &self.username)
            .field("credential", &"<redacted>")
            .finish()
    }
}

#[derive(Clone)]
pub struct Config {
    pub listen: String,
    pub phone: EndpointConfig,
    pub station: EndpointConfig,
    pub access_url: String,
    pub service_token: Secret,
    pub station_token: Secret,
    pub ring_timeout: Duration,
    pub connect_timeout: Duration,
    pub ended_retention: Duration,
    pub max_call_duration: Duration,
    pub ice_servers: Vec<IceServerConfig>,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Config")
            .field("listen", &self.listen)
            .field("phone", &self.phone)
            .field("station", &self.station)
            .field("ring_timeout", &self.ring_timeout)
            .field("connect_timeout", &self.connect_timeout)
            .field("ended_retention", &self.ended_retention)
            .field("max_call_duration", &self.max_call_duration)
            .field("ice_servers", &self.ice_servers)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("INTERCOM_STATION_TOKEN or INTERCOM_STATION_TOKEN_FILE is required")]
    StationTokenRequired,
    #[error("INTERCOM_STATION_TOKEN_FILE: cannot read token file")]
    StationTokenFile,
    #[error("{0}")]
    Invalid(String),
}

impl Config {
    pub fn load() -> Result<Self, ConfigError> {
        Self::load_from(&|key| std::env::var(key).ok(), &|path| std::fs::read(path))
    }

    pub fn load_from(
        getenv: &dyn Fn(&str) -> Option<String>,
        read_file: &dyn Fn(&str) -> std::io::Result<Vec<u8>>,
    ) -> Result<Self, ConfigError> {
        let listen =
            first(getenv, &["INTERCOM_LISTEN"]).unwrap_or_else(|| DEFAULT_LISTEN.to_owned());
        let station_token = load_token(
            getenv,
            read_file,
            &["INTERCOM_STATION_TOKEN"],
            &["INTERCOM_STATION_TOKEN_FILE"],
            ConfigError::StationTokenRequired,
            ConfigError::StationTokenFile,
        )?;
        let access_url = first(getenv, &["INTERCOM_ACCESS_URL"])
            .ok_or_else(|| ConfigError::Invalid("INTERCOM_ACCESS_URL is required".into()))?;
        let service_token = load_service_token(getenv, read_file)?;

        let phone = EndpointConfig {
            id: endpoint_id(
                &first(getenv, &["INTERCOM_PHONE_ENDPOINT_ID"])
                    .unwrap_or_else(|| DEFAULT_PHONE_ENDPOINT.to_owned()),
                "INTERCOM_PHONE_ENDPOINT_ID",
            )?,
            display_name: first(getenv, &["INTERCOM_PHONE_DISPLAY_NAME"])
                .unwrap_or_else(|| "Phone".to_owned()),
            capabilities: list(getenv, "INTERCOM_PHONE_CAPABILITIES")
                .unwrap_or_else(|| vec!["audio".to_owned()]),
        };
        let station = EndpointConfig {
            id: endpoint_id(
                &first(getenv, &["INTERCOM_STATION_ENDPOINT_ID"])
                    .unwrap_or_else(|| DEFAULT_STATION_ENDPOINT.to_owned()),
                "INTERCOM_STATION_ENDPOINT_ID",
            )?,
            display_name: first(getenv, &["INTERCOM_STATION_DISPLAY_NAME"])
                .unwrap_or_else(|| "Station".to_owned()),
            capabilities: list(getenv, "INTERCOM_STATION_CAPABILITIES")
                .unwrap_or_else(|| vec!["audio".to_owned()]),
        };
        if phone.id == station.id {
            return Err(ConfigError::Invalid(
                "phone and station endpoint ids must differ".into(),
            ));
        }

        let ring_timeout = duration(
            getenv,
            "INTERCOM_RING_TIMEOUT_SECONDS",
            DEFAULT_RING_TIMEOUT,
        )?;
        let connect_timeout = duration(
            getenv,
            "INTERCOM_CONNECT_TIMEOUT_SECONDS",
            DEFAULT_CONNECT_TIMEOUT,
        )?;
        let ended_retention = duration(
            getenv,
            "INTERCOM_ENDED_RETENTION_SECONDS",
            DEFAULT_ENDED_RETENTION,
        )?;
        let max_call_duration = duration(getenv, "INTERCOM_MAX_CALL_SECONDS", DEFAULT_MAX_CALL)?;
        let ice_servers = ice_config(getenv)?;

        let config = Self {
            listen,
            phone,
            station,
            access_url,
            service_token,
            station_token,
            ring_timeout,
            connect_timeout,
            ended_retention,
            max_call_duration,
            ice_servers,
        };
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        if self.listen.rsplit_once(':').is_none() {
            return Err(ConfigError::Invalid(
                "INTERCOM_LISTEN must be host:port".into(),
            ));
        }
        for (name, value) in [
            ("INTERCOM_RING_TIMEOUT_SECONDS", self.ring_timeout),
            ("INTERCOM_CONNECT_TIMEOUT_SECONDS", self.connect_timeout),
        ] {
            if value.is_zero() || value > Duration::from_secs(300) {
                return Err(ConfigError::Invalid(format!(
                    "{name} must be within 1s..300s"
                )));
            }
        }
        if self.ended_retention < Duration::from_secs(1)
            || self.ended_retention > Duration::from_secs(3600)
        {
            return Err(ConfigError::Invalid(
                "INTERCOM_ENDED_RETENTION_SECONDS must be within 1s..1h".into(),
            ));
        }
        if self.max_call_duration < Duration::from_secs(10)
            || self.max_call_duration > Duration::from_secs(24 * 60 * 60)
        {
            return Err(ConfigError::Invalid(
                "INTERCOM_MAX_CALL_SECONDS must be within 10s..24h".into(),
            ));
        }
        Ok(())
    }

    pub fn healthcheck_address(&self) -> Result<SocketAddr, String> {
        healthcheck_address(&self.listen)
    }
}

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

fn ice_config(
    getenv: &dyn Fn(&str) -> Option<String>,
) -> Result<Vec<IceServerConfig>, ConfigError> {
    let mut servers = Vec::new();
    if let Some(urls) = list(getenv, "INTERCOM_STUN_URLS") {
        servers.push(IceServerConfig {
            urls,
            username: String::new(),
            credential: Secret::new(String::new()),
        });
    }
    if let Some(urls) = list(getenv, "INTERCOM_TURN_URLS") {
        let username = first(getenv, &["INTERCOM_TURN_USERNAME"]).unwrap_or_default();
        let credential = first(getenv, &["INTERCOM_TURN_CREDENTIAL"]).unwrap_or_default();
        if username.is_empty() || credential.is_empty() {
            return Err(ConfigError::Invalid(
                "INTERCOM_TURN_USERNAME and INTERCOM_TURN_CREDENTIAL are required with INTERCOM_TURN_URLS"
                    .into(),
            ));
        }
        servers.push(IceServerConfig {
            urls,
            username,
            credential: Secret::new(credential),
        });
    }
    if servers.is_empty() {
        tracing::info!("no ICE servers configured; clients use host candidates only");
    }
    Ok(servers)
}

fn endpoint_id(value: &str, key: &str) -> Result<String, ConfigError> {
    let value = value.trim();
    let valid = !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if !valid {
        return Err(ConfigError::Invalid(format!(
            "{key} must be 1..64 chars of [A-Za-z0-9._-]"
        )));
    }
    Ok(value.to_owned())
}

fn load_service_token(
    getenv: &dyn Fn(&str) -> Option<String>,
    read_file: &dyn Fn(&str) -> std::io::Result<Vec<u8>>,
) -> Result<Secret, ConfigError> {
    if let Some(token) = first(getenv, &["INTERCOM_SERVICE_TOKEN"]) {
        return Ok(Secret::new(token));
    }
    let Some(path) = first(getenv, &["INTERCOM_SERVICE_TOKEN_FILE"]) else {
        return Err(ConfigError::Invalid(
            "INTERCOM_SERVICE_TOKEN_FILE is required".into(),
        ));
    };
    match read_file(&path) {
        Ok(data) => {
            let token = String::from_utf8_lossy(&data);
            let token = token.trim();
            if token.is_empty() {
                return Err(ConfigError::Invalid(
                    "INTERCOM_SERVICE_TOKEN_FILE is empty".into(),
                ));
            }
            Ok(Secret::new(token))
        }
        Err(_) => Err(ConfigError::Invalid(
            "INTERCOM_SERVICE_TOKEN_FILE: cannot read file".into(),
        )),
    }
}

fn load_token(
    getenv: &dyn Fn(&str) -> Option<String>,
    read_file: &dyn Fn(&str) -> std::io::Result<Vec<u8>>,
    direct_keys: &[&str],
    file_keys: &[&str],
    required: ConfigError,
    unreadable: ConfigError,
) -> Result<Secret, ConfigError> {
    if let Some(token) = first(getenv, direct_keys) {
        return Ok(Secret::new(token));
    }
    let Some(path) = first(getenv, file_keys) else {
        return Err(required);
    };
    let data = read_file(&path).map_err(|_| unreadable)?;
    let token = String::from_utf8_lossy(&data);
    let token = token.trim();
    if token.is_empty() {
        return Err(required);
    }
    Ok(Secret::new(token))
}

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

fn list(getenv: &dyn Fn(&str) -> Option<String>, key: &str) -> Option<Vec<String>> {
    let raw = first(getenv, &[key])?;
    let values: Vec<String> = raw
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect();
    (!values.is_empty()).then_some(values)
}

fn duration(
    getenv: &dyn Fn(&str) -> Option<String>,
    key: &str,
    default: Duration,
) -> Result<Duration, ConfigError> {
    let Some(raw) = first(getenv, &[key]) else {
        return Ok(default);
    };
    parse_duration_seconds(&raw)
        .map_err(|_| ConfigError::Invalid(format!("{key} must be a whole number of seconds")))
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
                .or_else(|| (key == "INTERCOM_SERVICE_TOKEN").then(|| "service".to_owned()))
                .or_else(|| {
                    (key == "INTERCOM_ACCESS_URL")
                        .then(|| "http://scheduler:8098/v1/access/snapshot".to_owned())
                })
        }
    }

    fn no_file(_path: &str) -> std::io::Result<Vec<u8>> {
        Ok(Vec::new())
    }

    #[test]
    fn defaults_and_tokens() {
        let config = Config::load_from(
            &env(&[
                ("INTERCOM_TOKEN", "phone-secret"),
                ("INTERCOM_STATION_TOKEN", "station-secret"),
            ]),
            &no_file,
        )
        .expect("load");
        assert_eq!(config.listen, DEFAULT_LISTEN);
        assert_eq!(config.phone.id, "phone");
        assert_eq!(config.station.id, "station");
        assert_eq!(config.ring_timeout, Duration::from_secs(30));
        assert_eq!(config.connect_timeout, Duration::from_secs(20));
        assert!(config.ice_servers.is_empty());
        assert!(
            !format!("{config:?}").contains("phone-secret"),
            "debug must not print tokens"
        );
        assert!(
            !format!("{config:?}").contains("station-secret"),
            "debug must not print tokens"
        );
    }

    #[test]
    fn the_station_token_is_required() {
        let error = Config::load_from(&env(&[]), &no_file).expect_err("station token");
        assert!(error.to_string().contains("INTERCOM_STATION_TOKEN"));
    }

    #[test]
    fn token_files_are_trimmed_and_direct_values_win() {
        let config = Config::load_from(
            &|key| match key {
                "INTERCOM_SERVICE_TOKEN_FILE" => Some("/run/service".to_owned()),
                "INTERCOM_STATION_TOKEN" => Some("station".to_owned()),
                "INTERCOM_ACCESS_URL" => {
                    Some("http://scheduler:8098/v1/access/snapshot".to_owned())
                }
                _ => None,
            },
            &|path| {
                assert_eq!(path, "/run/service");
                Ok(b"  service-from-file\n".to_vec())
            },
        )
        .expect("load");
        assert_eq!(config.service_token.expose(), "service-from-file");

        let config = Config::load_from(
            &env(&[
                ("INTERCOM_SERVICE_TOKEN", "direct"),
                ("INTERCOM_SERVICE_TOKEN_FILE", "/run/service"),
                ("INTERCOM_STATION_TOKEN", "station"),
            ]),
            &|_| Ok(b"file".to_vec()),
        )
        .expect("load");
        assert_eq!(config.service_token.expose(), "direct");
    }

    #[test]
    fn the_service_token_is_required_and_separate_from_the_phone_token() {
        let error = Config::load_from(
            &|key| {
                (key == "INTERCOM_TOKEN")
                    .then(|| "legacy".to_owned())
                    .or_else(|| (key == "INTERCOM_STATION_TOKEN").then(|| "station".to_owned()))
                    .or_else(|| {
                        (key == "INTERCOM_ACCESS_URL")
                            .then(|| "http://scheduler:8098/v1/access/snapshot".to_owned())
                    })
            },
            &no_file,
        )
        .expect_err("service token");
        assert!(error.to_string().contains("INTERCOM_SERVICE_TOKEN"));
    }

    #[test]
    fn ice_configuration_is_optional_and_validated() {
        let config = Config::load_from(
            &env(&[
                ("INTERCOM_TOKEN", "p"),
                ("INTERCOM_STATION_TOKEN", "s"),
                (
                    "INTERCOM_STUN_URLS",
                    "stun:stun.l.google.com:19302, stun:home.example.com:3478",
                ),
            ]),
            &no_file,
        )
        .expect("load");
        assert_eq!(config.ice_servers.len(), 1);
        assert_eq!(config.ice_servers[0].urls.len(), 2);

        let error = Config::load_from(
            &env(&[
                ("INTERCOM_TOKEN", "p"),
                ("INTERCOM_STATION_TOKEN", "s"),
                ("INTERCOM_TURN_URLS", "turn:turn.example.com:3478"),
            ]),
            &no_file,
        )
        .expect_err("turn credentials");
        assert!(error.to_string().contains("INTERCOM_TURN_USERNAME"));
        assert!(!error.to_string().contains("turn:"));
    }

    #[test]
    fn endpoint_ids_are_validated() {
        let error = Config::load_from(
            &env(&[
                ("INTERCOM_TOKEN", "p"),
                ("INTERCOM_STATION_TOKEN", "s"),
                ("INTERCOM_PHONE_ENDPOINT_ID", "bad id"),
            ]),
            &no_file,
        )
        .expect_err("invalid id");
        assert!(error.to_string().contains("INTERCOM_PHONE_ENDPOINT_ID"));
    }

    #[test]
    fn healthcheck_address_uses_loopback() {
        for (input, want) in [
            ("127.0.0.1:8099", "127.0.0.1:8099"),
            ("0.0.0.0:8099", "127.0.0.1:8099"),
            (":8099", "127.0.0.1:8099"),
            ("[::]:8099", "127.0.0.1:8099"),
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
