//! Deploy-data contract: the tracked catalog and bb allow-list must load with
//! the gateway's real validators.
//!
//! This replaces `projects/home/kubernetes/mcp-gateway/catalog_contract_test.go`,
//! which built the former Go binary and waited for `/readyz`. The schema check
//! itself never needed a process: startup parses and validates both files before
//! the listener exists, so parsing them here with the same code is equivalent.
//! The negative case proves the check is not vacuous.

use std::path::{Path, PathBuf};

use mcp_gateway::bb::BbResolver;
use mcp_gateway::domain::catalog::Catalog;

/// Resolve a repository-relative path from `CARGO_MANIFEST_DIR` (cargo) or the
/// runfiles workspace root (Bazel).
fn repo_path(relative: &str) -> PathBuf {
    let candidate = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../..")
        .join(relative);
    if candidate.exists() {
        return candidate;
    }
    PathBuf::from(relative)
}

const DEPLOY_DIR: &str = "projects/home/kubernetes/mcp-gateway/manifests";

#[test]
fn tracked_catalog_loads_with_the_real_validator() {
    let path = repo_path(&format!("{DEPLOY_DIR}/catalog.yaml"));
    let data = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    Catalog::parse(&data)
        .unwrap_or_else(|error| panic!("tracked catalog {} is invalid: {error}", path.display()));
}

#[test]
fn tracked_bb_targets_load_with_the_real_validator() {
    let path = repo_path(&format!("{DEPLOY_DIR}/bb-targets.yaml"));
    let data = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    BbResolver::parse(data.as_bytes()).unwrap_or_else(|error| {
        panic!("tracked bb targets {} are invalid: {error}", path.display())
    });
}

#[test]
fn a_duplicate_service_id_is_rejected() {
    let broken = r#"
overview: broken
services:
  - id: duplicate
    aliases: {en: [one]}
    kind: external
    logs: {source: none}
    restart: {allowed: false}
  - id: duplicate
    aliases: {en: [two]}
    kind: external
    logs: {source: none}
    restart: {allowed: false}
"#;
    let error = Catalog::parse(broken).expect_err("a duplicate id must be rejected");
    assert!(
        error.to_string().contains("duplicate"),
        "expected a duplicate-id error, got {error}"
    );
}

#[test]
fn a_target_without_aliases_is_rejected() {
    let broken = r#"
targets:
  - project: proj_example
    aliases: []
"#;
    let error = BbResolver::parse(broken.as_bytes()).expect_err("aliases are required");
    assert!(
        error.contains("no aliases"),
        "expected an alias error, got {error}"
    );
}
