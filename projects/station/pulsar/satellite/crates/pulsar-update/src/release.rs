//! Данные релиза: версия, слои OCI и digest слоя как тип.

use std::fmt;

use crate::version::Version;
/// SHA-256 слоя OCI (`sha256:<64 hex>`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Digest([u8; 32]);

impl Digest {
    /// Разбор `sha256:<64 hex>`; другая длина, регистр или алгоритм — отказ.
    pub fn parse(text: &str) -> Option<Self> {
        let hex = text.strip_prefix("sha256:")?;
        if hex.len() != 64 {
            return None;
        }
        let mut bytes = [0u8; 32];
        for (i, pair) in hex.as_bytes().as_chunks::<2>().0.iter().enumerate() {
            bytes[i] = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
        }
        Some(Self(bytes))
    }

    pub fn of(data: &[u8]) -> Self {
        let digest = ring::digest::digest(&ring::digest::SHA256, data);
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(digest.as_ref());
        Self(bytes)
    }

    /// Совпадает с hex-строкой из `SHA256SUMS` (регистр не важен).
    pub fn matches_hex(self, hex: &str) -> bool {
        hex.len() == 64
            && hex
                .as_bytes()
                .as_chunks::<2>()
                .0
                .iter()
                .enumerate()
                .all(|(i, pair)| {
                    std::str::from_utf8(pair)
                        .ok()
                        .and_then(|pair| u8::from_str_radix(pair, 16).ok())
                        .is_some_and(|byte| byte == self.0[i])
                })
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("sha256:")?;
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// Слой OCI-образа с аннотацией `org.opencontainers.image.title`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layer {
    /// Имя ассета релиза (`pulsar-armhf.bin`, `SHA256SUMS`, образы радио).
    pub title: String,
    /// URL blob'а в реестре.
    pub(crate) url: String,
    pub(crate) digest: Digest,
}

impl Layer {
    /// Ожидаемый digest слоя.
    #[must_use]
    pub fn digest(&self) -> Digest {
        self.digest
    }
}

/// Найденный релиз: всё, что нужно установщику и сущности HA.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    version: Version,
    page_url: String,
    /// `owner/name` в GHCR: по нему берётся свежий pull-token.
    oci_repo: String,
    layers: Vec<Layer>,
}

impl Release {
    pub fn new(version: Version, page_url: String, oci_repo: String, layers: Vec<Layer>) -> Self {
        Self {
            version,
            page_url,
            oci_repo,
            layers,
        }
    }

    #[must_use]
    pub fn version(&self) -> &Version {
        &self.version
    }

    /// Страница пакета в GitHub (ссылка «Release notes» в HA).
    #[must_use]
    pub fn page_url(&self) -> &str {
        &self.page_url
    }

    /// URL ассета по имени слоя.
    #[must_use]
    pub fn asset_url(&self, title: &str) -> Option<&str> {
        self.layer(title).map(|layer| layer.url.as_str())
    }

    /// Слои образа в порядке манифеста.
    #[must_use]
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    pub fn layer(&self, title: &str) -> Option<&Layer> {
        self.layers.iter().find(|layer| layer.title == title)
    }

    /// Тест: подменить digest слоя, не трогая всё остальное.
    #[cfg(test)]
    pub fn set_layer_digest(&mut self, title: &str, digest: Digest) {
        if let Some(layer) = self.layers.iter_mut().find(|layer| layer.title == title) {
            layer.digest = digest;
        }
    }
}

impl fmt::Display for Release {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.version.as_str())
    }
}
