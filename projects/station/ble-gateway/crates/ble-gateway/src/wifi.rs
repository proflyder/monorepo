//! `wpa_cli` interaction and the Wi-Fi apply/rollback sequence.

use std::process::Stdio;
use std::sync::Arc;
use std::time::Instant;

use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::gateway::Gateway;
use crate::logf;

const WPA_CLI: &str = "/usr/sbin/wpa_cli";

/// Wi-Fi credential validation shared by the GATT commit path and tests.
pub fn validate_wifi(ssid: &[u8], psk: &[u8]) -> Result<(), &'static str> {
    if ssid.is_empty() {
        return Err("EMPTY_SSID");
    }
    if ssid.len() > 32 || std::str::from_utf8(ssid).is_err() {
        return Err("BAD_SSID");
    }
    if ssid.iter().any(|b| *b == 0 || *b == b'\n' || *b == b'\r') {
        return Err("BAD_SSID");
    }
    if psk.is_empty() {
        return Err("EMPTY_PSK");
    }
    if psk.len() == 64 {
        if psk.iter().all(|b| b.is_ascii_hexdigit()) {
            return Ok(());
        }
        return Err("BAD_PSK");
    }
    if psk.len() < 8 || psk.len() > 63 {
        return Err("BAD_PSK_LENGTH");
    }
    if psk.iter().any(|b| *b == 0 || *b == b'\n' || *b == b'\r') {
        return Err("BAD_PSK");
    }
    Ok(())
}

/// Escape an SSID/PSK for the interactive `set_network` command.
pub fn wpa_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        if ch == '\\' || ch == '"' {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push('"');
    out
}

fn combine(output: std::process::Output) -> (String, bool) {
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    (text.trim().to_string(), output.status.success())
}

/// One invocation of the configured `wpa_cli` binary.
#[derive(Clone)]
pub struct Wpa {
    path: String,
}

impl Wpa {
    pub fn system() -> Self {
        Self {
            path: WPA_CLI.to_string(),
        }
    }

    pub fn with_path(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    async fn cli(&self, args: &[&str]) -> (String, bool) {
        let output = Command::new(&self.path)
            .arg("-i")
            .arg("wlan0")
            .args(args)
            .output()
            .await;
        match output {
            Ok(output) => combine(output),
            Err(err) => (err.to_string(), false),
        }
    }

    /// One-shot argv mode cannot reliably carry SSID/PSK values, and stdin
    /// keeps the PSK out of the process argv.
    async fn interactive(&self, commands: &[String]) -> (String, bool) {
        let mut child = match Command::new(&self.path)
            .arg("-i")
            .arg("wlan0")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(err) => return (err.to_string(), false),
        };
        let input = format!("{}\n", commands.join("\n"));
        if let Some(mut stdin) = child.stdin.take()
            && let Err(err) = stdin.write_all(input.as_bytes()).await
        {
            return (err.to_string(), false);
        }
        match child.wait_with_output().await {
            Ok(output) => combine(output),
            Err(err) => (err.to_string(), false),
        }
    }

    pub async fn state(&self) -> String {
        let (out, ok) = self.cli(&["status"]).await;
        if !ok && out.is_empty() {
            return "UNKNOWN".to_string();
        }
        for line in out.lines() {
            if let Some(state) = line.strip_prefix("wpa_state=") {
                return state.to_string();
            }
        }
        "UNKNOWN".to_string()
    }

    pub async fn current_network(&self) -> i32 {
        let (out, ok) = self.cli(&["list_networks"]).await;
        if !ok {
            return -1;
        }
        for line in out.lines() {
            if !line.contains("[CURRENT]") {
                continue;
            }
            if let Some(field) = line.split('\t').next()
                && let Ok(id) = field.trim().parse::<i32>()
            {
                return id;
            }
        }
        -1
    }

    pub async fn enabled_networks(&self) -> Vec<i32> {
        let (out, ok) = self.cli(&["list_networks"]).await;
        if !ok {
            return Vec::new();
        }
        let mut ids = Vec::new();
        for line in out.lines() {
            if line.contains("[DISABLED]") || !line.contains('\t') {
                continue;
            }
            if let Some(field) = line.split('\t').next()
                && let Ok(id) = field.trim().parse::<i32>()
            {
                ids.push(id);
            }
        }
        ids
    }

    async fn set_network(&self, id: i32, field: &str, value: &str) -> Result<(), String> {
        let (out, ok) = self
            .interactive(&[format!("set_network {id} {field} {value}")])
            .await;
        if !ok {
            return Err(format!("command failed: {out}"));
        }
        if !out.contains("OK") {
            return Err(format!("not OK: {out}"));
        }
        Ok(())
    }

    async fn enable_network(&self, id: i32) -> (String, bool) {
        self.cli(&["enable_network", &id.to_string()]).await
    }

    async fn select_network(&self, id: i32) -> (String, bool) {
        self.cli(&["select_network", &id.to_string()]).await
    }

    async fn remove_network(&self, id: i32) -> (String, bool) {
        self.cli(&["remove_network", &id.to_string()]).await
    }

    async fn add_network(&self) -> (String, bool) {
        self.cli(&["add_network"]).await
    }

    async fn save_config(&self) -> (String, bool) {
        self.cli(&["save_config"]).await
    }

    pub async fn reconnect(&self) -> (String, bool) {
        self.cli(&["reconnect"]).await
    }

    /// `--test-wpa`: always returns an error carrying the result, like Go.
    pub async fn test_wpa_set(&self) -> Result<(), String> {
        let (out, ok) = self.add_network().await;
        if !ok {
            return Err(format!("add_network: {out}"));
        }
        let id = out
            .trim()
            .parse::<i32>()
            .map_err(|_| format!("add_network output {out:?}"))?;

        let mut failure = None;
        for (field, value) in [
            ("ssid", wpa_quote("Test Net")),
            ("psk", wpa_quote("examplepass")),
            ("key_mgmt", "WPA-PSK".to_string()),
        ] {
            if let Err(err) = self.set_network(id, field, &value).await {
                failure = Some(format!("set {field}: {err}"));
                break;
            }
        }
        if failure.is_none() {
            let (list, ok) = self.cli(&["list_networks"]).await;
            if !ok {
                failure = Some(list);
            } else {
                failure = Some(format!("temporary network id={id} set OK; list:\n{list}"));
            }
        }
        let _ = self.remove_network(id).await;
        Err(failure.unwrap_or_else(|| "test-wpa failed".to_string()))
    }

    async fn re_enable(&self, ids: &[i32], gateway: &Gateway) {
        for id in ids {
            let (out, ok) = self.enable_network(*id).await;
            if !ok {
                logf!(gateway.logger, "re-enable network {id}: {out}");
            }
        }
    }
}

impl Gateway {
    /// Apply a network with rollback to the previous profile on any failure.
    pub async fn apply_wifi(self: &Arc<Self>, ssid: &str, psk: &str) {
        if self.cfg.dry_run {
            self.set_status("DRY_RUN_OK").await;
            return;
        }
        self.lock_state().applying = true;
        self.apply_wifi_inner(ssid, psk).await;
        self.lock_state().applying = false;
    }

    async fn apply_wifi_inner(&self, ssid: &str, psk: &str) {
        let old_id = self.wpa.current_network().await;
        let prior_enabled = self.wpa.enabled_networks().await;
        logf!(
            self.logger,
            "Wi-Fi apply: old network id={old_id}; enabled={prior_enabled:?}; trying SSID {ssid:?}"
        );

        let (out, ok) = self.wpa.add_network().await;
        if !ok {
            logf!(self.logger, "add_network: {out}");
            self.set_status("ERROR:ADD_NETWORK").await;
            return;
        }
        let id = match out.trim().parse::<i32>() {
            Ok(id) => id,
            Err(_) => {
                logf!(self.logger, "add_network: bad output {out:?}");
                self.set_status("ERROR:ADD_NETWORK").await;
                return;
            }
        };
        logf!(self.logger, "new network id={id}");

        for (field, value) in [
            ("ssid", wpa_quote(ssid)),
            ("psk", wpa_quote(psk)),
            ("key_mgmt", "WPA-PSK".to_string()),
            ("priority", "10".to_string()),
        ] {
            if let Err(err) = self.wpa.set_network(id, field, &value).await {
                logf!(self.logger, "set_network {field}: {err}");
                self.cleanup_new_network(id).await;
                self.restore_old(old_id).await;
                self.set_status("ERROR:SET_NETWORK").await;
                return;
            }
        }
        let (out, ok) = self.wpa.enable_network(id).await;
        if !ok {
            logf!(self.logger, "enable_network: {out}");
            self.cleanup_new_network(id).await;
            self.restore_old(old_id).await;
            self.set_status("ERROR:ENABLE_NETWORK").await;
            return;
        }
        let (out, ok) = self.wpa.select_network(id).await;
        if !ok {
            logf!(self.logger, "select_network: {out}");
            self.cleanup_new_network(id).await;
            self.restore_old(old_id).await;
            self.set_status("ERROR:SELECT_NETWORK").await;
            return;
        }

        let deadline = Instant::now() + self.timings.wifi_apply_timeout;
        let mut state = "UNKNOWN".to_string();
        while Instant::now() < deadline {
            state = self.wpa.state().await;
            if state == "COMPLETED" {
                break;
            }
            tokio::time::sleep(self.timings.wifi_poll_interval).await;
        }
        if state != "COMPLETED" {
            logf!(
                self.logger,
                "new network failed: state={state}; rolling back"
            );
            self.cleanup_new_network(id).await;
            self.restore_old(old_id).await;
            self.wpa.re_enable(&prior_enabled, self).await;
            self.set_status("ERROR:TIMEOUT").await;
            return;
        }

        // select_network disables the other profiles; restore the previous one
        // as a lower-priority fallback and persist both.
        self.wpa.re_enable(&prior_enabled, self).await;
        let (out, ok) = self.wpa.save_config().await;
        if ok {
            logf!(self.logger, "saved Wi-Fi config");
        } else {
            logf!(self.logger, "save_config: {out}");
        }
        logf!(self.logger, "Wi-Fi connected: SSID {ssid:?}");
        self.set_status("CONNECTED").await;
        tokio::time::sleep(self.timings.wifi_post_connect_delay).await;
    }

    async fn cleanup_new_network(&self, id: i32) {
        if id < 0 {
            return;
        }
        let (out, ok) = self.wpa.remove_network(id).await;
        if !ok {
            logf!(self.logger, "remove_network {id}: {out}");
        }
    }

    async fn restore_old(&self, id: i32) {
        if id < 0 {
            logf!(self.logger, "no previous network to restore");
            return;
        }
        let (out, ok) = self.wpa.select_network(id).await;
        if !ok {
            logf!(self.logger, "restore old {id}: {out}");
            return;
        }
        logf!(self.logger, "restored old network id={id}");
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use super::*;
    use crate::test_support::{fake_wpa, new_gateway, new_gateway_live, read_file, temp_dir};

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
        let cases = [
            ("Home", "\"Home\""),
            ("a\"b", "\"a\\\"b\""),
            ("a\\b", "\"a\\\\b\""),
            ("Дом 5G", "\"Дом 5G\""),
        ];
        for (input, expected) in cases {
            assert_eq!(wpa_quote(input), expected, "wpa_quote({input:?})");
        }
    }

    #[tokio::test]
    async fn wpa_queries_and_set_network() {
        let dir = temp_dir("wpa-queries");
        let path = fake_wpa(
            &dir,
            r#"case "$*" in
  *" status"*) echo wpa_state=COMPLETED ;;
  *" list_networks"*) printf '0\tHome\tany\t[CURRENT]\n1\tOld\tany\t[DISABLED]\n2\tWork\tany\n' ;;
  *" add_network"*) echo 7 ;;
  *) echo OK ;;
esac"#,
        );
        let wpa = Wpa::with_path(path.to_string_lossy().into_owned());
        assert_eq!(wpa.state().await, "COMPLETED");
        assert_eq!(wpa.current_network().await, 0);
        assert_eq!(wpa.enabled_networks().await, vec![0, 2]);
        wpa.set_network(7, "ssid", &wpa_quote("Test"))
            .await
            .expect("set_network");
        let error = wpa
            .test_wpa_set()
            .await
            .expect_err("test_wpa_set returns error");
        assert!(
            error.contains("temporary network id=7"),
            "unexpected error: {error}"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn wpa_unknown_when_cli_empty() {
        let dir = temp_dir("wpa-empty");
        let path = fake_wpa(&dir, "exit 1");
        let wpa = Wpa::with_path(path.to_string_lossy().into_owned());
        assert_eq!(wpa.state().await, "UNKNOWN");
        assert_eq!(wpa.current_network().await, -1);
        assert!(wpa.enabled_networks().await.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn apply_wifi_success() {
        let dir = temp_dir("wifi-success-script");
        let path = fake_wpa(
            &dir,
            r#"case "$*" in
  *" status"*) echo wpa_state=COMPLETED ;;
  *" list_networks"*) printf '0\tHome\tany\t[CURRENT]\n1\tFallback\tany\n' ;;
  *" add_network"*) echo 7 ;;
  *" enable_network"*) echo OK ;;
  *" select_network"*) echo OK ;;
  *" save_config"*) echo OK ;;
  *) echo OK ;;
esac"#,
        );
        let context = new_gateway_live("wifi-success", &path);
        let gateway = context.gateway.clone();
        gateway.apply_wifi("Home", "password123").await;
        assert_eq!(read_file(&context.dir.join("status")).trim(), "CONNECTED");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn apply_wifi_add_network_failure() {
        let dir = temp_dir("wifi-add-fail-script");
        let path = fake_wpa(
            &dir,
            r#"case "$*" in
  *" list_networks"*) printf '0\tHome\tany\t[CURRENT]\n' ;;
  *" add_network"*) echo FAIL; exit 1 ;;
esac"#,
        );
        let context = new_gateway_live("wifi-add-fail", &path);
        let gateway = context.gateway.clone();
        gateway.apply_wifi("Home", "password123").await;
        assert_eq!(
            read_file(&context.dir.join("status")).trim(),
            "ERROR:ADD_NETWORK"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn apply_wifi_set_network_failure_rolls_back() {
        let dir = temp_dir("wifi-set-fail-script");
        let path = fake_wpa(
            &dir,
            r#"case "$*" in
  *" list_networks"*) printf '0\tHome\tany\t[CURRENT]\n1\tFallback\tany\n' ;;
  *" add_network"*) echo 7 ;;
  *" set_network "*) echo FAIL; exit 1 ;;
  *" select_network "*) echo OK ;;
esac"#,
        );
        let context = new_gateway_live("wifi-set-fail", &path);
        let gateway = context.gateway.clone();
        gateway.apply_wifi("Home", "password123").await;
        assert_eq!(
            read_file(&context.dir.join("status")).trim(),
            "ERROR:SET_NETWORK"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn apply_wifi_timeout_rolls_back_and_reenables() {
        let dir = temp_dir("wifi-timeout-script");
        let path = fake_wpa(
            &dir,
            r#"case "$*" in
  *" status"*) echo wpa_state=DISCONNECTED ;;
  *" list_networks"*) printf '0\tHome\tany\t[CURRENT]\n1\tFallback\tany\n' ;;
  *" add_network"*) echo 7 ;;
  *) echo OK ;;
esac"#,
        );
        let context = new_gateway_live("wifi-timeout", &path);
        let gateway: Arc<Gateway> = context.gateway.clone();
        gateway.apply_wifi("Home", "password123").await;
        assert_eq!(
            read_file(&context.dir.join("status")).trim(),
            "ERROR:TIMEOUT"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn dry_run_never_touches_wpa() {
        let context = new_gateway("wifi-dry-run", Path::new("/nonexistent/wpa_cli"));
        let gateway = context.gateway.clone();
        gateway.apply_wifi("Home", "password123").await;
        assert_eq!(read_file(&context.dir.join("status")).trim(), "DRY_RUN_OK");
    }
}
