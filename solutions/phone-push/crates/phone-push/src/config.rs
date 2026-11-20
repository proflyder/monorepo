use std::time::Duration;

use home_secret::Token;
use thiserror::Error;

pub const DEFAULT_LISTEN: &str = "0.0.0.0:8095";
pub const DEFAULT_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
pub const DEFAULT_FCM_URL: &str = "https://fcm.googleapis.com";
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
pub const FCM_SCOPE: &str = "https://www.googleapis.com/auth/firebase.messaging";

#[derive(Clone)]
pub struct Config {
    pub listen: String,
    pub token: Token,
    pub scheduler_url: String,
    pub scheduler_token: Token,
    pub service_account: String,
    pub token_url: String,
    pub fcm_url: String,
    pub timeout: Duration,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Config")
            .field("listen", &self.listen)
            .field("scheduler_url", &self.scheduler_url)
            .field("token_url", &self.token_url)
            .field("fcm_url", &self.fcm_url)
            .field("timeout", &self.timeout)
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
}

impl Config {
    pub fn load() -> Result<Self, ConfigError> {
        Self::load_from(&|key| std::env::var(key).ok(), &|path| std::fs::read(path))
    }

    pub fn load_from(
        getenv: &dyn Fn(&str) -> Option<String>,
        read_file: &dyn Fn(&str) -> std::io::Result<Vec<u8>>,
    ) -> Result<Self, ConfigError> {
        let listen = value(getenv, "PUSH_LISTEN").unwrap_or_else(|| DEFAULT_LISTEN.to_owned());
        let token = secret(read_file, getenv, "PUSH_TOKEN_FILE", true)?;
        let scheduler_url = value(getenv, "PUSH_SCHEDULER_URL")
            .ok_or(ConfigError::Required("PUSH_SCHEDULER_URL"))?;
        let scheduler_token = secret(read_file, getenv, "PUSH_SCHEDULER_TOKEN_FILE", true)?;
        let service_account = secret(read_file, getenv, "PUSH_FCM_SERVICE_ACCOUNT_FILE", false)?
            .expose()
            .to_owned();
        let token_url =
            value(getenv, "PUSH_TOKEN_URL").unwrap_or_else(|| DEFAULT_TOKEN_URL.to_owned());
        let fcm_url = value(getenv, "PUSH_FCM_URL").unwrap_or_else(|| DEFAULT_FCM_URL.to_owned());
        let timeout = match value(getenv, "PUSH_TIMEOUT") {
            None => DEFAULT_TIMEOUT,
            Some(raw) => home_config::parse_go_duration(&raw)
                .ok()
                .filter(|duration| {
                    *duration >= Duration::from_millis(500) && *duration <= Duration::from_secs(60)
                })
                .ok_or(ConfigError::Duration("PUSH_TIMEOUT"))?,
        };
        Ok(Self {
            listen,
            token,
            scheduler_url: scheduler_url.trim_end_matches('/').to_owned(),
            scheduler_token,
            service_account,
            token_url,
            fcm_url: fcm_url.trim_end_matches('/').to_owned(),
            timeout,
        })
    }

    pub fn healthcheck_url(&self) -> Result<String, String> {
        let (host, port) = self
            .listen
            .rsplit_once(':')
            .ok_or_else(|| format!("invalid PUSH_LISTEN: {:?}", self.listen))?;
        let host = match host {
            "" | "0.0.0.0" | "::" | "[::]" => "127.0.0.1",
            other => other,
        };
        if port.is_empty() {
            return Err(format!("invalid PUSH_LISTEN: {:?}", self.listen));
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
    trim: bool,
) -> Result<Token, ConfigError> {
    let path = value(getenv, key).ok_or(ConfigError::Required(key))?;
    let data = read_file(&path).map_err(|_| ConfigError::File(key))?;
    let text = String::from_utf8_lossy(&data);
    let value = if trim { text.trim() } else { text.as_ref() };
    if value.is_empty() {
        return Err(ConfigError::Empty(key));
    }
    Ok(Token::new(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| {
            pairs
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    fn read(path: &str) -> std::io::Result<Vec<u8>> {
        match path {
            "/sa" => Ok(b"{\"project_id\":\"example\"}".to_vec()),
            _ => Ok(b"secret\n".to_vec()),
        }
    }

    #[test]
    fn a_complete_environment_loads() {
        let pairs = [
            ("PUSH_TOKEN_FILE", "/token"),
            ("PUSH_SCHEDULER_URL", "http://scheduler:8098/"),
            ("PUSH_SCHEDULER_TOKEN_FILE", "/scheduler"),
            ("PUSH_FCM_SERVICE_ACCOUNT_FILE", "/sa"),
        ];
        let config = Config::load_from(&env(&pairs), &read).expect("config");
        assert_eq!(config.listen, DEFAULT_LISTEN);
        assert_eq!(config.scheduler_url, "http://scheduler:8098");
        assert_eq!(config.timeout, DEFAULT_TIMEOUT);
        assert_eq!(config.service_account, "{\"project_id\":\"example\"}");
        assert!(format!("{config:?}").find("secret").is_none());
    }

    #[test]
    fn a_missing_endpoint_is_named_but_never_printed() {
        let pairs = [("PUSH_TOKEN_FILE", "/token")];
        let error = Config::load_from(&env(&pairs), &read).expect_err("missing scheduler");
        assert!(error.to_string().contains("PUSH_SCHEDULER_URL"), "{error}");
        let pairs = [("PUSH_SCHEDULER_URL", "http://scheduler:8098")];
        let error = Config::load_from(&env(&pairs), &read).expect_err("missing token");
        assert!(error.to_string().contains("PUSH_TOKEN_FILE"), "{error}");
    }

    #[test]
    fn an_out_of_range_timeout_is_rejected() {
        let pairs = [
            ("PUSH_TOKEN_FILE", "/token"),
            ("PUSH_SCHEDULER_URL", "http://scheduler:8098"),
            ("PUSH_SCHEDULER_TOKEN_FILE", "/scheduler"),
            ("PUSH_FCM_SERVICE_ACCOUNT_FILE", "/sa"),
            ("PUSH_TIMEOUT", "1h"),
        ];
        let error = Config::load_from(&env(&pairs), &read).expect_err("timeout");
        assert!(error.to_string().contains("PUSH_TIMEOUT"), "{error}");
    }
}
