use std::path::PathBuf;

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
