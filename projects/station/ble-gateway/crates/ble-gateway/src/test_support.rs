//! Shared helpers for the unit tests: temporary directories, a fake
//! `wpa_supplicant` control socket and a recording Bluetooth adapter.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::net::UnixDatagram;
use tokio::task::JoinHandle;

use crate::bluetooth::{AdapterFuture, AdapterOps, ClassicBluetooth};
use crate::cli::Config;
use crate::credentials::Credentials;
use crate::gateway::{Gateway, Timings};
use crate::gatt::Ble;
use crate::wifi::WpaClient;

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn temp_dir(tag: &str) -> PathBuf {
    let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir =
        std::env::temp_dir().join(format!("ble-gateway-{tag}-{}-{unique}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// A unix-datagram server that answers one command per datagram.
pub struct FakeWpa {
    path: PathBuf,
    task: JoinHandle<()>,
}

impl FakeWpa {
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for FakeWpa {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub fn fake_wpa<F>(dir: &Path, responder: F) -> FakeWpa
where
    F: Fn(&str) -> String + Send + 'static,
{
    let path = dir.join("wpa.sock");
    let socket = UnixDatagram::bind(&path).expect("bind fake wpa_supplicant");
    let task = tokio::spawn(async move {
        let mut buffer = vec![0u8; 4096];
        loop {
            let Ok((received, peer)) = socket.recv_from(&mut buffer).await else {
                return;
            };
            let command = String::from_utf8_lossy(&buffer[..received])
                .trim_end()
                .to_string();
            let mut reply = responder(&command);
            if !reply.ends_with('\n') {
                reply.push('\n');
            }
            let Some(target) = peer.as_pathname() else {
                continue;
            };
            let _ = socket.send_to(reply.as_bytes(), target).await;
        }
    });
    FakeWpa { path, task }
}

/// Records adapter operations so tests can assert on the BlueZ calls without a
/// system bus.
#[derive(Default)]
pub struct RecordingAdapter {
    calls: Mutex<Vec<String>>,
}

impl RecordingAdapter {
    #[must_use]
    pub fn calls(&self) -> Vec<String> {
        self.calls
            .lock()
            .map(|calls| calls.clone())
            .unwrap_or_default()
    }

    fn record(&self, call: impl Into<String>) {
        if let Ok(mut calls) = self.calls.lock() {
            calls.push(call.into());
        }
    }
}

impl AdapterOps for RecordingAdapter {
    fn prepare_audio(&self) -> AdapterFuture<'_, ()> {
        Box::pin(async move {
            self.record("prepare");
            Ok(())
        })
    }

    fn set_powered(&self, on: bool) -> AdapterFuture<'_, ()> {
        Box::pin(async move {
            self.record(format!("powered:{on}"));
            Ok(())
        })
    }

    fn set_pairable(&self, on: bool) -> AdapterFuture<'_, ()> {
        Box::pin(async move {
            self.record(format!("pairable:{on}"));
            Ok(())
        })
    }

    fn set_discoverable(&self, on: bool) -> AdapterFuture<'_, ()> {
        Box::pin(async move {
            self.record(format!("discoverable:{on}"));
            Ok(())
        })
    }

    fn disconnect_devices(&self) -> AdapterFuture<'_, ()> {
        Box::pin(async move {
            self.record("disconnect");
            Ok(())
        })
    }
}

/// Gateway wired to a fake wpa_supplicant socket and a recording adapter.
pub struct TestGateway {
    pub gateway: Arc<Gateway>,
    pub dir: PathBuf,
    wpa: FakeWpa,
    _commits: tokio::sync::mpsc::Receiver<Vec<u8>>,
}

impl TestGateway {
    pub fn new<F>(tag: &str, responder: F) -> Self
    where
        F: Fn(&str) -> String + Send + 'static,
    {
        Self::with_dry_run(tag, false, responder)
    }

    pub fn new_dry_run(tag: &str) -> Self {
        Self::with_dry_run(tag, true, |_| "OK".to_string())
    }

    fn with_dry_run<F>(tag: &str, dry_run: bool, responder: F) -> Self
    where
        F: Fn(&str) -> String + Send + 'static,
    {
        let dir = temp_dir(tag);
        let wpa = fake_wpa(&dir, responder);
        let config = Config {
            auto_wifi: false,
            log: None,
            token_file: dir.join("token"),
            button: PathBuf::from("/dev/input/event2"),
            dry_run,
            no_button: true,
            force: false,
            test_wpa: false,
            control_addr: String::new(),
            control_token_file: dir.join("control-token"),
            wpa_ctrl: wpa.path().to_path_buf(),
            log_level: "info".to_string(),
        };
        let timings = Timings {
            wifi_apply_timeout: Duration::from_secs(1),
            wifi_poll_interval: Duration::from_millis(1),
            wifi_post_connect_delay: Duration::ZERO,
            ..Timings::default()
        };
        let credentials = Arc::new(Credentials::default());
        let (commit_tx, commits) = tokio::sync::mpsc::channel(8);
        let ble = Ble::new(credentials.clone(), commit_tx);
        let client = WpaClient::new(wpa.path());
        let bluetooth = Arc::new(ClassicBluetooth::new(
            Arc::new(RecordingAdapter::default()),
            client.clone(),
            Duration::from_secs(90),
        ));
        let gateway = Gateway::new(
            config,
            timings,
            home_secret::Secret::new("123456"),
            credentials,
            ble,
            bluetooth,
            client,
        );
        Self {
            gateway,
            dir,
            wpa,
            _commits: commits,
        }
    }
}

impl Drop for TestGateway {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl std::fmt::Debug for TestGateway {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TestGateway")
            .field("dir", &self.dir)
            .field("wpa", &self.wpa.path)
            .finish_non_exhaustive()
    }
}
