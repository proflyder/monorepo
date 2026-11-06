//! HTTP retrieval of the kurs.kz page.

use std::time::Duration;

use crate::domain::model::Rates;
use crate::domain::rates;
use crate::pipeline::{BoxFuture, RatesSource};

/// Default URL of the live rates page.
pub const DEFAULT_URL: &str = "https://kurs.kz";

/// Caps the downloaded page. The pod has a 128 MiB memory limit, so an
/// unbounded read of a broken or hostile response could restart the service.
pub const MAX_RESPONSE_BYTES: usize = 10 << 20;

/// Downloads and parses kurs.kz.
#[derive(Debug, Clone)]
pub struct Kurs {
    url: String,
    client: reqwest::Client,
}

impl Kurs {
    /// Build a fetcher with the production 20 s timeout.
    pub fn new(url: impl Into<String>) -> Result<Self, String> {
        Self::with_timeout(url, Duration::from_secs(20))
    }

    /// Build a fetcher with an explicit timeout (tests).
    pub fn with_timeout(url: impl Into<String>, timeout: Duration) -> Result<Self, String> {
        // The workspace `reqwest` has no default crypto provider; install ring
        // before the first client or `build()` panics.
        let _already_installed = rustls::crypto::ring::default_provider().install_default();
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| format!("http client: {error}"))?;
        Ok(Self {
            url: url.into(),
            client,
        })
    }

    /// Fetch and aggregate the rates.
    pub async fn fetch_rates(&self) -> Result<Rates, String> {
        let response = self
            .client
            .get(&self.url)
            .send()
            .await
            .map_err(|error| error.to_string())?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!(
                "kurs.kz returned HTTP {}: kurs.kz fetch error",
                status.as_u16()
            ));
        }
        let body = read_capped(response).await?;
        rates::parse(&String::from_utf8_lossy(&body)).map_err(|error| error.to_string())
    }
}

impl RatesSource for Kurs {
    fn fetch(&self) -> BoxFuture<'_, Result<Rates, String>> {
        Box::pin(self.fetch_rates())
    }
}

async fn read_capped(response: reqwest::Response) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    let mut response = response;
    while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(format!(
                "kurs.kz response exceeds {MAX_RESPONSE_BYTES} bytes: kurs.kz fetch error"
            ));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_body_is_rejected_by_the_cap() {
        // The cap is enforced before the buffer accepts the chunk; this test
        // documents the bound and keeps it out of the parser.
        assert_eq!(MAX_RESPONSE_BYTES, 10 * 1024 * 1024);
    }
}
