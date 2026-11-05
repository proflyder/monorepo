//! Native `wpa_supplicant` control-interface client and the Wi-Fi apply /
//! rollback sequence.
//!
//! The daemon talks to `/var/run/wpa_supplicant/wlan0` over a unix datagram
//! socket instead of forking `wpa_cli`. Replies are typed in [`WpaStatus`] and
//! [`Network`]; every other command returns [`WpaError`].

use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use thiserror::Error;
use tokio::net::UnixDatagram;
use tokio::time::{Instant, timeout};
use tracing::{debug, info, warn};

use crate::gateway::Gateway;
use crate::status::Status;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const STATE_COMPLETED: &str = "COMPLETED";
static SOCKET_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn validate_wifi(ssid: &[u8], psk: &[u8]) -> Result<(), &'static str> {
    if ssid.is_empty() {
        return Err("EMPTY_SSID");
    }
    if ssid.len() > 32
        || std::str::from_utf8(ssid).is_err()
        || ssid.iter().any(|b| matches!(b, 0 | b'\n' | b'\r'))
    {
        return Err("BAD_SSID");
    }
    if psk.is_empty() {
        return Err("EMPTY_PSK");
    }
    if psk.len() == 64 {
        return psk
            .iter()
            .all(u8::is_ascii_hexdigit)
            .then_some(())
            .ok_or("BAD_PSK");
    }
    if !(8..=63).contains(&psk.len()) {
        return Err("BAD_PSK_LENGTH");
    }
    if psk.iter().any(|b| matches!(b, 0 | b'\n' | b'\r')) {
        return Err("BAD_PSK");
    }
    Ok(())
}

/// Quote an SSID/PSK for `SET_NETWORK`, escaping `\` and `"`.
pub fn wpa_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// `SET_NETWORK <id> <field> <value>` may carry the Wi-Fi password. Every
/// error stores only the verb, id and field; the value is never formatted.
#[must_use]
pub fn safe_command(command: &str) -> String {
    let mut parts = command.split_whitespace();
    if parts.next() == Some("SET_NETWORK") {
        let id = parts.next().unwrap_or("");
        let field = parts.next().unwrap_or("");
        return format!("SET_NETWORK {id} {field}").trim().to_string();
    }
    command.to_string()
}

#[derive(Debug, Error)]
pub enum WpaError {
    #[error("wpa_supplicant io: {0}")]
    Io(#[from] io::Error),
    #[error("wpa_supplicant command {command:?} timed out")]
    Timeout { command: String },
    #[error("wpa_supplicant command {command:?} failed: {reply}")]
    Failed { command: String, reply: String },
    #[error("unexpected wpa_supplicant reply to {command:?}: {reply}")]
    Unexpected { command: String, reply: String },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WpaStatus {
    pub state: String,
}

impl WpaStatus {
    fn parse(reply: &str) -> Self {
        Self {
            state: reply
                .lines()
                .find_map(|line| line.strip_prefix("wpa_state="))
                .unwrap_or("UNKNOWN")
                .to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Network {
    pub id: i32,
    pub ssid: String,
    pub flags: String,
}

impl Network {
    pub fn is_current(&self) -> bool {
        self.flags.contains("[CURRENT]")
    }

    pub fn is_enabled(&self) -> bool {
        !self.flags.contains("[DISABLED]")
    }
}

/// Client for one `wpa_supplicant` control socket.
#[derive(Clone, Debug)]
pub struct WpaClient {
    ctrl: PathBuf,
    socket_dir: PathBuf,
}

impl WpaClient {
    pub fn new(ctrl: impl Into<PathBuf>) -> Self {
        Self {
            ctrl: ctrl.into(),
            socket_dir: short_socket_directory(),
        }
    }

    /// Override the reply-socket directory (tests).
    pub fn with_socket_dir(mut self, directory: impl Into<PathBuf>) -> Self {
        self.socket_dir = directory.into();
        self
    }

    pub async fn request(&self, command: &str) -> Result<String, WpaError> {
        let safe = safe_command(command);
        let path = self.socket_dir.join(format!(
            "rw-{}-{}.sock",
            std::process::id(),
            SOCKET_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        if let Err(error) = std::fs::remove_file(&path)
            && error.kind() != io::ErrorKind::NotFound
        {
            debug!(%error, "remove stale wpa reply socket");
        }
        let socket = UnixDatagram::bind(&path)?;
        let _cleanup = SocketCleanup(path);
        socket.connect(&self.ctrl)?;
        socket.send(command.as_bytes()).await?;
        let mut buffer = [0u8; 8192];
        let received = timeout(REQUEST_TIMEOUT, socket.recv(&mut buffer))
            .await
            .map_err(|_| WpaError::Timeout {
                command: safe.clone(),
            })??;
        let reply = String::from_utf8_lossy(&buffer[..received])
            .trim_end()
            .to_string();
        if reply.starts_with("FAIL") {
            let reply = if safe.starts_with("SET_NETWORK ") {
                "FAIL".to_string()
            } else {
                reply
            };
            return Err(WpaError::Failed {
                command: safe,
                reply,
            });
        }
        Ok(reply)
    }

    async fn expect_ok(&self, command: &str) -> Result<(), WpaError> {
        match self.request(command).await? {
            reply if reply == "OK" => Ok(()),
            reply => Err(WpaError::Unexpected {
                command: safe_command(command),
                reply: if command.starts_with("SET_NETWORK ") {
                    "FAIL".to_string()
                } else {
                    reply
                },
            }),
        }
    }

    pub async fn status(&self) -> Result<WpaStatus, WpaError> {
        Ok(WpaStatus::parse(&self.request("STATUS").await?))
    }

    /// State for the UI/auto loop; a broken socket reads as `UNKNOWN`.
    pub async fn state(&self) -> String {
        match self.status().await {
            Ok(status) => status.state,
            Err(error) => {
                debug!(%error, "wpa_supplicant STATUS failed");
                "UNKNOWN".to_string()
            }
        }
    }

    pub async fn list_networks(&self) -> Result<Vec<Network>, WpaError> {
        Ok(self
            .request("LIST_NETWORKS")
            .await?
            .lines()
            .filter_map(|line| {
                let mut fields = line.split('\t');
                Some(Network {
                    id: fields.next()?.trim().parse().ok()?,
                    ssid: fields.next().unwrap_or_default().to_string(),
                    flags: fields.nth(1).unwrap_or_default().to_string(),
                })
            })
            .collect())
    }

    pub async fn current_network(&self) -> Option<i32> {
        self.list_networks()
            .await
            .ok()?
            .iter()
            .find(|network| network.is_current())
            .map(|network| network.id)
    }

    pub async fn enabled_networks(&self) -> Vec<i32> {
        self.list_networks()
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(Network::is_enabled)
            .map(|network| network.id)
            .collect()
    }

    pub async fn add_network(&self) -> Result<i32, WpaError> {
        let reply = self.request("ADD_NETWORK").await?;
        reply.trim().parse().map_err(|_| WpaError::Unexpected {
            command: "ADD_NETWORK".to_string(),
            reply,
        })
    }

    pub async fn set_network(&self, id: i32, field: &str, value: &str) -> Result<(), WpaError> {
        self.expect_ok(&format!("SET_NETWORK {id} {field} {value}"))
            .await
    }

    pub async fn enable_network(&self, id: i32) -> Result<(), WpaError> {
        self.expect_ok(&format!("ENABLE_NETWORK {id}")).await
    }

    pub async fn select_network(&self, id: i32) -> Result<(), WpaError> {
        self.expect_ok(&format!("SELECT_NETWORK {id}")).await
    }

    pub async fn remove_network(&self, id: i32) -> Result<(), WpaError> {
        self.expect_ok(&format!("REMOVE_NETWORK {id}")).await
    }

    pub async fn save_config(&self) -> Result<(), WpaError> {
        self.expect_ok("SAVE_CONFIG").await
    }

    pub async fn reconnect(&self) -> Result<(), WpaError> {
        self.expect_ok("RECONNECT").await
    }

    /// Add, fill and remove a temporary network. Every failure, including a
    /// failed cleanup, is an error so the process exit code reflects reality.
    pub async fn test_wpa(&self) -> Result<String, WpaError> {
        let id = self.add_network().await?;
        let result = async {
            self.set_network(id, "ssid", &wpa_quote("Test Net")).await?;
            self.set_network(id, "psk", &wpa_quote("examplepass"))
                .await?;
            self.set_network(id, "key_mgmt", "WPA-PSK").await?;
            Ok::<(), WpaError>(())
        }
        .await;
        let removed = self.remove_network(id).await;
        result?;
        removed?;
        let count = self.list_networks().await?.len();
        Ok(format!(
            "temporary network id={id} set OK; {count} network(s) configured"
        ))
    }
}

struct SocketCleanup(PathBuf);

impl Drop for SocketCleanup {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.0)
            && error.kind() != io::ErrorKind::NotFound
        {
            debug!(%error, "remove wpa reply socket");
        }
    }
}

/// Unix socket paths are limited to `SUN_LEN`; the default temp directory is
/// already too deep on macOS, so the reply socket lives in `/tmp`.
fn short_socket_directory() -> PathBuf {
    let short = PathBuf::from("/tmp");
    if short.is_dir() {
        short
    } else {
        std::env::temp_dir()
    }
}

impl Gateway {
    pub(crate) async fn apply_wifi(self: &Arc<Self>, ssid: &str, psk: &str) {
        if self.cfg.dry_run {
            self.set_status(Status::DryRunOk);
            return;
        }
        self.applying.store(true, Ordering::SeqCst);
        let outcome = self.apply_wifi_inner(ssid, psk).await;
        self.applying.store(false, Ordering::SeqCst);
        if let Err(code) = outcome {
            self.set_status(Status::error(code));
        }
    }

    async fn apply_wifi_inner(&self, ssid: &str, psk: &str) -> Result<(), &'static str> {
        let old = self.wpa.current_network().await;
        let prior = self.wpa.enabled_networks().await;
        info!(?old, ?prior, "Wi-Fi apply");
        let id = match self.wpa.add_network().await {
            Ok(id) => id,
            Err(error) => {
                warn!(%error, "add_network");
                return Err("ADD_NETWORK");
            }
        };
        if let Err(code) = self.configure(id, ssid, psk).await {
            self.rollback(id, old).await;
            return Err(code);
        }
        let mut state = "UNKNOWN".to_string();
        let deadline = Instant::now() + self.timings.wifi_apply_timeout;
        while Instant::now() < deadline {
            state = self.wpa.state().await;
            if state == STATE_COMPLETED {
                break;
            }
            tokio::time::sleep(self.timings.wifi_poll_interval).await;
        }
        if state != STATE_COMPLETED {
            warn!(%state, "new network did not connect; rolling back");
            self.rollback(id, old).await;
            self.re_enable(&prior).await;
            return Err("TIMEOUT");
        }
        self.re_enable(&prior).await;
        if let Err(error) = self.wpa.save_config().await {
            warn!(%error, "save_config");
        } else {
            info!("saved Wi-Fi configuration");
        }
        info!("Wi-Fi connected");
        self.set_status(Status::Connected);
        tokio::time::sleep(self.timings.wifi_post_connect_delay).await;
        Ok(())
    }

    async fn configure(&self, id: i32, ssid: &str, psk: &str) -> Result<(), &'static str> {
        for (field, value) in [
            ("ssid", wpa_quote(ssid)),
            ("psk", wpa_quote(psk)),
            ("key_mgmt", "WPA-PSK".to_string()),
            ("priority", "10".to_string()),
        ] {
            self.wpa
                .set_network(id, field, &value)
                .await
                .map_err(|error| {
                    warn!(%error, field, "set_network");
                    "SET_NETWORK"
                })?;
        }
        self.wpa.enable_network(id).await.map_err(|error| {
            warn!(%error, "enable_network");
            "ENABLE_NETWORK"
        })?;
        self.wpa.select_network(id).await.map_err(|error| {
            warn!(%error, "select_network");
            "SELECT_NETWORK"
        })
    }

    async fn rollback(&self, id: i32, old: Option<i32>) {
        if id >= 0
            && let Err(error) = self.wpa.remove_network(id).await
        {
            warn!(%error, id, "remove_network");
        }
        let Some(old) = old else {
            return info!("no previous network to restore");
        };
        if let Err(error) = self.wpa.select_network(old).await {
            warn!(%error, id = old, "restore old network");
        }
    }

    async fn re_enable(&self, ids: &[i32]) {
        for id in ids {
            if let Err(error) = self.wpa.enable_network(*id).await {
                warn!(%error, id, "re-enable network");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::significant_drop_tightening)]

    use std::sync::Mutex;

    use super::*;
    use crate::test_support::TestGateway;

    type ValidationCase = (&'static str, Vec<u8>, Vec<u8>, Option<&'static str>);

    fn validation_cases() -> Vec<ValidationCase> {
        vec![
            ("ok", b"Home".to_vec(), b"password123".to_vec(), None),
            ("ok hex 64", b"Home".to_vec(), vec![b'a'; 64], None),
            (
                "empty ssid",
                Vec::new(),
                b"password123".to_vec(),
                Some("EMPTY_SSID"),
            ),
            (
                "ssid too long",
                vec![b'x'; 33],
                b"password123".to_vec(),
                Some("BAD_SSID"),
            ),
            (
                "ssid newline",
                b"Home\n".to_vec(),
                b"password123".to_vec(),
                Some("BAD_SSID"),
            ),
            ("empty psk", b"Home".to_vec(), Vec::new(), Some("EMPTY_PSK")),
            (
                "short psk",
                b"Home".to_vec(),
                b"short".to_vec(),
                Some("BAD_PSK_LENGTH"),
            ),
            (
                "long psk",
                b"Home".to_vec(),
                vec![b'x'; 64],
                Some("BAD_PSK"),
            ),
            (
                "psk newline",
                b"Home".to_vec(),
                b"password\n123".to_vec(),
                Some("BAD_PSK"),
            ),
            (
                "invalid utf8 ssid",
                vec![0xff, 0xfe],
                b"password123".to_vec(),
                Some("BAD_SSID"),
            ),
        ]
    }

    #[test]
    fn validate_wifi_cases() {
        for (name, ssid, psk, expected) in validation_cases() {
            assert_eq!(validate_wifi(&ssid, &psk).err(), expected, "{name}");
        }
    }

    #[test]
    fn wpa_quote_escapes() {
        for (input, expected) in [
            ("Home", "\"Home\""),
            ("a\"b", "\"a\\\"b\""),
            ("a\\b", "\"a\\\\b\""),
            ("Дом 5G", "\"Дом 5G\""),
        ] {
            assert_eq!(wpa_quote(input), expected, "wpa_quote({input:?})");
        }
    }

    fn recording_wpa() -> (TestGateway, Arc<Mutex<Vec<String>>>) {
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let sink = recorded.clone();
        let gateway = TestGateway::new("wpa-recording", move |command| {
            sink.lock().expect("commands").push(command.to_string());
            match command {
                "STATUS" => "wpa_state=COMPLETED".to_string(),
                "LIST_NETWORKS" => {
                    "network id / ssid / bssid / flags\n0\tHome\tany\t[CURRENT]\n1\tOld\tany\t[DISABLED]\n2\tWork\tany".to_string()
                }
                "ADD_NETWORK" => "7".to_string(),
                _ => "OK".to_string(),
            }
        });
        (gateway, recorded)
    }

    #[tokio::test]
    async fn client_parses_status_and_networks() {
        let (gateway, _) = recording_wpa();
        let status = gateway.gateway.wpa.status().await.expect("status");
        assert_eq!(status.state, "COMPLETED");
        assert_eq!(status.state, "COMPLETED");
        let networks = gateway.gateway.wpa.list_networks().await.expect("networks");
        assert_eq!(networks.len(), 3);
        assert_eq!(networks[0].id, 0);
        assert!(networks[0].is_current());
        assert!(!networks[1].is_enabled());
        assert_eq!(gateway.gateway.wpa.current_network().await, Some(0));
        assert_eq!(gateway.gateway.wpa.enabled_networks().await, vec![0, 2]);
    }

    #[tokio::test]
    async fn apply_wifi_success_persists_and_restores_fallback() {
        let (gateway, recorded) = recording_wpa();
        gateway.gateway.apply_wifi("Home", "password123").await;
        assert_eq!(gateway.gateway.status(), Status::Connected);
        let commands = recorded.lock().expect("commands");
        for expected in [
            "ADD_NETWORK",
            "SET_NETWORK 7 ssid \"Home\"",
            "SET_NETWORK 7 psk \"password123\"",
            "SET_NETWORK 7 key_mgmt WPA-PSK",
            "SET_NETWORK 7 priority 10",
            "ENABLE_NETWORK 7",
            "SELECT_NETWORK 7",
            "ENABLE_NETWORK 0",
            "ENABLE_NETWORK 2",
            "SAVE_CONFIG",
        ] {
            assert!(commands.iter().any(|c| c == expected), "missing {expected}");
        }
    }

    #[tokio::test]
    async fn add_network_failure_reports_add_network() {
        let gateway = TestGateway::new("wpa-add-fail", |command| match command {
            "LIST_NETWORKS" => "0\tHome\tany\t[CURRENT]".to_string(),
            "ADD_NETWORK" => "FAIL".to_string(),
            _ => "OK".to_string(),
        });
        gateway.gateway.apply_wifi("Home", "password123").await;
        assert_eq!(gateway.gateway.status(), Status::error("ADD_NETWORK"));
    }

    #[tokio::test]
    async fn set_network_failure_rolls_back_to_the_old_network() {
        let commands = Arc::new(Mutex::new(Vec::new()));
        let sink = commands.clone();
        let gateway = TestGateway::new("wpa-set-fail", move |command| {
            sink.lock().expect("commands").push(command.to_string());
            match command {
                "LIST_NETWORKS" => "0\tHome\tany\t[CURRENT]".to_string(),
                "ADD_NETWORK" => "7".to_string(),
                command if command.starts_with("SET_NETWORK 7 psk") => "FAIL".to_string(),
                _ => "OK".to_string(),
            }
        });
        gateway.gateway.apply_wifi("Home", "password123").await;
        assert_eq!(gateway.gateway.status(), Status::error("SET_NETWORK"));
        let commands = commands.lock().expect("commands");
        assert!(commands.iter().any(|c| c == "REMOVE_NETWORK 7"));
        assert!(commands.iter().any(|c| c == "SELECT_NETWORK 0"));
    }

    #[tokio::test(start_paused = true)]
    async fn timeout_rolls_back_and_reenables_prior_networks() {
        let commands = Arc::new(Mutex::new(Vec::new()));
        let sink = commands.clone();
        let gateway = TestGateway::new("wpa-timeout", move |command| {
            sink.lock().expect("commands").push(command.to_string());
            match command {
                "STATUS" => "wpa_state=DISCONNECTED".to_string(),
                "LIST_NETWORKS" => "0\tHome\tany\t[CURRENT]\n1\tFallback\tany".to_string(),
                "ADD_NETWORK" => "7".to_string(),
                _ => "OK".to_string(),
            }
        });
        gateway.gateway.apply_wifi("Home", "password123").await;
        assert_eq!(gateway.gateway.status(), Status::error("TIMEOUT"));
        let commands = commands.lock().expect("commands");
        assert!(commands.iter().any(|c| c == "REMOVE_NETWORK 7"));
        assert!(commands.iter().any(|c| c == "SELECT_NETWORK 0"));
        assert!(commands.iter().any(|c| c == "ENABLE_NETWORK 0"));
        assert!(commands.iter().any(|c| c == "ENABLE_NETWORK 1"));
    }

    #[tokio::test]
    async fn dry_run_never_touches_wpa_supplicant() {
        let gateway = TestGateway::new_dry_run("wpa-dry-run");
        gateway.gateway.apply_wifi("Home", "password123").await;
        assert_eq!(gateway.gateway.status(), Status::DryRunOk);
    }

    #[tokio::test]
    async fn test_wpa_succeeds_and_cleans_up() {
        let commands = Arc::new(Mutex::new(Vec::new()));
        let sink = commands.clone();
        let gateway = TestGateway::new("wpa-test-ok", move |command| {
            sink.lock().expect("commands").push(command.to_string());
            match command {
                "ADD_NETWORK" => "9".to_string(),
                "LIST_NETWORKS" => "0\tHome\tany\t[CURRENT]".to_string(),
                _ => "OK".to_string(),
            }
        });
        let message = gateway.gateway.wpa.test_wpa().await.expect("test-wpa");
        assert!(message.contains("id=9"), "{message}");
        assert!(
            commands
                .lock()
                .expect("commands")
                .iter()
                .any(|c| c == "REMOVE_NETWORK 9")
        );
    }

    #[tokio::test]
    async fn test_wpa_failure_is_reported() {
        let gateway = TestGateway::new("wpa-test-fail", |command| match command {
            "ADD_NETWORK" => "9".to_string(),
            command if command.starts_with("SET_NETWORK") => "FAIL".to_string(),
            _ => "OK".to_string(),
        });
        assert!(matches!(
            gateway.gateway.wpa.test_wpa().await,
            Err(WpaError::Failed { .. })
        ));
    }

    #[tokio::test]
    async fn set_network_error_never_contains_the_psk() {
        let gateway = TestGateway::new("wpa-secret", |command| {
            if command.starts_with("SET_NETWORK") {
                "FAIL".to_string()
            } else {
                "OK".to_string()
            }
        });
        let error = gateway
            .gateway
            .wpa
            .set_network(3, "psk", &wpa_quote("super-secret-password"))
            .await
            .expect_err("write must fail");
        for rendered in [error.to_string(), format!("{error:?}")] {
            assert!(!rendered.contains("super-secret-password"), "{rendered}");
            assert!(rendered.contains("SET_NETWORK 3 psk"), "{rendered}");
        }
    }

    #[tokio::test]
    async fn missing_socket_is_an_error_not_a_hang() {
        let client = WpaClient::new("/nonexistent/wpa.sock");
        assert!(matches!(client.status().await, Err(WpaError::Io(_))));
        assert_eq!(client.state().await, "UNKNOWN");
    }

    #[tokio::test]
    async fn fake_wpa_helper_is_reachable() {
        let gateway = TestGateway::new("wpa-helper", |_| "OK".to_string());
        assert!(gateway.gateway.wpa.ctrl.exists());
    }
}
