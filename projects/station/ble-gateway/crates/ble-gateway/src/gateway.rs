//! Daemon state and the behaviours shared by GATT, HTTP, the button and auto
//! mode.
//!
//! The status is a [`tokio::sync::watch`] channel: the status file writer, GATT
//! notifications and HTTP all read the same value. Commits arrive through an
//! `mpsc` worker and an async mutex, so two parallel commits never run two
//! Wi-Fi applies. Long D-Bus operations take short async mutexes; timestamps and
//! flags use atomics or a one-field `std::sync::Mutex`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Context;
use home_runtime::Supervisor;
use home_secret::Secret;
use tokio::sync::{Mutex as AsyncMutex, mpsc, watch};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use zbus::Connection;

use crate::bluetooth::{BlueZAdapter, ClassicBluetooth};
use crate::cli::Config;
use crate::control;
use crate::credentials::Credentials;
use crate::gatt::Ble;
use crate::status::Status;
use crate::wifi::{WpaClient, validate_wifi};

/// Timings are fields so tests can drive the same code without production
/// sleeps.
#[derive(Debug, Clone)]
pub struct Timings {
    pub auto_tick: Duration,
    pub disconnect_threshold: Duration,
    pub wifi_apply_timeout: Duration,
    pub wifi_poll_interval: Duration,
    pub wifi_post_connect_delay: Duration,
    pub manual_window: Duration,
    pub bluetooth_window: Duration,
    pub hold_window: Duration,
    pub press_max: Duration,
    pub press_group: Duration,
}

impl Default for Timings {
    fn default() -> Self {
        Self {
            auto_tick: Duration::from_secs(5),
            disconnect_threshold: Duration::from_secs(20),
            wifi_apply_timeout: Duration::from_secs(20),
            wifi_poll_interval: Duration::from_millis(500),
            wifi_post_connect_delay: Duration::from_secs(3),
            manual_window: Duration::from_secs(15 * 60),
            bluetooth_window: Duration::from_secs(90),
            hold_window: Duration::from_secs(5),
            press_max: Duration::from_millis(400),
            press_group: Duration::from_millis(1500),
        }
    }
}

pub struct Gateway {
    pub(crate) cfg: Config,
    pub(crate) timings: Timings,
    pub(crate) wpa: WpaClient,
    token: Secret,
    pub(crate) credentials: Arc<Credentials>,
    pub(crate) ble: Ble,
    pub(crate) bluetooth: Arc<ClassicBluetooth>,
    status_tx: watch::Sender<Status>,
    manual_until: Mutex<Option<Instant>>,
    disconnected_since: Mutex<Option<Instant>>,
    pub(crate) applying: AtomicBool,
    ble_lock: AsyncMutex<()>,
    bluetooth_lock: AsyncMutex<()>,
    commit_lock: AsyncMutex<()>,
}

impl Gateway {
    pub(crate) fn new(
        cfg: Config,
        timings: Timings,
        token: Secret,
        credentials: Arc<Credentials>,
        ble: Ble,
        bluetooth: Arc<ClassicBluetooth>,
        wpa: WpaClient,
    ) -> Arc<Self> {
        Arc::new(Self {
            cfg,
            timings,
            wpa,
            token,
            credentials,
            ble,
            bluetooth,
            status_tx: watch::channel(Status::Idle).0,
            manual_until: Mutex::new(None),
            disconnected_since: Mutex::new(None),
            applying: AtomicBool::new(false),
            ble_lock: AsyncMutex::new(()),
            bluetooth_lock: AsyncMutex::new(()),
            commit_lock: AsyncMutex::new(()),
        })
    }

    /// Build the daemon, connect to BlueZ and spawn every background task.
    pub async fn start(
        config: Config,
        token: Secret,
        supervisor: Supervisor,
    ) -> anyhow::Result<Arc<Self>> {
        let credentials = Arc::new(Credentials::default());
        let (commit, receiver) = mpsc::channel(8);
        let ble = Ble::new(credentials.clone(), commit);
        let wpa = WpaClient::new(&config.wpa_ctrl);
        let adapter = Arc::new(BlueZAdapter::new());
        let timings = Timings::default();
        let bluetooth = Arc::new(ClassicBluetooth::new(
            adapter.clone(),
            wpa.clone(),
            timings.bluetooth_window,
        ));
        let gateway = Self::new(
            config.clone(),
            timings,
            token,
            credentials,
            ble,
            bluetooth,
            wpa,
        );
        let connection = Connection::system().await.context("system D-Bus")?;
        adapter.set_connection(connection.clone());
        gateway.ble.set_connection(connection.clone());
        if let Err(error) = connection
            .request_name_with_flags(
                "com.station.gateway",
                zbus::fdo::RequestNameFlags::DoNotQueue.into(),
            )
            .await
        {
            warn!(%error, "RequestName com.station.gateway");
        }
        if !config.control_addr.is_empty()
            && let Err(error) = control::start(&gateway, &config, &supervisor).await
        {
            warn!(%error, "control HTTP disabled");
        }
        supervisor.spawn("status", gateway.clone().status_task(supervisor.token()));
        supervisor.spawn(
            "commits",
            gateway.clone().run_commits(receiver, supervisor.token()),
        );
        if !config.no_button {
            let button = gateway.clone();
            let path = config.button.clone();
            let shutdown = supervisor.token();
            supervisor.spawn(
                "button",
                async move { button.run_button(&path, shutdown).await },
            );
        }
        supervisor.spawn("auto", gateway.clone().auto_loop(supervisor.token()));
        gateway.set_status(Status::Idle);
        info!(token = %config.token_file.display(), "station BLE gateway starting");
        if config.force
            && let Err(error) = gateway.enable_ble().await
        {
            warn!(%error, "force enable BLE");
        }
        Ok(gateway)
    }

    pub fn status(&self) -> Status {
        self.status_tx.borrow().clone()
    }

    pub fn set_status(&self, status: Status) {
        if *self.status_tx.borrow() == status {
            return;
        }
        info!(%status, "status");
        self.status_tx.send_replace(status);
    }

    pub fn is_manual(&self) -> bool {
        match self.manual_until.lock() {
            Ok(until) => until.is_some_and(|until| until > Instant::now()),
            Err(error) => {
                warn!(%error, "manual window lock poisoned");
                false
            }
        }
    }

    pub async fn enable_ble(&self) -> Result<(), String> {
        let _guard = self.ble_lock.lock().await;
        if self.ble.advertising() {
            return Ok(());
        }
        self.ble.enable().await.map_err(|error| error.to_string())?;
        self.set_status(Status::Ready);
        Ok(())
    }

    pub async fn disable_ble(&self) {
        let _guard = self.ble_lock.lock().await;
        self.ble.disable().await;
        self.set_status(Status::Idle);
    }

    async fn set_manual(&self, open: bool) {
        match self.manual_until.lock() {
            Ok(mut until) => *until = open.then(|| Instant::now() + self.timings.manual_window),
            Err(error) => warn!(%error, "manual window lock poisoned"),
        }
        if open {
            if let Err(error) = self.enable_ble().await {
                warn!(%error, "enable BLE for manual window");
            }
        } else {
            self.disable_ble().await;
        }
    }

    pub async fn open_manual(&self) {
        self.set_manual(true).await;
    }

    pub async fn close_manual(&self) {
        self.set_manual(false).await;
    }

    pub async fn toggle_manual(&self) {
        self.set_manual(!self.is_manual()).await;
    }

    pub async fn toggle_bluetooth(&self) {
        let _guard = self.bluetooth_lock.lock().await;
        self.bluetooth.toggle().await;
    }

    pub async fn enable_bluetooth(&self) {
        let _guard = self.bluetooth_lock.lock().await;
        self.bluetooth.enable().await;
    }

    pub async fn disable_bluetooth(&self) {
        let _guard = self.bluetooth_lock.lock().await;
        self.bluetooth.disable().await;
    }

    /// Validate a commit frame and apply the Wi-Fi network.
    pub async fn handle_commit(self: &Arc<Self>, value: &[u8]) {
        let command = trim_bytes(value);
        info!(request = %String::from_utf8_lossy(command), "commit request");
        if command.is_empty() {
            return self.set_status(Status::error("EMPTY_COMMIT"));
        }
        if command == b"CANCEL" {
            return self.disable_ble().await;
        }
        let snapshot = self.credentials.snapshot();
        let ssid = trim_bytes(&snapshot.ssid).to_vec();
        let psk = trim_bytes(&snapshot.psk).to_vec();
        if !self.token.ct_eq(trim_bytes(&snapshot.token)) {
            warn!("commit rejected: bad token");
            return self.set_status(Status::error("BAD_TOKEN"));
        }
        if let Err(code) = validate_wifi(&ssid, &psk) {
            warn!(code, "commit rejected: invalid Wi-Fi credentials");
            return self.set_status(Status::error(code));
        }
        let _guard = self.commit_lock.lock().await;
        self.set_status(Status::Connecting);
        self.apply_wifi(
            &String::from_utf8_lossy(&ssid),
            &String::from_utf8_lossy(&psk),
        )
        .await;
    }

    /// Drain GATT commit frames one at a time until shutdown.
    pub async fn run_commits(
        self: Arc<Self>,
        mut receiver: mpsc::Receiver<Vec<u8>>,
        shutdown: CancellationToken,
    ) {
        loop {
            tokio::select! {
                () = shutdown.cancelled() => return,
                frame = receiver.recv() => match frame {
                    Some(frame) => self.handle_commit(&frame).await,
                    None => return,
                },
            }
        }
    }

    /// Write the latest status and notify GATT subscribers until shutdown.
    pub async fn status_task(self: Arc<Self>, shutdown: CancellationToken) {
        let mut receiver = self.status_tx.subscribe();
        loop {
            let status = receiver.borrow_and_update().clone();
            self.publish_status(&status).await;
            tokio::select! {
                result = receiver.changed() => if result.is_err() { return },
                () = shutdown.cancelled() => {
                    self.publish_status(&self.status()).await;
                    return;
                }
            }
        }
    }

    async fn publish_status(&self, status: &Status) {
        let path = status_path(&self.cfg.token_file);
        if let Err(error) = write_status(&path, status) {
            warn!(%error, path = %path.display(), "write status");
        }
        self.ble.notify_status(status.as_str().as_bytes()).await;
    }

    /// Automatic BLE fallback: advertise when Wi-Fi has been down long enough.
    pub async fn auto_loop(self: Arc<Self>, shutdown: CancellationToken) {
        let mut ticker = tokio::time::interval(self.timings.auto_tick);
        ticker.tick().await;
        loop {
            tokio::select! {
                () = shutdown.cancelled() => return,
                _ = ticker.tick() => {}
            }
            if !self.cfg.auto_wifi || self.applying.load(Ordering::SeqCst) {
                continue;
            }
            let state = self.wpa.state().await;
            if state == "COMPLETED" {
                match self.disconnected_since.lock() {
                    Ok(mut since) => *since = None,
                    Err(error) => warn!(%error, "disconnect timer lock poisoned"),
                }
                if self.ble.advertising() && !self.is_manual() {
                    self.disable_ble().await;
                }
                continue;
            }
            let first = match self.disconnected_since.lock() {
                Ok(mut since) => since.replace(Instant::now()).is_none(),
                Err(error) => {
                    warn!(%error, "disconnect timer lock poisoned");
                    false
                }
            };
            if first {
                continue;
            }
            let down = match self.disconnected_since.lock() {
                Ok(since) => since
                    .map(|at| at.elapsed())
                    .is_some_and(|elapsed| elapsed >= self.timings.disconnect_threshold),
                Err(error) => {
                    warn!(%error, "disconnect timer lock poisoned");
                    false
                }
            };
            if down
                && !self.ble.advertising()
                && !self.is_manual()
                && let Err(error) = self.enable_ble().await
            {
                warn!(%error, "auto enable BLE");
            }
        }
    }
}

/// Trim whitespace like Go's `strings.TrimSpace`; invalid UTF-8 falls back to
/// ASCII trimming so Wi-Fi validation still rejects it.
pub fn trim_bytes(value: &[u8]) -> &[u8] {
    if let Ok(text) = std::str::from_utf8(value) {
        return text.trim().as_bytes();
    }
    let first = value.iter().position(|byte| !byte.is_ascii_whitespace());
    let last = value.iter().rposition(|byte| !byte.is_ascii_whitespace());
    match (first, last) {
        (Some(first), Some(last)) => &value[first..=last],
        _ => &[],
    }
}

/// Keep digits only and cap at 12 characters.
pub fn normalize_token(value: &str) -> Option<String> {
    let digits: String = value
        .chars()
        .filter(char::is_ascii_digit)
        .take(12)
        .collect();
    (!digits.is_empty()).then_some(digits)
}

/// `status` lives next to the token file.
pub fn status_path(token_file: &Path) -> PathBuf {
    token_file
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .join("status")
}

fn write_status(path: &Path, status: &Status) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, format!("{status}\n"))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::significant_drop_tightening)]

    use std::time::Duration;

    use super::*;
    use crate::test_support::TestGateway;

    #[test]
    fn trim_bytes_matches_go_trim_space() {
        assert_eq!(trim_bytes(b"  Home\t\n"), b"Home");
        assert_eq!(trim_bytes(" \u{00a0}Home\u{2028} ".as_bytes()), b"Home");
        assert_eq!(trim_bytes(&[b' ', 0xff, b' ']), &[0xff]);
        assert!(trim_bytes(b" \t").is_empty());
    }

    #[test]
    fn normalize_token_keeps_digits_only() {
        assert_eq!(normalize_token("987-654"), Some("987654".to_string()));
        assert_eq!(normalize_token("abc"), None);
        assert_eq!(
            normalize_token("0123456789012"),
            Some("012345678901".to_string())
        );
    }

    #[test]
    fn status_path_follows_token_dir() {
        assert_eq!(
            status_path(Path::new("/data/quasar/ble-gateway/token")),
            PathBuf::from("/data/quasar/ble-gateway/status")
        );
        assert_eq!(status_path(Path::new("token")), PathBuf::from("./status"));
    }

    #[tokio::test]
    async fn status_task_writes_the_file_and_updates_readers() {
        let context = TestGateway::new("status-task", |_| "OK".to_string());
        let gateway = context.gateway.clone();
        let mut receiver = gateway.status_tx.subscribe();
        let task = tokio::spawn(gateway.clone().status_task(CancellationToken::new()));
        gateway.set_status(Status::Ready);
        tokio::time::timeout(Duration::from_secs(2), receiver.changed())
            .await
            .expect("watch update")
            .expect("watch alive");
        assert_eq!(*receiver.borrow(), Status::Ready);
        let path = context.dir.join("status");
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while std::fs::read_to_string(&path).unwrap_or_default().trim() != "READY" {
            assert!(std::time::Instant::now() < deadline, "status not written");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        task.abort();
    }

    #[tokio::test]
    async fn commit_validates_token_and_credentials() {
        let context = TestGateway::new("commit-validation", |_| "OK".to_string());
        let gateway = &context.gateway;
        gateway.handle_commit(&[]).await;
        assert_eq!(gateway.status(), Status::error("EMPTY_COMMIT"));
        gateway
            .credentials
            .set_all(b"Home", b"password123", b"wrong");
        gateway.handle_commit(b"1").await;
        assert_eq!(gateway.status(), Status::error("BAD_TOKEN"));
        gateway.credentials.set_all(b"Home", b"", b"123456");
        gateway.handle_commit(b"1").await;
        assert_eq!(gateway.status(), Status::error("EMPTY_PSK"));
    }

    #[tokio::test]
    async fn cancel_commit_only_disables_ble() {
        let context = TestGateway::new("commit-cancel", |_| "OK".to_string());
        context.gateway.set_status(Status::Ready);
        context.gateway.handle_commit(b" CANCEL \n").await;
        assert_eq!(context.gateway.status(), Status::Idle);
    }

    #[tokio::test]
    async fn two_parallel_commits_are_serialised() {
        let commands = Arc::new(Mutex::new(Vec::new()));
        let sink = commands.clone();
        let context = TestGateway::new("commit-serialised", move |command| {
            sink.lock().expect("commands").push(command.to_string());
            match command {
                "STATUS" => "wpa_state=COMPLETED".to_string(),
                "LIST_NETWORKS" => "0\tOld\tany\t[CURRENT]".to_string(),
                "ADD_NETWORK" => "7".to_string(),
                _ => "OK".to_string(),
            }
        });
        let gateway = context.gateway.clone();
        gateway
            .credentials
            .set_all(b"Home", b"password123", b"123456");
        let _ = tokio::join!(gateway.handle_commit(b"1"), gateway.handle_commit(b"1"));
        assert_eq!(gateway.status(), Status::Connected);
        let commands = commands.lock().expect("commands");
        let mut applies = 0;
        let mut in_apply = false;
        for command in commands.iter() {
            if command == "ADD_NETWORK" {
                assert!(!in_apply, "second apply started before the first finished");
                in_apply = true;
                applies += 1;
            } else if command == "SAVE_CONFIG" {
                in_apply = false;
            }
        }
        assert_eq!(applies, 2, "commands: {commands:?}");
    }

    #[tokio::test]
    async fn background_tasks_stop_on_shutdown() {
        let context = TestGateway::new("shutdown", |_| "OK".to_string());
        let gateway = context.gateway.clone();
        let shutdown = CancellationToken::new();
        let (sender, receiver) = mpsc::channel(1);
        let status = tokio::spawn(gateway.clone().status_task(shutdown.clone()));
        let commits = tokio::spawn(gateway.clone().run_commits(receiver, shutdown.clone()));
        let auto = tokio::spawn(gateway.clone().auto_loop(shutdown.clone()));
        drop(sender);
        shutdown.cancel();
        for task in [status, commits, auto] {
            tokio::time::timeout(Duration::from_secs(1), task)
                .await
                .expect("task must stop")
                .expect("task");
        }
    }

    #[tokio::test]
    async fn manual_window_toggles_ble_without_a_bus() {
        let context = TestGateway::new("manual", |_| "OK".to_string());
        let gateway = &context.gateway;
        gateway.open_manual().await;
        assert!(gateway.is_manual());
        gateway.close_manual().await;
        assert!(!gateway.is_manual());
        assert_eq!(gateway.status(), Status::Idle);
    }
}
