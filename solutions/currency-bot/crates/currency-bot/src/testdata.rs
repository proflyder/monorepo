//! Locate the shared `solutions/currency-bot/testdata` directory in both
//! `cargo test` (package-relative) and Bazel (`TEST_SRCDIR` runfiles).

use std::path::PathBuf;

/// The repository testdata root.
pub fn dir() -> PathBuf {
    if std::env::var_os("TEST_SRCDIR").is_some() {
        let root = std::env::var("TEST_SRCDIR").expect("TEST_SRCDIR");
        let workspace = std::env::var("TEST_WORKSPACE").unwrap_or_default();
        let relative = "solutions/currency-bot/testdata";
        let with_workspace = PathBuf::from(&root).join(&workspace).join(relative);
        if with_workspace.is_dir() {
            return with_workspace;
        }
        return PathBuf::from(root).join(relative);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("testdata")
}
