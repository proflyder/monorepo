use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::Value;

use super::model::{MessagePage, MessageRow, SessionList};
use crate::tools::BoxFuture;

pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryError {
    RelayUnreachable,
    HermesFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Health {
    Up { version: String },
    HermesDown,
    RelayDown,
}

pub trait HistoryApi: Send + Sync {
    fn health(&self) -> BoxFuture<'_, Health>;

    fn sessions<'a>(&'a self, source: &'a str) -> BoxFuture<'a, Result<SessionList, HistoryError>>;

    fn messages<'a>(
        &'a self,
        session: &'a str,
    ) -> BoxFuture<'a, Result<Vec<MessageRow>, HistoryError>>;
}

pub struct RelayClient {
    http: reqwest::Client,
    base_url: String,
    token: String,
}

impl std::fmt::Debug for RelayClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("hermes::RelayClient")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

impl RelayClient {
    pub fn new(base_url: &str, token: String) -> Result<Self, String> {
        let _already_installed = rustls::crypto::ring::default_provider().install_default();
        let http = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|error| format!("hermes relay client: {error}"))?;
        Ok(Self {
            http,
            base_url: base_url.trim().trim_end_matches('/').to_owned(),
            token,
        })
    }

    pub(super) async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, HistoryError> {
        let response = self
            .http
            .get(format!("{}{path}", self.base_url))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|_| HistoryError::RelayUnreachable)?;
        if !response.status().is_success() {
            return Err(HistoryError::HermesFailed);
        }
        let body = response
            .bytes()
            .await
            .map_err(|_| HistoryError::HermesFailed)?;
        serde_json::from_slice(&body).map_err(|_| HistoryError::HermesFailed)
    }
}

impl HistoryApi for RelayClient {
    fn health(&self) -> BoxFuture<'_, Health> {
        Box::pin(async move {
            match self.get::<Value>("/v1/health").await {
                Ok(body) => Health::Up {
                    version: body
                        .get("version")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                },
                Err(HistoryError::HermesFailed) => Health::HermesDown,
                Err(HistoryError::RelayUnreachable) => Health::RelayDown,
            }
        })
    }

    fn sessions<'a>(&'a self, source: &'a str) -> BoxFuture<'a, Result<SessionList, HistoryError>> {
        Box::pin(async move { self.get(&format!("/api/sessions?source={source}")).await })
    }

    fn messages<'a>(
        &'a self,
        session: &'a str,
    ) -> BoxFuture<'a, Result<Vec<MessageRow>, HistoryError>> {
        Box::pin(async move {
            let page: MessagePage = self
                .get(&format!("/api/sessions/{session}/messages"))
                .await?;
            Ok(page.data)
        })
    }
}
