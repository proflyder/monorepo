//! Daemon state and the behaviours shared by GATT, HTTP, the button and auto mode.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use zbus::Connection;

use crate::gatt::Characteristic;
use crate::logf;
use crate::logging::Logger;
use crate::wifi::{Wpa, validate_wifi};

const DEFAULT_TOKEN_FILE: &str = "/data/quasar/ble-gateway/token";
const DEFAULT_BUTTON: &str = "/dev/input/event2";
const DEFAULT_CONTROL_ADDR: &str = "0.0.0.0:8099";
const DEFAULT_CONTROL_TOKEN_FILE: &str = "/data/quasar/ble-gateway/control-token";

/// Command-line configuration, mirroring the Go flags one for one.
#[derive(Debug, Clone)]
pub struct Config {
    pub log: Option<PathBuf>,
    pub token_file: PathBuf,
    pub button: PathBuf,
    pub dry_run: bool,
    pub auto_wifi: bool,
    pub no_button: bool,
    pub force: bool,
    pub test_wpa: bool,
    pub control_addr: String,
    pub control_token_file: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            log: None,
            token_file: PathBuf::from(DEFAULT_TOKEN_FILE),
            button: PathBuf::from(DEFAULT_BUTTON),
            dry_run: false,
            auto_wifi: true,
            no_button: false,
            force: false,
            test_wpa: false,
            control_addr: DEFAULT_CONTROL_ADDR.to_string(),
            control_token_file: PathBuf::from(DEFAULT_CONTROL_TOKEN_FILE),
        }
    }
}

/// Timings are fields rather than constants so unit tests can drive the same
/// code without sleeping for production intervals.
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

pub struct State {
    pub by_name: HashMap<String, Arc<Characteristic>>,
    pub chars: Vec<Arc<Characteristic>>,
    pub gatt_registered: bool,
    pub advertising: bool,
    pub token: String,
    pub status: String,
    pub manual_until: Option<Instant>,
    pub bt_mode: bool,
    pub bt_generation: u64,
    pub control_token: String,
    pub ssid: String,
    pub psk: String,
    pub applying: bool,
    pub disconnected_since: Option<Instant>,
}

impl State {
    fn new() -> Self {
        Self {
            by_name: HashMap::new(),
            chars: Vec::new(),
            gatt_registered: false,
            advertising: false,
            token: String::new(),
            status: "IDLE".to_string(),
            manual_until: None,
            bt_mode: false,
            bt_generation: 0,
            control_token: String::new(),
            ssid: String::new(),
            psk: String::new(),
            applying: false,
            disconnected_since: None,
        }
    }
}

pub struct Gateway {
    pub cfg: Config,
    pub logger: Logger,
    pub wpa: Wpa,
    pub timings: Timings,
    pub state: Mutex<State>,
    status_file: PathBuf,
    conn: OnceLock<Connection>,
}

/// Snapshot of the mutable fields reported by `/status`.
pub struct ControlStatus {
    pub status: String,
    pub manual: bool,
    pub bluetooth: bool,
}

impl Gateway {
    pub fn new(cfg: Config, logger: Logger, wpa: Wpa, timings: Timings) -> Arc<Self> {
        Arc::new(Self {
            status_file: status_path(&cfg.token_file),
            cfg,
            logger,
            wpa,
            timings,
            state: Mutex::new(State::new()),
            conn: OnceLock::new(),
        })
    }

    pub fn status_file(&self) -> &Path {
        &self.status_file
    }

    pub fn set_connection(&self, conn: Connection) {
        let _ = self.conn.set(conn);
    }

    pub fn connection(&self) -> Option<&Connection> {
        self.conn.get()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(crate) fn lock_state(&self) -> std::sync::MutexGuard<'_, State> {
        self.lock()
    }

    pub fn load_token(&self) -> std::io::Result<()> {
        if let Ok(bytes) = std::fs::read(&self.cfg.token_file) {
            let token = trim_token(&String::from_utf8_lossy(&bytes));
            if !token.is_empty() {
                self.lock().token = token;
                logf!(
                    self.logger,
                    "loaded token from {}",
                    self.cfg.token_file.display()
                );
                return Ok(());
            }
        }

        let value = random_u64()? % 900_000 + 100_000;
        let token = format!("{value:06}");
        let dir = directory_of(&self.cfg.token_file);
        create_private_dir(&dir)?;
        write_mode(&self.cfg.token_file, format!("{token}\n").as_bytes(), 0o600)?;
        self.lock().token = token;
        logf!(
            self.logger,
            "generated token in {}",
            self.cfg.token_file.display()
        );
        Ok(())
    }

    pub async fn set_status(&self, status: &str) {
        let status_file = {
            let mut state = self.lock();
            state.status = status.to_string();
            drop(state);
            self.status_file.clone()
        };
        logf!(self.logger, "status: {status}");
        if !status_file.as_os_str().is_empty() {
            let _ = write_mode(&status_file, format!("{status}\n").as_bytes(), 0o644);
        }
        let characteristic = {
            let state = self.lock();
            state.by_name.get("status").cloned()
        };
        if let Some(characteristic) = characteristic {
            self.set_characteristic_value(&characteristic, status.as_bytes())
                .await;
        }
    }

    pub async fn set_characteristic_value(&self, characteristic: &Characteristic, value: &[u8]) {
        characteristic.set_value(value);
        if characteristic.notifying() {
            self.emit_value(characteristic, value).await;
        }
    }

    pub fn snapshot(&self) -> ControlStatus {
        let state = self.lock();
        ControlStatus {
            status: state.status.clone(),
            manual: state
                .manual_until
                .is_some_and(|until| until > Instant::now()),
            bluetooth: state.bt_mode,
        }
    }

    pub fn advertise_enabled(&self) -> bool {
        self.lock().advertising
    }

    pub fn is_manual(&self) -> bool {
        let state = self.lock();
        state
            .manual_until
            .is_some_and(|until| until > Instant::now())
    }

    pub async fn enable_ble(self: &Arc<Self>) -> Result<(), String> {
        if self.lock().advertising {
            return Ok(());
        }
        if self.connection().is_none() {
            return Err("no dbus connection".to_string());
        }
        let needs_gatt = !self.lock().gatt_registered;
        if needs_gatt {
            self.register_gatt().await?;
            self.lock().gatt_registered = true;
        }
        self.set_adapter_connectable(true).await;
        self.register_advertisement().await?;
        self.lock().advertising = true;
        logf!(self.logger, "BLE advertising ON");
        self.set_status("READY").await;
        Ok(())
    }

    pub async fn disable_ble(&self) {
        if !self.advertise_enabled() {
            return;
        }
        self.lock().advertising = false;
        self.unregister_advertisement().await;
        if self.lock().gatt_registered {
            self.unregister_gatt().await;
            self.lock().gatt_registered = false;
        }
        logf!(self.logger, "BLE advertising OFF");
        self.set_status("IDLE").await;
    }

    pub async fn toggle_manual(self: &Arc<Self>) {
        let now = Instant::now();
        let open = {
            let mut state = self.lock();
            if state.manual_until.is_some_and(|until| until > now) {
                state.manual_until = None;
                false
            } else {
                state.manual_until = Some(now + self.timings.manual_window);
                true
            }
        };
        if !open {
            logf!(self.logger, "manual window closed");
            self.disable_ble().await;
            return;
        }
        logf!(self.logger, "manual window opened for 15 minutes");
        if let Err(err) = self.enable_ble().await {
            logf!(self.logger, "enable BLE: {err}");
        }
    }

    pub async fn open_manual(self: &Arc<Self>) {
        {
            let mut state = self.lock();
            state.manual_until = Some(Instant::now() + self.timings.manual_window);
        }
        logf!(self.logger, "manual window opened for 15 minutes");
        if let Err(err) = self.enable_ble().await {
            logf!(self.logger, "enable BLE: {err}");
        }
    }

    pub async fn close_manual(&self) {
        {
            let mut state = self.lock();
            state.manual_until = None;
        }
        self.disable_ble().await;
        logf!(self.logger, "manual window closed");
    }

    fn characteristic_bytes(&self, name: &str) -> Vec<u8> {
        let characteristic = {
            let state = self.lock();
            state.by_name.get(name).cloned()
        };
        characteristic.map_or_else(Vec::new, |c| c.value())
    }

    /// Validate a commit frame and, when accepted, start the Wi-Fi task.
    ///
    /// Status is written before returning, exactly like the Go handler; only
    /// the multi-second Wi-Fi apply runs in a background task.
    pub async fn handle_commit(self: &Arc<Self>, value: &[u8]) {
        let cmd = trim_bytes(value);
        logf!(
            self.logger,
            "commit request: {:?}",
            String::from_utf8_lossy(cmd)
        );
        if cmd.is_empty() {
            self.set_status("ERROR:EMPTY_COMMIT").await;
            return;
        }
        if cmd == b"CANCEL" {
            self.disable_ble().await;
            return;
        }
        let ssid_bytes = trim_bytes(&self.characteristic_bytes("ssid")).to_vec();
        let psk_bytes = trim_bytes(&self.characteristic_bytes("psk")).to_vec();
        let token_bytes = trim_bytes(&self.characteristic_bytes("token")).to_vec();
        let expected = self.lock().token.clone();
        if token_bytes != expected.as_bytes() {
            logf!(self.logger, "commit: bad token");
            self.set_status("ERROR:BAD_TOKEN").await;
            return;
        }
        if let Err(code) = validate_wifi(&ssid_bytes, &psk_bytes) {
            logf!(
                self.logger,
                "commit: invalid credentials: {}",
                String::from_utf8_lossy(code.as_bytes())
            );
            self.set_status(&format!("ERROR:{code}")).await;
            return;
        }
        let ssid = String::from_utf8_lossy(&ssid_bytes).into_owned();
        let psk = String::from_utf8_lossy(&psk_bytes).into_owned();
        {
            let mut state = self.lock();
            state.ssid.clone_from(&ssid);
            state.psk.clone_from(&psk);
        }
        self.set_status("CONNECTING").await;
        let gateway = self.clone();
        tokio::spawn(async move {
            gateway.apply_wifi(&ssid, &psk).await;
        });
    }

    pub async fn auto_loop(self: Arc<Self>) {
        let mut ticker = tokio::time::interval(self.timings.auto_tick);
        // The Go ticker fires after the first interval, not immediately.
        ticker.tick().await;
        loop {
            ticker.tick().await;
            if !self.cfg.auto_wifi {
                continue;
            }
            let (applying, manual) = {
                let state = self.lock();
                (
                    state.applying,
                    state
                        .manual_until
                        .is_some_and(|until| until > Instant::now()),
                )
            };
            if applying {
                continue;
            }
            let state = self.wpa.state().await;
            if state == "COMPLETED" {
                self.lock().disconnected_since = None;
                if self.advertise_enabled() && !manual {
                    logf!(
                        self.logger,
                        "Wi-Fi connected; stopping automatic advertisement"
                    );
                    self.disable_ble().await;
                }
                continue;
            }
            let first = {
                let mut guard = self.lock();
                if guard.disconnected_since.is_none() {
                    guard.disconnected_since = Some(Instant::now());
                    true
                } else {
                    false
                }
            };
            if first {
                logf!(self.logger, "Wi-Fi state {state:?}; auto timer started");
                continue;
            }
            let down_for = {
                let guard = self.lock();
                guard.disconnected_since.map(|since| since.elapsed())
            };
            if down_for.is_some_and(|elapsed| elapsed >= self.timings.disconnect_threshold)
                && !self.advertise_enabled()
                && !manual
            {
                logf!(
                    self.logger,
                    "Wi-Fi down for 20s; starting automatic BLE gateway"
                );
                if let Err(err) = self.enable_ble().await {
                    logf!(self.logger, "auto enable BLE: {err}");
                }
            }
        }
    }
}

/// Trim whitespace the way Go's `strings.TrimSpace` does. Valid UTF-8 uses the
/// Unicode definition; invalid byte sequences fall back to ASCII trimming so
/// they are still rejected by Wi-Fi validation instead of being rewritten.
pub fn trim_bytes(value: &[u8]) -> &[u8] {
    if let Ok(text) = std::str::from_utf8(value) {
        return text.trim().as_bytes();
    }
    let mut start = 0;
    let mut end = value.len();
    while start < end && value[start].is_ascii_whitespace() {
        start += 1;
    }
    while end > start && value[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    &value[start..end]
}

/// `os.MkdirAll(dir, 0700)` for secrets such as token files.
pub fn create_private_dir(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true).mode(0o700);
    builder.create(path)
}

/// Keep only digits and cap the result at 12 characters, like the Go daemon.
pub fn trim_token(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_digit)
        .take(12)
        .collect()
}

/// `status` lives next to the token file.
pub fn status_path(token_file: &Path) -> PathBuf {
    token_file
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .join("status")
}

pub fn directory_of(path: &Path) -> PathBuf {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

pub fn write_mode(path: &Path, bytes: &[u8], mode: u32) -> std::io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(mode)
        .open(path)?;
    file.write_all(bytes)
}

pub fn random_bytes(len: usize) -> std::io::Result<Vec<u8>> {
    let mut buffer = vec![0u8; len];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut buffer)?;
    Ok(buffer)
}

pub fn random_u64() -> std::io::Result<u64> {
    let bytes = random_bytes(8)?;
    let mut raw = [0u8; 8];
    raw.copy_from_slice(&bytes);
    Ok(u64::from_le_bytes(raw))
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;
    use crate::test_support::{new_gateway, read_file};

    #[test]
    fn trim_token_digits_and_limit() {
        let cases = [
            ("123456", "123456"),
            (" 12-34 56\n", "123456"),
            ("abc123def456xyz789012", "123456789012"),
            ("", ""),
            ("только текст", ""),
            ("0 1 2 3 4 5 6 7 8 9 10", "012345678910"),
        ];
        for (input, expected) in cases {
            assert_eq!(trim_token(input), expected, "trim_token({input:?})");
        }
    }

    #[test]
    fn load_token_creates_and_reads_file() {
        let context = new_gateway("token", Path::new("/nonexistent/wpa_cli"));
        let gateway = &context.gateway;
        gateway.load_token().expect("load token");
        assert_eq!(gateway.lock_state().token.len(), 6);
        let raw = read_file(&context.dir.join("token"));
        assert_eq!(raw, format!("{}\n", gateway.lock_state().token));
        let mode = std::fs::metadata(context.dir.join("token"))
            .expect("stat token")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);

        std::fs::write(context.dir.join("token"), " 987-654 \n").expect("write token");
        gateway.lock_state().token.clear();
        gateway.load_token().expect("reload token");
        assert_eq!(gateway.lock_state().token, "987654");
    }

    #[test]
    fn load_token_existing_empty_falls_back() {
        let context = new_gateway("token-invalid", Path::new("/nonexistent/wpa_cli"));
        std::fs::write(context.dir.join("token"), "not digits\n").expect("write token");
        context.gateway.load_token().expect("load token");
        assert_eq!(context.gateway.lock_state().token.len(), 6);
    }

    #[test]
    fn trim_bytes_matches_go_trim_space() {
        assert_eq!(trim_bytes(b"  Home\t\n"), b"Home");
        assert_eq!(trim_bytes(" \u{00a0}Home\u{2028} ".as_bytes()), b"Home");
        assert_eq!(trim_bytes(&[b' ', 0xff, b' ']), &[0xff]);
    }

    #[test]
    fn status_path_follows_token_dir() {
        assert_eq!(
            status_path(Path::new("/data/quasar/ble-gateway/token")),
            PathBuf::from("/data/quasar/ble-gateway/status")
        );
        assert_eq!(status_path(Path::new("token")), PathBuf::from("./status"));
    }
}
