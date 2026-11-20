use std::time::Duration;

use futures::future::BoxFuture;
use home_ha::Client as HaClient;
use home_secret::Token;

pub const HA_TIMEOUT: Duration = Duration::from_secs(30);
pub const MAX_CHECK_ERRORS: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckConfig {
    pub valid: bool,
    pub errors: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum HaError {
    #[error("Home Assistant call: {0}")]
    Client(#[from] home_ha::Error),
    #[error("Home Assistant check_config request failed")]
    Transport(#[source] reqwest::Error),
    #[error("Home Assistant check_config returned HTTP {0}")]
    Status(u16),
    #[error("Home Assistant check_config answer is invalid")]
    Decode,
}

pub trait HaApi: Send + Sync {
    fn check_config<'a>(&'a self) -> BoxFuture<'a, Result<CheckConfig, HaError>>;
    fn call_service<'a>(&'a self, service: &'a str) -> BoxFuture<'a, Result<(), HaError>>;
    fn state<'a>(&'a self, entity: &'a str) -> BoxFuture<'a, Result<String, HaError>>;
    fn ping<'a>(&'a self) -> BoxFuture<'a, Result<(), HaError>>;
}

pub struct Ha {
    http: reqwest::Client,
    client: HaClient,
    base_url: String,
    token: Token,
}

impl std::fmt::Debug for Ha {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Ha")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

impl Ha {
    pub fn new(base_url: impl Into<String>, token: Token) -> Result<Self, HaError> {
        let base_url = base_url.into().trim().trim_end_matches('/').to_owned();
        let client =
            HaClient::new(base_url.clone(), token.clone(), HA_TIMEOUT).map_err(HaError::Client)?;
        let http = reqwest::Client::builder()
            .timeout(HA_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(HaError::Transport)?;
        Ok(Self {
            http,
            client,
            base_url,
            token,
        })
    }
}

impl HaApi for Ha {
    fn check_config<'a>(&'a self) -> BoxFuture<'a, Result<CheckConfig, HaError>> {
        Box::pin(async move {
            let response = self
                .http
                .post(format!("{}/api/config/core/check_config", self.base_url))
                .bearer_auth(self.token.expose())
                .header("content-type", "application/json")
                .body("{}")
                .send()
                .await
                .map_err(HaError::Transport)?;
            let status = response.status();
            if !status.is_success() {
                return Err(HaError::Status(status.as_u16()));
            }
            let body = response.bytes().await.map_err(HaError::Transport)?;
            let payload: serde_json::Value =
                serde_json::from_slice(&body).map_err(|_| HaError::Decode)?;
            let result = payload
                .get("result")
                .and_then(serde_json::Value::as_str)
                .ok_or(HaError::Decode)?;
            if result == "valid" {
                return Ok(CheckConfig {
                    valid: true,
                    errors: None,
                });
            }
            let errors = payload
                .get("errors")
                .filter(|value| !value.is_null())
                .map(|value| truncate(&value.to_string(), MAX_CHECK_ERRORS));
            Ok(CheckConfig {
                valid: false,
                errors,
            })
        })
    }

    fn call_service<'a>(&'a self, service: &'a str) -> BoxFuture<'a, Result<(), HaError>> {
        Box::pin(async move {
            let (domain, action) =
                service
                    .split_once('.')
                    .ok_or_else(|| home_ha::Error::Invalid {
                        op: "call service",
                        detail: "expected <domain>.<service>".to_owned(),
                    })?;
            self.client
                .call_service(domain, action, serde_json::json!({}))
                .await
                .map_err(HaError::Client)
        })
    }

    fn state<'a>(&'a self, entity: &'a str) -> BoxFuture<'a, Result<String, HaError>> {
        Box::pin(async move { self.client.state(entity).await.map_err(HaError::Client) })
    }

    fn ping<'a>(&'a self) -> BoxFuture<'a, Result<(), HaError>> {
        Box::pin(async move { self.client.ping().await.map_err(HaError::Client) })
    }
}

fn truncate(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}
