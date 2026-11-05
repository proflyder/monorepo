//! Фейковый источник релизов для хостовых тестов установщика и актора.

use std::collections::HashMap;

use tokio_util::sync::CancellationToken;

use crate::error::UpdateError;
use crate::http::{HttpError, Progress};
use crate::release::{Digest, Layer, Release};
use crate::source::ReleaseSource;
use crate::version::Version;

#[derive(Clone)]
pub enum FakeAsset {
    Body(Vec<u8>),
    Status(u16),
}

#[derive(Clone)]
pub struct Fake {
    pub latest: Result<Release, UpdateError>,
    pub assets: HashMap<String, FakeAsset>,
}

impl Fake {
    pub fn new(assets: impl IntoIterator<Item = (&'static str, FakeAsset)>) -> Self {
        Self {
            latest: Err(UpdateError::retry("no release")),
            assets: assets
                .into_iter()
                .map(|(title, asset)| (title.to_owned(), asset))
                .collect(),
        }
    }

    pub fn with_latest(mut self, release: Release) -> Self {
        self.latest = Ok(release);
        self
    }
}

impl ReleaseSource for Fake {
    async fn latest(&self, _cancel: &CancellationToken) -> Result<Release, UpdateError> {
        self.latest.clone()
    }

    async fn download(
        &self,
        layer: &Layer,
        _max_bytes: usize,
        progress: Option<Progress<'_>>,
        _cancel: &CancellationToken,
    ) -> Result<Option<Vec<u8>>, UpdateError> {
        match self.assets.get(&layer.title) {
            Some(FakeAsset::Body(body)) => {
                if let Some(callback) = progress {
                    callback(0, body.len());
                    callback(body.len() / 2, body.len());
                    callback(body.len(), body.len());
                }
                Ok(Some(body.clone()))
            }
            Some(FakeAsset::Status(404)) | None => Ok(None),
            Some(FakeAsset::Status(status)) => {
                Err(HttpError::Status(*status).with_context(&layer.title))
            }
        }
    }
}

/// digest OCI-слоя: `sha256:<hex>`.
pub fn sha256_digest(data: &[u8]) -> String {
    Digest::of(data).to_string()
}

/// Голый hex для `SHA256SUMS`.
pub fn sha_hex(data: &[u8]) -> String {
    let digest = sha256_digest(data);
    digest["sha256:".len()..].to_owned()
}

pub fn layer(title: &str, url: &str, data: &[u8]) -> Layer {
    layer_with_digest(title, url, Digest::of(data))
}

pub fn layer_with_digest(title: &str, url: &str, digest: Digest) -> Layer {
    Layer {
        title: title.into(),
        url: url.into(),
        digest,
    }
}

pub fn release(version: &str, layers: Vec<Layer>) -> Release {
    Release::new(
        Version::parse(version).expect("version"),
        String::new(),
        "owner/repo".into(),
        layers,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_round_trip_and_case() {
        let digest = Digest::of(b"");
        let text = digest.to_string();
        assert_eq!(
            text,
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(Digest::parse(&text), Some(digest));
        assert!(digest.matches_hex(&text["sha256:".len()..].to_uppercase()));
        assert_eq!(Digest::parse("sha512:00"), None);
    }
}
