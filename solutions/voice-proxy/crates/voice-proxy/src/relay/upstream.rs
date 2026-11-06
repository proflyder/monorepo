//! The upstream half: a small OpenAI-compatible passthrough client plus the
//! error normalisation HA's `llama_cpp` adapter requires.
//!
//! There is no authentication of our own: the client's `Authorization` header
//! is forwarded verbatim, because the proxy is not an auth boundary (the
//! nftables rule in front of the Pod is).

use std::time::Duration;

use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderValue};
use reqwest::{Method, StatusCode};

/// The request body cap (10 MiB).
pub const MAX_BODY: usize = 10 << 20;

/// The upstream timeout for the response headers and for every idle read.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(180);

/// One buffered upstream response.
#[derive(Debug, Clone)]
pub struct Fetch {
    pub status: StatusCode,
    pub body: Vec<u8>,
    pub content_type: String,
}

/// Why an upstream request failed.
#[derive(Debug, thiserror::Error)]
pub enum UpstreamError {
    #[error("upstream request failed: {0}")]
    Request(#[source] reqwest::Error),
    #[error("upstream request timed out")]
    Timeout,
    #[error("upstream body could not be read: {0}")]
    Body(#[source] reqwest::Error),
}

/// Talks to the OpenAI-compatible upstream.
#[derive(Clone)]
pub struct Upstream {
    base: String,
    timeout: Duration,
    client: reqwest::Client,
}

impl std::fmt::Debug for Upstream {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Upstream")
            .field("base", &self.base)
            .field("timeout", &self.timeout)
            .finish()
    }
}

impl Upstream {
    /// Builds a client. The workspace `reqwest` has rustls without a default
    /// crypto provider, so the process-wide ring provider is installed first.
    ///
    /// # Errors
    ///
    /// Returns the builder error for an invalid client configuration.
    pub fn new(base_url: &str, timeout: Duration) -> Result<Self, UpstreamError> {
        let _already_installed = rustls::crypto::ring::default_provider().install_default();
        let timeout = if timeout.is_zero() {
            DEFAULT_TIMEOUT
        } else {
            timeout
        };
        let client = reqwest::Client::builder()
            // Bounds the wait for response headers and every gap between body
            // chunks, so a stalled stream fails while a long turn keeps going.
            .read_timeout(timeout)
            .build()
            .map_err(UpstreamError::Request)?;
        Ok(Self {
            base: base_url.trim().trim_end_matches('/').to_owned(),
            timeout,
            client,
        })
    }

    /// The normalised upstream base URL.
    #[must_use]
    pub fn base(&self) -> &str {
        &self.base
    }

    /// Performs one buffered request. A non-2xx status is not an error: the
    /// caller relays it and normalises the body.
    ///
    /// # Errors
    ///
    /// Returns a transport or timeout failure.
    pub async fn buffered(
        &self,
        method: Method,
        path_and_query: &str,
        body: Option<Vec<u8>>,
        authorization: Option<HeaderValue>,
        content_type: Option<&str>,
        accept: Option<&str>,
    ) -> Result<Fetch, UpstreamError> {
        let mut request = self
            .client
            .request(method, format!("{}{path_and_query}", self.base));
        request = request
            .header(CONTENT_TYPE, content_type.unwrap_or("application/json"))
            .header(ACCEPT, accept.unwrap_or("application/json"));
        if let Some(value) = authorization {
            request = request.header(AUTHORIZATION, value);
        }
        if let Some(bytes) = body {
            request = request.body(bytes);
        }
        let response = tokio::time::timeout(self.timeout, request.send())
            .await
            .map_err(|_| UpstreamError::Timeout)?
            .map_err(UpstreamError::Request)?;
        let status = response.status();
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("application/json")
            .to_owned();
        let body = tokio::time::timeout(self.timeout, response.bytes())
            .await
            .map_err(|_| UpstreamError::Timeout)?
            .map_err(UpstreamError::Body)?
            .to_vec();
        Ok(Fetch {
            status,
            body,
            content_type,
        })
    }

    /// Opens a streaming request and leaves the response body to the caller,
    /// who must drop it: dropping the body also cancels the upstream turn.
    ///
    /// # Errors
    ///
    /// Returns a transport or timeout failure while waiting for the headers.
    pub async fn open_stream(
        &self,
        path_and_query: &str,
        body: Vec<u8>,
        authorization: Option<HeaderValue>,
    ) -> Result<reqwest::Response, UpstreamError> {
        let mut request = self
            .client
            .post(format!("{}{path_and_query}", self.base))
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "text/event-stream")
            .body(body);
        if let Some(value) = authorization {
            request = request.header(AUTHORIZATION, value);
        }
        tokio::time::timeout(self.timeout, request.send())
            .await
            .map_err(|_| UpstreamError::Timeout)?
            .map_err(UpstreamError::Request)
    }

    /// The per-read timeout, also used by the relay loop.
    #[must_use]
    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use axum::Router;
    use axum::http::StatusCode;
    use axum::routing::get;

    use super::{Upstream, UpstreamError};

    async fn serve(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        format!("http://{address}")
    }

    #[tokio::test]
    async fn buffered_forwards_method_path_and_authorization() {
        async fn echo(
            headers: axum::http::HeaderMap,
            uri: axum::http::Uri,
        ) -> (StatusCode, String) {
            let auth = headers
                .get("authorization")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("");
            (StatusCode::OK, format!("{} {auth}", uri.path()))
        }
        let base = serve(Router::new().route("/v1/models", get(echo))).await;
        let upstream = Upstream::new(&base, Duration::from_secs(2)).expect("client");
        let response = upstream
            .buffered(
                reqwest::Method::GET,
                "/v1/models",
                None,
                Some(reqwest::header::HeaderValue::from_static("Bearer caller")),
                None,
                None,
            )
            .await
            .expect("response");
        assert_eq!(response.status, StatusCode::OK);
        assert_eq!(response.body, b"/v1/models Bearer caller");
        assert_eq!(response.content_type, "text/plain; charset=utf-8");
    }

    #[tokio::test]
    async fn a_stalled_upstream_times_out() {
        async fn slow() -> &'static str {
            tokio::time::sleep(Duration::from_secs(5)).await;
            "late"
        }
        let base = serve(Router::new().route("/v1/models", get(slow))).await;
        let upstream = Upstream::new(&base, Duration::from_millis(100)).expect("client");
        let error = upstream
            .buffered(reqwest::Method::GET, "/v1/models", None, None, None, None)
            .await
            .expect_err("timeout");
        assert!(matches!(
            error,
            UpstreamError::Timeout | UpstreamError::Request(_)
        ));
    }
}
