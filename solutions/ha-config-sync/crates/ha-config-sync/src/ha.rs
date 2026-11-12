//! Home Assistant REST calls used by a sync cycle.
//!
//! The shared [`home_ha`] client performs the service calls, entity reads and
//! the reachability probe. `config/core/check_config` has no typed wrapper
//! there yet, so this module keeps one narrow authenticated POST for it. The
//! bearer token never appears in an error or a log line.

use std::time::Duration;

use futures::future::BoxFuture;
use home_ha::Client as HaClient;
use home_secret::Token;

/// Bound for one Home Assistant REST call.
pub const HA_TIMEOUT: Duration = Duration::from_secs(30);
/// Upper bound for the check_config error text kept for the log.
pub const MAX_CHECK_ERRORS: usize = 512;

/// The answer of `config/core/check_config`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckConfig {
    /// `result` was `"valid"`.
    pub valid: bool,
    /// Truncated error text when it was not.
    pub errors: Option<String>,
}

/// A Home Assistant call failure.
#[derive(Debug, thiserror::Error)]
pub enum HaError {
    /// The shared client rejected the call.
    #[error("Home Assistant call: {0}")]
    Client(#[from] home_ha::Error),
    /// The custom check_config request failed at the transport layer.
    #[error("Home Assistant check_config request failed")]
    Transport(#[source] reqwest::Error),
    /// check_config returned a non-2xx status.
    #[error("Home Assistant check_config returned HTTP {0}")]
    Status(u16),
    /// check_config returned a body without a result.
    #[error("Home Assistant check_config answer is invalid")]
    Decode,
}

/// The calls a cycle needs.
pub trait HaApi: Send + Sync {
    /// Ask Home Assistant to validate its configuration.
    fn check_config<'a>(&'a self) -> BoxFuture<'a, Result<CheckConfig, HaError>>;
    /// Call `<domain>.<service>` with an empty body.
    fn call_service<'a>(&'a self, service: &'a str) -> BoxFuture<'a, Result<(), HaError>>;
    /// Read one entity state.
    fn state<'a>(&'a self, entity: &'a str) -> BoxFuture<'a, Result<String, HaError>>;
    /// Probe `/api/`.
    fn ping<'a>(&'a self) -> BoxFuture<'a, Result<(), HaError>>;
}

/// The production client.
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
    /// Build a client for one Home Assistant instance.
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
