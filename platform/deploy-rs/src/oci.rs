//! Inspects and packages the OCI image layouts produced by `rules_oci`.
//!
//! A port of `platform/ci/oci_image.py` and `platform/deploy/internal/oci`:
//! the layout index and manifest are parsed with key order preserved, blobs are
//! copied byte for byte, and the archives use the Python USTAR/JSON layout, so
//! `oci pack`/`oci docker` output is byte-identical to the historical helper.

mod archive;
mod json;

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;

use self::archive::Archive;
pub use self::json::Json;

const INDEX_FILE: &str = "index.json";
const REF_ANNOTATION: &str = "org.opencontainers.image.ref.name";

/// The single image manifest referenced by a layout index.
pub struct Manifest {
    /// Manifest blob digest as written in the index.
    pub digest: String,
    /// Parsed manifest blob (ordered).
    pub value: Json,
    /// Parsed layout index (ordered).
    pub index: Json,
}

/// Returns the manifest digest of the (single) image in the layout.
pub fn digest(directory: &str) -> Result<String, String> {
    Ok(read_manifest(directory)?.digest)
}

/// Writes a standard OCI archive, annotating the image with `reference`, and
/// returns the manifest digest.
pub fn pack(directory: &str, reference: &str, output: &str) -> Result<String, String> {
    let mut manifest = read_manifest(directory)?;
    let Some(Json::Array(manifests)) = manifest.index.get_mut("manifests") else {
        return Err(format!("{directory}: index has no manifests"));
    };
    let Some(Json::Object(entry)) = manifests.first_mut() else {
        return Err(format!("{directory}: image manifest is not a JSON object"));
    };
    let position = entry.iter().position(|(name, _)| name == "annotations");
    let annotations = if let Some(index) = position {
        if !entry[index].1.is_object() {
            entry[index].1 = Json::Object(Vec::new());
        }
        let Json::Object(annotations) = &mut entry[index].1 else {
            unreachable!("annotations was made an object");
        };
        annotations
    } else {
        entry.push(("annotations".to_string(), Json::Object(Vec::new())));
        let Json::Object(annotations) = &mut entry.last_mut().expect("just pushed").1 else {
            unreachable!("annotations was just inserted");
        };
        annotations
    };
    if let Some((_, existing)) = annotations
        .iter_mut()
        .find(|(name, _)| name == REF_ANNOTATION)
    {
        *existing = Json::String(reference.to_string());
    } else {
        annotations.push((
            REF_ANNOTATION.to_string(),
            Json::String(reference.to_string()),
        ));
    }

    let blobs = list_blobs(directory)?;
    let mut archive = Archive::create(output).map_err(|error| error.to_string())?;
    let layout = fs::read(format!("{directory}/oci-layout")).map_err(|error| error.to_string())?;
    archive
        .add_file("oci-layout", &layout)
        .map_err(|error| error.to_string())?;
    archive
        .add_file(INDEX_FILE, &json::dump_indent(&manifest.index))
        .map_err(|error| error.to_string())?;
    for relative in &blobs {
        let data =
            fs::read(format!("{directory}/{relative}")).map_err(|error| error.to_string())?;
        archive
            .add_file(relative, &data)
            .map_err(|error| error.to_string())?;
    }
    archive.close().map_err(|error| error.to_string())?;
    Ok(manifest.digest)
}

/// Writes a docker `load` archive to output: uncompressed layers plus a
/// `manifest.json`, and returns the manifest digest.
pub fn docker_archive(directory: &str, reference: &str, output: &str) -> Result<String, String> {
    let manifest = read_manifest(directory)?;
    let Some(config) = manifest.value.get("config") else {
        return Err(format!("{directory}: manifest has no config"));
    };
    if !config.is_object() {
        return Err(format!("{directory}: manifest has no config"));
    }
    let Some(config_digest) = config.get("digest").and_then(Json::as_str) else {
        return Err(format!("{directory}: manifest config has no digest"));
    };
    let config_name = format!("{}.json", strip_sha256(config_digest));
    let config_path = blob_path(directory, config_digest)?;
    let config_bytes = fs::read(&config_path).map_err(|error| error.to_string())?;

    let mut layers = Vec::new();
    let mut entries = Vec::new();
    let Some(Json::Array(layer_list)) = manifest.value.get("layers") else {
        return Err(format!("{directory}: manifest has no layers"));
    };
    for layer in layer_list {
        if !layer.is_object() {
            return Err(format!("{directory}: layer is not a JSON object"));
        }
        let Some(layer_digest) = layer.get("digest").and_then(Json::as_str) else {
            return Err(format!("{directory}: layer has no digest"));
        };
        let media_type = layer.get("mediaType").and_then(Json::as_str).unwrap_or("");
        let name = format!("{}/layer.tar", strip_sha256(layer_digest));
        let path = blob_path(directory, layer_digest)?;
        let data = read_possibly_gzip(&path, media_type.ends_with("+gzip"))?;
        entries.push((name.clone(), data));
        layers.push(name);
    }

    let manifest_json = Json::Array(vec![Json::Object(vec![
        ("Config".to_string(), Json::String(config_name.clone())),
        (
            "RepoTags".to_string(),
            Json::Array(vec![Json::String(reference.to_string())]),
        ),
        (
            "Layers".to_string(),
            Json::Array(layers.into_iter().map(Json::String).collect()),
        ),
    ])]);

    let mut archive = Archive::create(output).map_err(|error| error.to_string())?;
    archive
        .add_file(&config_name, &config_bytes)
        .map_err(|error| error.to_string())?;
    for (name, data) in &entries {
        archive
            .add_file(name, data)
            .map_err(|error| error.to_string())?;
    }
    archive
        .add_file("manifest.json", &json::dump_compact(&manifest_json))
        .map_err(|error| error.to_string())?;
    archive.close().map_err(|error| error.to_string())?;
    Ok(manifest.digest)
}

/// Parses `<directory>/index.json` with key order preserved.
fn read_index(directory: &str) -> Result<Json, String> {
    let raw = fs::read(format!("{directory}/{INDEX_FILE}")).map_err(|error| error.to_string())?;
    let value = Json::parse(&raw)?;
    if !value.is_object() {
        return Err(format!("{directory}: index is not a JSON object"));
    }
    Ok(value)
}

/// Resolves a `sha256:<hex>` digest to its path in the layout.
fn blob_path(directory: &str, digest: &str) -> Result<PathBuf, String> {
    let Some((algorithm, value)) = digest.split_once(':') else {
        return Err(format!("unsupported digest: {digest}"));
    };
    if algorithm != "sha256" || value.is_empty() {
        return Err(format!("unsupported digest: {digest}"));
    }
    Ok(Path::new(directory)
        .join("blobs")
        .join(algorithm)
        .join(value))
}

/// Loads the only image manifest referenced by the layout index.
fn read_manifest(directory: &str) -> Result<Manifest, String> {
    let index = read_index(directory)?;
    let Some(Json::Array(manifests)) = index.get("manifests") else {
        return Err(format!("{directory}: expected one image manifest, found 0"));
    };
    if manifests.len() != 1 {
        return Err(format!(
            "{directory}: expected one image manifest, found {}",
            manifests.len()
        ));
    }
    let entry = &manifests[0];
    if !entry.is_object() {
        return Err(format!("{directory}: image manifest is not a JSON object"));
    }
    let Some(manifest_digest) = entry.get("digest").and_then(Json::as_str) else {
        return Err(format!("{directory}: image manifest has no digest"));
    };
    let path = blob_path(directory, manifest_digest)?;
    let raw = fs::read(&path).map_err(|error| error.to_string())?;
    let value = Json::parse(&raw)?;
    if !value.is_object() {
        return Err(format!(
            "{}: manifest blob is not a JSON object",
            path.display()
        ));
    }
    Ok(Manifest {
        digest: manifest_digest.to_string(),
        value,
        index,
    })
}

/// Returns every regular file under `<directory>/blobs` as a sorted
/// slash-separated path relative to the layout.
fn list_blobs(directory: &str) -> Result<Vec<String>, String> {
    let root = Path::new(directory).join("blobs");
    let mut blobs = Vec::new();
    walk(&root, Path::new(directory), &mut blobs).map_err(|error| error.to_string())?;
    blobs.sort();
    Ok(blobs)
}

fn walk(path: &Path, root: &Path, blobs: &mut Vec<String>) -> std::io::Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk(&path, root, blobs)?;
        } else {
            let relative = path.strip_prefix(root).unwrap_or(&path);
            blobs.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

fn strip_sha256(digest: &str) -> &str {
    digest.strip_prefix("sha256:").unwrap_or(digest)
}

/// Reads data, decompressing a gzip stream when asked.
fn read_possibly_gzip(path: &Path, compressed: bool) -> Result<Vec<u8>, String> {
    let raw = fs::read(path).map_err(|error| error.to_string())?;
    if !compressed {
        return Ok(raw);
    }
    let mut reader = GzDecoder::new(raw.as_slice());
    let mut decoded = Vec::new();
    reader
        .read_to_end(&mut decoded)
        .map_err(|error| error.to_string())?;
    Ok(decoded)
}

impl Json {
    /// Returns the string value of a JSON string node.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(relative: &str) -> String {
        if let Ok(root) = std::env::var("TEST_SRCDIR") {
            for workspace in ["_main", "yandex_station_lab"] {
                let candidate = format!(
                    "{root}/{workspace}/platform/deploy-rs/testdata/frozen/cases/{relative}"
                );
                if Path::new(&candidate).exists() {
                    return candidate;
                }
            }
            if let Ok(workspace) = std::env::var("TEST_WORKSPACE") {
                return format!(
                    "{root}/{workspace}/platform/deploy-rs/testdata/frozen/cases/{relative}"
                );
            }
        }
        format!(
            "{}/testdata/frozen/cases/{relative}",
            env!("CARGO_MANIFEST_DIR")
        )
    }

    const GOLDEN_DIGEST: &str =
        "sha256:8883b693cf69f68bc4762c2764d610df8e6dd8388f6b6e536c0d90a50bf82880";

    #[test]
    fn digest_matches_the_python_helper() {
        assert_eq!(
            digest(&fixture("oci-digest/layout")).expect("digest"),
            GOLDEN_DIGEST
        );
    }

    #[test]
    fn pack_matches_the_python_bytes() {
        let directory = tempfile::tempdir().expect("tempdir");
        let output = directory.path().join("pack.tar");
        assert_eq!(
            pack(
                &fixture("oci-pack/layout"),
                "home.test/example:test",
                &output.display().to_string()
            )
            .expect("pack"),
            GOLDEN_DIGEST
        );
        let actual = fs::read(&output).expect("read output");
        let expected = fs::read(fixture("oci-pack/expected/pack.tar")).expect("read golden");
        assert_eq!(actual, expected);
    }

    #[test]
    fn docker_archive_matches_the_python_bytes() {
        let directory = tempfile::tempdir().expect("tempdir");
        let output = directory.path().join("docker.tar");
        assert_eq!(
            docker_archive(
                &fixture("oci-docker/layout"),
                "home.test/example:test",
                &output.display().to_string()
            )
            .expect("docker"),
            GOLDEN_DIGEST
        );
        let actual = fs::read(&output).expect("read output");
        let expected = fs::read(fixture("oci-docker/expected/docker.tar")).expect("read golden");
        assert_eq!(actual, expected);
    }

    #[test]
    fn errors_name_the_unsupported_digest_and_manifest_count() {
        let unsupported = tempfile::tempdir().expect("tempdir");
        fs::write(
            unsupported.path().join(INDEX_FILE),
            br#"{"manifests":[{"digest":"sha512:abc"}]}"#,
        )
        .expect("write");
        let error = digest(&unsupported.path().display().to_string()).expect_err("unsupported");
        assert!(error.contains("unsupported digest: sha512:abc"), "{error}");

        let empty = tempfile::tempdir().expect("tempdir");
        fs::write(empty.path().join(INDEX_FILE), br#"{"manifests":[]}"#).expect("write");
        let error = digest(&empty.path().display().to_string()).expect_err("empty");
        assert!(
            error.contains("expected one image manifest, found 0"),
            "{error}"
        );

        let two = tempfile::tempdir().expect("tempdir");
        fs::write(
            two.path().join(INDEX_FILE),
            br#"{"manifests":[{"digest":"sha256:a"},{"digest":"sha256:b"}]}"#,
        )
        .expect("write");
        let error = digest(&two.path().display().to_string()).expect_err("two");
        assert!(
            error.contains("expected one image manifest, found 2"),
            "{error}"
        );
    }
}
