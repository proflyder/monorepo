use std::time::Duration;

pub mod alertmanager;
pub mod loki;
pub mod prometheus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Transient,
    Permanent,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{operation}: {detail}")]
    Invalid {
        operation: &'static str,
        detail: &'static str,
    },
    #[error("{operation}: returned HTTP {status}")]
    Status {
        operation: &'static str,
        status: u16,
    },
    #[error("{operation}: failed: {detail}")]
    Api {
        operation: &'static str,
        detail: String,
    },
    #[error("{operation}: request failed")]
    Transport {
        operation: &'static str,
        #[source]
        source: reqwest::Error,
    },
    #[error("{operation}: invalid JSON response")]
    Decode {
        operation: &'static str,
        #[source]
        source: serde_json::Error,
    },
    #[error("{operation}: public TLS roots: {source}")]
    Tls {
        operation: &'static str,
        #[source]
        source: home_tls::Error,
    },
}

impl Error {
    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::Transport { .. } => ErrorKind::Transient,
            Self::Status { status, .. } => match *status {
                408 | 425 | 429 => ErrorKind::Transient,
                status if status >= 500 => ErrorKind::Transient,
                _ => ErrorKind::Permanent,
            },
            Self::Invalid { .. } | Self::Api { .. } | Self::Decode { .. } | Self::Tls { .. } => {
                ErrorKind::Permanent
            }
        }
    }

    #[must_use]
    pub fn is_transient(&self) -> bool {
        self.kind() == ErrorKind::Transient
    }
}

pub(crate) fn build_http(
    operation: &'static str,
    timeout: Duration,
) -> Result<reqwest::Client, Error> {
    home_tls::public_https_client()
        .map_err(|source| Error::Tls { operation, source })?
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|source| Error::Transport { operation, source })
}

pub(crate) fn normalize_base_url(base_url: &str) -> String {
    base_url.trim().trim_end_matches('/').to_owned()
}

pub(crate) fn decode<T: serde::de::DeserializeOwned>(
    operation: &'static str,
    body: &[u8],
    cap: usize,
) -> Result<T, Error> {
    let body = &body[..body.len().min(cap)];
    serde_json::from_slice(body).map_err(|source| Error::Decode { operation, source })
}
