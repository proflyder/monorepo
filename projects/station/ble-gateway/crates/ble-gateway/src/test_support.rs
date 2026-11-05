//! Shared helpers for the unit tests (temporary dirs and fake `wpa_cli`).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::gateway::{Config, Gateway, Timings};
use crate::logging::Logger;
use crate::wifi::Wpa;

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct TestContext {
    pub gateway: Arc<Gateway>,
    pub dir: PathBuf,
}

impl Drop for TestContext {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

pub fn temp_dir(tag: &str) -> PathBuf {
    let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir =
        std::env::temp_dir().join(format!("ble-gateway-{tag}-{}-{unique}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

pub fn new_gateway(tag: &str, wpa_path: &Path) -> TestContext {
    new_gateway_with(tag, wpa_path, true)
}

pub fn new_gateway_live(tag: &str, wpa_path: &Path) -> TestContext {
    new_gateway_with(tag, wpa_path, false)
}

fn new_gateway_with(tag: &str, wpa_path: &Path, dry_run: bool) -> TestContext {
    let dir = temp_dir(tag);
    let config = Config {
        token_file: dir.join("token"),
        dry_run,
        auto_wifi: false,
        control_addr: String::new(),
        ..Config::default()
    };
    let logger = Logger::file(&dir.join("test.log"), time::UtcOffset::UTC).expect("test log");
    let timings = Timings {
        wifi_apply_timeout: Duration::from_secs(1),
        wifi_poll_interval: Duration::from_millis(1),
        wifi_post_connect_delay: Duration::ZERO,
        ..Timings::default()
    };
    let gateway = Gateway::new(
        config,
        logger,
        Wpa::with_path(wpa_path.to_string_lossy().into_owned()),
        timings,
    );
    gateway.lock_state().token = "123456".to_string();
    TestContext { gateway, dir }
}

/// Create an executable `/bin/sh` fake and return its path.
pub fn fake_wpa(dir: &Path, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("wpa_cli");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write fake wpa_cli");
    let mut permissions = std::fs::metadata(&path).expect("stat fake").permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions).expect("chmod fake");
    path
}

pub fn read_file(path: &Path) -> String {
    std::fs::read_to_string(path).expect("read file")
}
