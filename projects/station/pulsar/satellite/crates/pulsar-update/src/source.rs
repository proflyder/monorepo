//! Источник релизов: OCI-образ в GHCR. Анонимный pull-token, манифест `latest`, слои по
//! аннотации `org.opencontainers.image.title`.

use std::path::PathBuf;

use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::error::UpdateError;
use crate::http::{self, HttpError, Progress};
use crate::release::{Digest, Layer, Release};
use crate::version::Version;

const OCI_MANIFEST: &str = "application/vnd.oci.image.manifest.v1+json";
const MAX_SMALL: usize = 1 << 20;

/// Настройки OCI-источника.
pub struct OciConfig {
    /// `owner/name` или `ghcr.io/owner/name`.
    pub(crate) repo: String,
    /// `aarch64`, `armhf` или `host` (тесты): суффикс `pulsar-<arch>.bin`.
    pub(crate) arch: &'static str,
    pub(crate) user_agent: String,
    pub(crate) ca_bundle: PathBuf,
    /// `https://ghcr.io/` или локальный тестовый сервер.
    pub(crate) origin: String,
}

impl OciConfig {
    /// Реестр, CA-бандл прошивки и `User-Agent` по умолчанию.
    #[must_use]
    pub fn ghcr(repo: String, current_version: &str, arch: &'static str) -> Self {
        Self {
            repo,
            arch,
            user_agent: format!("pulsar/{current_version}"),
            ca_bundle: http::CA_BUNDLE.into(),
            origin: "https://ghcr.io/".into(),
        }
    }
}

/// Порт источника релизов. Публичный ради `update_probe`; актор станции `!Send`, поэтому
/// трейт не требует `Send`-футур и не используется как `dyn`.
#[allow(async_fn_in_trait)]
pub trait ReleaseSource {
    /// Последний стабильный релиз.
    async fn latest(&self, cancel: &CancellationToken) -> Result<Release, UpdateError>;

    /// Слой релиза целиком; `None` — ассета нет (HTTP 404).
    async fn download(
        &self,
        layer: &Layer,
        max_bytes: usize,
        progress: Option<Progress<'_>>,
        cancel: &CancellationToken,
    ) -> Result<Option<Vec<u8>>, UpdateError>;
}

/// Источник в состоянии «не настроился»: ошибка отдаётся на каждый запрос, как раньше
/// ошибка загрузки CA.
pub struct Broken(UpdateError);

/// Диспетчер источников без `dyn`.
pub enum Source {
    Oci(Oci),
    Broken(Broken),
}

impl Source {
    pub(crate) fn new(config: &OciConfig) -> Self {
        match Oci::new(config) {
            Ok(source) => Self::Oci(source),
            Err(error) => Self::Broken(Broken(error)),
        }
    }
}

impl ReleaseSource for Source {
    async fn latest(&self, cancel: &CancellationToken) -> Result<Release, UpdateError> {
        match self {
            Self::Oci(source) => source.latest(cancel).await,
            Self::Broken(source) => Err(source.0.clone()),
        }
    }

    async fn download(
        &self,
        layer: &Layer,
        max_bytes: usize,
        progress: Option<Progress<'_>>,
        cancel: &CancellationToken,
    ) -> Result<Option<Vec<u8>>, UpdateError> {
        match self {
            Self::Oci(source) => source.download(layer, max_bytes, progress, cancel).await,
            Self::Broken(source) => Err(source.0.clone()),
        }
    }
}

pub struct Oci {
    repo: String,
    arch: &'static str,
    client: reqwest::Client,
    origin: String,
}

impl Oci {
    pub fn new(config: &OciConfig) -> Result<Self, UpdateError> {
        // Константа демона задаёт реестр: `ghcr.io/owner/name`.
        let repo = config.repo.strip_prefix("ghcr.io/").unwrap_or(&config.repo);
        if !safe_repo(repo) {
            return Err(UpdateError::retry(format!(
                "unexpected OCI repository {}",
                config.repo
            )));
        }
        let client =
            http::client(&config.ca_bundle, &config.user_agent).map_err(UpdateError::retry)?;
        Ok(Self {
            repo: repo.to_owned(),
            arch: config.arch,
            client,
            origin: config.origin.clone(),
        })
    }

    /// Анонимный pull-token; токены GHCR живут минуты, поэтому берётся на каждую загрузку.
    async fn token(&self, cancel: &CancellationToken) -> Result<String, UpdateError> {
        let url = format!(
            "{}token?scope=repository:{}:pull&service=ghcr.io",
            self.origin, self.repo
        );
        let body = http::get(&self.client, &url, None, None, MAX_SMALL, None, cancel)
            .await
            .map_err(|e| e.with_context("ghcr.io token"))?;
        parse_token(&body).map_err(UpdateError::retry)
    }

    async fn manifest(&self, cancel: &CancellationToken) -> Result<Release, UpdateError> {
        let token = self.token(cancel).await?;
        let url = format!("{}v2/{}/manifests/latest", self.origin, self.repo);
        let body = http::get(
            &self.client,
            &url,
            Some(OCI_MANIFEST),
            Some(&token),
            MAX_SMALL,
            None,
            cancel,
        )
        .await
        .map_err(|e| e.with_context("ghcr.io"))?;
        parse_manifest(&body, &self.repo, &self.origin, self.arch).map_err(UpdateError::retry)
    }
}

impl ReleaseSource for Oci {
    async fn latest(&self, cancel: &CancellationToken) -> Result<Release, UpdateError> {
        self.manifest(cancel).await
    }

    async fn download(
        &self,
        layer: &Layer,
        max_bytes: usize,
        progress: Option<Progress<'_>>,
        cancel: &CancellationToken,
    ) -> Result<Option<Vec<u8>>, UpdateError> {
        let token = self.token(cancel).await?;
        match http::get(
            &self.client,
            &layer.url,
            None,
            Some(&token),
            max_bytes,
            progress,
            cancel,
        )
        .await
        {
            Ok(body) => Ok(Some(body)),
            Err(HttpError::Status(404)) => Ok(None),
            Err(error) => Err(error.with_context(&layer.title)),
        }
    }
}

fn parse_token(body: &[u8]) -> Result<String, String> {
    let document: serde_json::Value = serde_json::from_slice(body)
        .map_err(|_| "ghcr.io token: response is not JSON".to_owned())?;
    let token = document
        .get("token")
        .and_then(serde_json::Value::as_str)
        .ok_or("ghcr.io token: response has no token")?;
    // Заголовок запроса: без пробелов и управляющих символов.
    if token.is_empty() || !token.bytes().all(|b| b.is_ascii_graphic()) {
        return Err("ghcr.io token: unexpected token".into());
    }
    Ok(token.into())
}

/// `owner/name`: только строчные символы пути, ровно один слэш.
fn safe_repo(repo: &str) -> bool {
    let Some((owner, name)) = repo.split_once('/') else {
        return false;
    };
    let safe = |s: &str| {
        !s.is_empty()
            && s.len() <= 128
            && s.bytes().all(|c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'-' | b'_' | b'.')
            })
    };
    safe(owner) && safe(name)
}

#[derive(Deserialize)]
struct Manifest {
    #[serde(default)]
    annotations: Annotations,
    /// Нет поля — отказ, как у прежнего парсера.
    layers: Option<Vec<ManifestLayer>>,
}

#[derive(Deserialize, Default)]
struct Annotations {
    #[serde(rename = "org.opencontainers.image.version")]
    version: Option<String>,
    #[serde(rename = "org.opencontainers.image.title")]
    title: Option<String>,
}

#[derive(Deserialize)]
struct ManifestLayer {
    #[serde(default)]
    annotations: Annotations,
    digest: Option<String>,
}

fn parse_manifest(body: &[u8], repo: &str, origin: &str, arch: &str) -> Result<Release, String> {
    let manifest: Manifest =
        serde_json::from_slice(body).map_err(|_| "manifest is not JSON".to_owned())?;
    let version = manifest
        .annotations
        .version
        .ok_or("manifest has no org.opencontainers.image.version")?;
    let version = Version::parse_tag(&version)
        .ok_or_else(|| format!("unexpected image version {version}"))?;
    let raw_layers = manifest.layers.ok_or("manifest has no layers")?;
    let mut layers = Vec::new();
    for layer in raw_layers {
        let Some(title) = layer.annotations.title else {
            continue;
        };
        let digest = layer
            .digest
            .ok_or_else(|| format!("layer {title} has no digest"))?;
        let digest = Digest::parse(&digest)
            .ok_or_else(|| format!("layer {title}: unexpected digest {digest}"))?;
        layers.push(Layer {
            url: format!("{origin}v2/{repo}/blobs/{digest}"),
            title,
            digest,
        });
    }
    let asset = format!("pulsar-{arch}.bin");
    if !layers.iter().any(|layer| layer.title == asset) {
        return Err(format!("manifest has no {asset} layer"));
    }
    if !layers.iter().any(|layer| layer.title == "SHA256SUMS") {
        return Err("manifest has no SHA256SUMS layer".into());
    }
    Ok(Release::new(
        version,
        package_page(repo),
        repo.into(),
        layers,
    ))
}

fn package_page(repo: &str) -> String {
    let (owner, name) = repo.split_once('/').unwrap_or((repo, ""));
    format!("https://github.com/users/{owner}/packages/container/package/{name}")
}

#[cfg(test)]
mod tests;
