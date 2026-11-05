//! HTTPS-клиент обновлений на reqwest: rustls с провайдером ring и корнями из CA-бандла
//! прошивки. Только HTTP/1; тело читается в память с пределом. Ошибка — типизированная,
//! статус ответа отделён от транспорта.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use rustls::RootCertStore;
use rustls_pki_types::CertificateDer;
use rustls_pki_types::pem::PemObject as _;
use tokio_util::sync::CancellationToken;

use crate::error::UpdateError;

/// CA-бандл прошивки (обе модели станции).
pub const CA_BUNDLE: &str = "/etc/ssl/certs/ca-certificates.crt";

/// На адрес хоста.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// На одно чтение тела (прежний SO_RCVTIMEO).
const READ_TIMEOUT: Duration = Duration::from_secs(20);
/// Столько редиректов проходит клиент (ассеты GHCR уходят на подписанный хост).
const REDIRECTS: usize = 5;

/// Ошибка одного GET.
#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("cancelled")]
    Cancelled,
    /// Сеть, TLS или DNS.
    #[error("{0}")]
    Transport(String),
    #[error("response too large")]
    TooLarge,
    /// Не-2xx ответ.
    #[error("HTTP {0}")]
    Status(u16),
}

impl HttpError {
    /// `context: <текст>` для Home Assistant; отмена остаётся отменой.
    pub fn with_context(self, context: &str) -> UpdateError {
        match self {
            Self::Cancelled => UpdateError::Cancelled,
            other => UpdateError::retry(format!("{context}: {other}")),
        }
    }
}

/// Прогресс загрузки: `(получено, всего)`; всего 0, когда длина неизвестна.
pub type Progress<'a> = &'a mut dyn FnMut(usize, usize);

/// Клиент с корнями из PEM-бандла и заголовком `User-Agent`.
pub fn client(ca_bundle: &Path, user_agent: &str) -> Result<reqwest::Client, String> {
    let tls = tls_config(ca_bundle)?;
    reqwest::Client::builder()
        .use_preconfigured_tls(tls)
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT)
        .redirect(reqwest::redirect::Policy::limited(REDIRECTS))
        .user_agent(user_agent)
        .build()
        .map_err(|e| format!("TLS setup: {e}"))
}

/// TLS 1.2/1.3, проверка имени хоста; сертификаты бандла, которые webpki не разбирает,
/// пропускаются (как у загрузчика OpenSSL).
fn tls_config(ca_bundle: &Path) -> Result<rustls::ClientConfig, String> {
    let fail = || format!("cannot load {}", ca_bundle.display());
    let pem = std::fs::read(ca_bundle).map_err(|_| fail())?;
    let mut roots = RootCertStore::empty();
    let (added, _) = roots
        .add_parsable_certificates(CertificateDer::pem_slice_iter(&pem).filter_map(Result::ok));
    if added == 0 {
        return Err(fail());
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| format!("TLS setup: {e}"))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(config)
}

/// GET тела успешного ответа; не-2xx — `HttpError::Status`.
pub async fn get(
    client: &reqwest::Client,
    url: &str,
    accept: Option<&str>,
    bearer: Option<&str>,
    max_bytes: usize,
    progress: Option<Progress<'_>>,
    cancel: &CancellationToken,
) -> Result<Vec<u8>, HttpError> {
    let fetch = fetch(client, url, accept, bearer, max_bytes, progress);
    tokio::select! {
        biased;
        () = cancel.cancelled() => Err(HttpError::Cancelled),
        response = fetch => response,
    }
}

async fn fetch(
    client: &reqwest::Client,
    url: &str,
    accept: Option<&str>,
    bearer: Option<&str>,
    max_bytes: usize,
    mut progress: Option<Progress<'_>>,
) -> Result<Vec<u8>, HttpError> {
    let mut request = client.get(url);
    if let Some(accept) = accept {
        request = request.header(reqwest::header::ACCEPT, accept);
    }
    if let Some(token) = bearer {
        request = request.bearer_auth(token);
    }
    let mut response = request
        .send()
        .await
        .map_err(|e| HttpError::Transport(transport(&e)))?;
    let status = response.status();
    if !status.is_success() {
        return Err(HttpError::Status(status.as_u16()));
    }
    let total = response
        .content_length()
        .and_then(|len| usize::try_from(len).ok())
        .unwrap_or(0);
    if total > max_bytes {
        return Err(HttpError::TooLarge);
    }
    let mut body = Vec::new();
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                if body.len().saturating_add(chunk.len()) > max_bytes {
                    return Err(HttpError::TooLarge);
                }
                body.extend_from_slice(&chunk);
                if let Some(callback) = progress.as_deref_mut() {
                    callback(body.len(), total);
                }
            }
            Ok(None) => break,
            Err(e) => return Err(HttpError::Transport(transport(&e))),
        }
    }
    Ok(body)
}

/// Текст транспортной ошибки без URL: прежний клиент тоже не печатал адрес.
fn transport(error: &reqwest::Error) -> String {
    if error.is_connect() && error.is_timeout() {
        "Connection timed out".into()
    } else if error.is_timeout() {
        "Resource temporarily unavailable".into()
    } else {
        std::error::Error::source(error).map_or_else(|| error.to_string(), ToString::to_string)
    }
}
