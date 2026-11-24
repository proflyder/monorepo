use std::time::Duration;

use home_secret::Token;
use thiserror::Error;

pub const DEFAULT_LISTEN: &str = "0.0.0.0:8096";
pub const DEFAULT_OPENBAO_URL: &str = "http://openbao.openbao.svc:8200";
pub const DEFAULT_OPENBAO_ROLE: &str = "secrets-gateway";
pub const DEFAULT_SA_TOKEN: &str = "/var/run/secrets/tokens/token";
pub const DEFAULT_STATE_FILE: &str = "/state/step-up.json";
pub const DEFAULT_DEVICE_PATH: &str = "k8s/home-assistant/secrets-gateway-device";
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
pub const DEFAULT_MAX_LIST_ENTRIES: usize = 200;
pub const MAX_LIST_ENTRIES: usize = 1_000;

#[derive(Clone)]
pub struct Config {
    pub listen: String,
    pub scheduler_url: String,
    pub service_token: Token,
    pub openbao_url: String,
    pub openbao_role: String,
    pub sa_token_file: String,
    pub catalog_file: String,
    pub deny_file: String,
    pub state_file: String,
    pub device_path: String,
    pub phone_push_url: String,
    pub phone_push_token: Token,
    pub timeout: Duration,
    pub max_list_entries: usize,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Config")
            .field("listen", &self.listen)
            .field("scheduler_url", &self.scheduler_url)
            .field("openbao_url", &self.openbao_url)
            .field("openbao_role", &self.openbao_role)
            .field("catalog_file", &self.catalog_file)
            .field("deny_file", &self.deny_file)
            .field("state_file", &self.state_file)
            .field("device_path", &self.device_path)
            .field("phone_push_url", &self.phone_push_url)
            .field("timeout", &self.timeout)
            .field("max_list_entries", &self.max_list_entries)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("{0} is required")]
    Required(&'static str),
    #[error("{0}: cannot read file")]
    File(&'static str),
    #[error("{0}: is empty")]
    Empty(&'static str),
    #[error("{0} is not a valid duration")]
    Duration(&'static str),
    #[error("{0} is not a valid number")]
    Number(&'static str),
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
            value(getenv, "SECRETS_GATEWAY_LISTEN").unwrap_or_else(|| DEFAULT_LISTEN.to_owned());
        let scheduler_url = value(getenv, "SECRETS_GATEWAY_SCHEDULER_URL")
            .ok_or(ConfigError::Required("SECRETS_GATEWAY_SCHEDULER_URL"))?;
        let service_token = secret(read_file, getenv, "SECRETS_GATEWAY_SERVICE_TOKEN_FILE")?;
        let openbao_url = value(getenv, "SECRETS_GATEWAY_OPENBAO_URL")
            .unwrap_or_else(|| DEFAULT_OPENBAO_URL.to_owned());
        let openbao_role = value(getenv, "SECRETS_GATEWAY_OPENBAO_ROLE")
            .unwrap_or_else(|| DEFAULT_OPENBAO_ROLE.to_owned());
        let sa_token_file = token_path(read_file, getenv, "SECRETS_GATEWAY_SA_TOKEN_FILE")?;
        let catalog_file = value(getenv, "SECRETS_GATEWAY_CATALOG_FILE")
            .ok_or(ConfigError::Required("SECRETS_GATEWAY_CATALOG_FILE"))?;
        let deny_file = value(getenv, "SECRETS_GATEWAY_DENY_FILE")
            .ok_or(ConfigError::Required("SECRETS_GATEWAY_DENY_FILE"))?;
        let state_file = value(getenv, "SECRETS_GATEWAY_STATE_FILE")
            .unwrap_or_else(|| DEFAULT_STATE_FILE.to_owned());
        let device_path = value(getenv, "SECRETS_GATEWAY_DEVICE_PATH")
            .unwrap_or_else(|| DEFAULT_DEVICE_PATH.to_owned());
        let phone_push_url = value(getenv, "SECRETS_GATEWAY_PHONE_PUSH_URL")
            .ok_or(ConfigError::Required("SECRETS_GATEWAY_PHONE_PUSH_URL"))?;
        let phone_push_token = match value(getenv, "SECRETS_GATEWAY_PHONE_PUSH_TOKEN_FILE") {
            Some(path) => token_file(read_file, &path, "SECRETS_GATEWAY_PHONE_PUSH_TOKEN_FILE")?,
            None => service_token.clone(),
        };
        let timeout = match value(getenv, "SECRETS_GATEWAY_TIMEOUT") {
            None => DEFAULT_TIMEOUT,
            Some(raw) => home_config::parse_go_duration(&raw)
                .ok()
                .filter(|duration| {
                    *duration >= Duration::from_millis(500) && *duration <= Duration::from_secs(60)
                })
                .ok_or(ConfigError::Duration("SECRETS_GATEWAY_TIMEOUT"))?,
        };
        let max_list_entries = match value(getenv, "SECRETS_GATEWAY_MAX_LIST_ENTRIES") {
            None => DEFAULT_MAX_LIST_ENTRIES,
            Some(raw) => raw
                .parse::<usize>()
                .ok()
                .filter(|size| *size > 0 && *size <= MAX_LIST_ENTRIES)
                .ok_or(ConfigError::Number("SECRETS_GATEWAY_MAX_LIST_ENTRIES"))?,
        };
        Ok(Self {
            listen,
            scheduler_url: scheduler_url.trim_end_matches('/').to_owned(),
            service_token,
            openbao_url: openbao_url.trim_end_matches('/').to_owned(),
            openbao_role,
            sa_token_file,
            catalog_file,
            deny_file,
            state_file,
            device_path,
            phone_push_url: phone_push_url.trim_end_matches('/').to_owned(),
            phone_push_token,
            timeout,
            max_list_entries,
        })
    }

    pub fn healthcheck_url(&self) -> Result<String, String> {
        let (host, port) = self
            .listen
            .rsplit_once(':')
            .ok_or_else(|| format!("invalid SECRETS_GATEWAY_LISTEN: {:?}", self.listen))?;
        let host = match host {
            "" | "0.0.0.0" | "::" | "[::]" => "127.0.0.1",
            other => other,
        };
        if port.is_empty() {
            return Err(format!("invalid SECRETS_GATEWAY_LISTEN: {:?}", self.listen));
        }
        Ok(format!("http://{host}:{port}"))
    }
}

fn value(getenv: &dyn Fn(&str) -> Option<String>, key: &str) -> Option<String> {
    let value = getenv(key).unwrap_or_default();
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn secret(
    read_file: &dyn Fn(&str) -> std::io::Result<Vec<u8>>,
    getenv: &dyn Fn(&str) -> Option<String>,
    key: &'static str,
) -> Result<Token, ConfigError> {
    let path = value(getenv, key).ok_or(ConfigError::Required(key))?;
    token_file(read_file, &path, key)
}

fn token_file(
    read_file: &dyn Fn(&str) -> std::io::Result<Vec<u8>>,
    path: &str,
    label: &'static str,
) -> Result<Token, ConfigError> {
    let data = read_file(path).map_err(|_| ConfigError::File(label))?;
    let text = String::from_utf8_lossy(&data);
    let value = text.trim();
    if value.is_empty() {
        return Err(ConfigError::Empty(label));
    }
    Ok(Token::new(value))
}

fn token_path(
    read_file: &dyn Fn(&str) -> std::io::Result<Vec<u8>>,
    getenv: &dyn Fn(&str) -> Option<String>,
    key: &'static str,
) -> Result<String, ConfigError> {
    let path = value(getenv, key).ok_or(ConfigError::Required(key))?;
    let data = read_file(&path).map_err(|_| ConfigError::File(key))?;
    if String::from_utf8_lossy(&data).trim().is_empty() {
        return Err(ConfigError::Empty(key));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| {
            pairs
                .iter()
                .rev()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    fn read(path: &str) -> std::io::Result<Vec<u8>> {
        match path {
            "/push" => Ok(b"push-secret\n".to_vec()),
            "/catalog" => Ok(b"{\"paths\":{}}".to_vec()),
            "/deny" => Ok(b"{\"prefixes\":[]}".to_vec()),
            _ => Ok(b"secret\n".to_vec()),
        }
    }

    fn complete() -> [(&'static str, &'static str); 7] {
        [
            ("SECRETS_GATEWAY_SCHEDULER_URL", "http://scheduler:8098/"),
            ("SECRETS_GATEWAY_SERVICE_TOKEN_FILE", "/service"),
            ("SECRETS_GATEWAY_CATALOG_FILE", "/catalog"),
            ("SECRETS_GATEWAY_DENY_FILE", "/deny"),
            ("SECRETS_GATEWAY_PHONE_PUSH_URL", "http://phone-push:8095/"),
            ("SECRETS_GATEWAY_SA_TOKEN_FILE", "/sa"),
            ("SECRETS_GATEWAY_MAX_LIST_ENTRIES", "100"),
        ]
    }

    #[test]
    fn a_complete_environment_loads() {
        let config = Config::load_from(&env(&complete()), &read).expect("config");
        assert_eq!(config.listen, DEFAULT_LISTEN);
        assert_eq!(config.scheduler_url, "http://scheduler:8098");
        assert_eq!(config.openbao_url, DEFAULT_OPENBAO_URL);
        assert_eq!(config.openbao_role, DEFAULT_OPENBAO_ROLE);
        assert_eq!(config.timeout, DEFAULT_TIMEOUT);
        assert_eq!(config.max_list_entries, 100);
        assert_eq!(config.catalog_file, "/catalog");
        assert_eq!(config.deny_file, "/deny");
        assert_eq!(config.state_file, DEFAULT_STATE_FILE);
        assert_eq!(config.device_path, DEFAULT_DEVICE_PATH);
        assert_eq!(config.sa_token_file, "/sa");
        assert!(config.service_token.ct_eq("secret"));
        assert!(config.phone_push_token.ct_eq("secret"));
        assert!(!format!("{config:?}").contains("secret\n"));
    }

    #[test]
    fn missing_required_values_are_named_without_printing_secrets() {
        let pairs = complete();
        let error = Config::load_from(&env(&pairs[..1]), &read).expect_err("missing service token");
        assert!(
            error
                .to_string()
                .contains("SECRETS_GATEWAY_SERVICE_TOKEN_FILE"),
            "{error}"
        );
    }

    #[test]
    fn the_device_state_path_can_be_overridden() {
        let mut pairs = complete().to_vec();
        pairs.push(("SECRETS_GATEWAY_DEVICE_PATH", "device/test"));
        let config = Config::load_from(&env(&pairs), &read).expect("config");
        assert_eq!(config.device_path, "device/test");
    }

    #[test]
    fn a_separate_phone_push_token_file_overrides_the_shared_token() {
        let mut pairs = complete().to_vec();
        pairs.push(("SECRETS_GATEWAY_PHONE_PUSH_TOKEN_FILE", "/push"));
        let config = Config::load_from(&env(&pairs), &read).expect("config");
        assert!(config.phone_push_token.ct_eq("push-secret"));
        assert_eq!(config.sa_token_file, "/sa");
        assert!(config.service_token.ct_eq("secret"));
    }

    #[test]
    fn out_of_range_values_are_rejected() {
        let mut pairs = complete().to_vec();
        pairs.push(("SECRETS_GATEWAY_TIMEOUT", "1h"));
        let error = Config::load_from(&env(&pairs), &read).expect_err("timeout");
        assert!(
            error.to_string().contains("SECRETS_GATEWAY_TIMEOUT"),
            "{error}"
        );
        let mut pairs = complete().to_vec();
        pairs.push(("SECRETS_GATEWAY_MAX_LIST_ENTRIES", "0"));
        let error = Config::load_from(&env(&pairs), &read).expect_err("list");
        assert!(
            error
                .to_string()
                .contains("SECRETS_GATEWAY_MAX_LIST_ENTRIES"),
            "{error}"
        );
    }
}
