use std::collections::BTreeMap;

use serde::Deserialize;

const EMBEDDED_CATALOG: &str = include_str!("../../../../../platform/workstation/catalog.json");

#[derive(Debug, Deserialize)]
struct CatalogFile {
    #[serde(default)]
    paths: BTreeMap<String, CatalogMeta>,
}

#[derive(Debug, Deserialize)]
struct CatalogMeta {
    #[serde(default)]
    purpose: String,
    #[serde(default)]
    consumer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub purpose: String,
    pub consumer: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Catalog {
    entries: BTreeMap<String, Entry>,
}

impl Catalog {
    pub fn embedded() -> Result<Self, String> {
        Self::parse(EMBEDDED_CATALOG)
    }

    pub fn parse(json: &str) -> Result<Self, String> {
        let file: CatalogFile =
            serde_json::from_str(json).map_err(|error| format!("catalog JSON: {error}"))?;
        let entries = file
            .paths
            .into_iter()
            .map(|(path, meta)| {
                (
                    path,
                    Entry {
                        purpose: meta.purpose,
                        consumer: meta.consumer,
                    },
                )
            })
            .collect();
        Ok(Self { entries })
    }

    pub fn get(&self, path: &str) -> Option<&Entry> {
        self.entries.get(path)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &Entry)> {
        self.entries.iter()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_catalog_describes_the_application_paths() {
        let catalog = Catalog::embedded().expect("catalog");
        assert!(!catalog.is_empty());
        assert!(catalog.get("k8s/home-assistant/phone-push").is_some());
        assert!(catalog.get("infra/vpn/client-ca").is_some());
    }

    #[test]
    fn descriptions_come_from_the_purpose_field() {
        let catalog =
            Catalog::parse(r#"{"paths":{"k8s/test":{"purpose":"probe","consumer":"tests"}}}"#)
                .expect("catalog");
        assert_eq!(catalog.get("k8s/test").expect("entry").purpose, "probe");
        assert_eq!(
            catalog.get("k8s/test").expect("entry").consumer.as_deref(),
            Some("tests")
        );
        assert!(Catalog::parse("not-json").is_err());
    }
}
