//! Bluetooth A2DP sink mode for the stock "mute x4" gesture.

use std::time::Duration;

use tokio::process::Command;

use crate::gateway::Gateway;
use crate::logf;

/// A discoverable classic adapter disturbs Wi-Fi on this combo chip, so the
/// pairing window is limited to 90 seconds.
const BLUETOOTH_WINDOW: Duration = Duration::from_secs(90);

async fn bluetoothctl(args: &[&str]) -> (String, bool) {
    match Command::new("/usr/bin/bluetoothctl")
        .args(args)
        .output()
        .await
    {
        Ok(output) => {
            let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&output.stderr));
            (text.trim().to_string(), output.status.success())
        }
        Err(err) => (err.to_string(), false),
    }
}

async fn process_running(name: &str) -> bool {
    match Command::new("/bin/pidof").arg(name).output().await {
        Ok(output) => {
            output.status.success() && !String::from_utf8_lossy(&output.stdout).trim().is_empty()
        }
        Err(_) => false,
    }
}

async fn ensure_bluetooth_audio(gateway: &Gateway) {
    if !process_running("bluealsad").await {
        run_logged(
            gateway,
            "/etc/init.d/S52bluealsa",
            &["start"],
            "bluealsad start",
        )
        .await;
    }
    if !process_running("bluealsa-aplay").await {
        run_logged(
            gateway,
            "/etc/init.d/S53bluealsa-aplay",
            &["start"],
            "bluealsa-aplay start",
        )
        .await;
    }
    if !process_running("bt-agent").await {
        // NoInputNoOutput is enough for a phone acting as an A2DP source.
        run_logged(
            gateway,
            "/usr/bin/bt-agent",
            &["-c", "NoInputNoOutput", "-d"],
            "bt-agent start",
        )
        .await;
    }
}

async fn run_logged(gateway: &Gateway, program: &str, args: &[&str], what: &str) {
    match Command::new(program).args(args).output().await {
        Ok(output) if output.status.success() => {}
        Ok(output) => {
            let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&output.stderr));
            logf!(gateway.logger, "bluetooth: {what}: {}", text.trim());
        }
        Err(err) => {
            logf!(gateway.logger, "bluetooth: {what}: {err}");
        }
    }
}

impl Gateway {
    async fn wifi_reconnect(&self) {
        let (out, ok) = self.wpa.reconnect().await;
        if !ok {
            logf!(self.logger, "bluetooth: wpa reconnect: {out}");
            return;
        }
        logf!(
            self.logger,
            "bluetooth: asked wpa_supplicant to reconnect Wi-Fi"
        );
    }

    async fn disconnect_bluetooth_devices(&self) {
        let (out, _) = bluetoothctl(&["devices", "Connected"]).await;
        for line in out.lines() {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() >= 3 && fields[0].eq_ignore_ascii_case("Device") {
                let (_, ok) = bluetoothctl(&["disconnect", fields[1]]).await;
                if !ok {
                    logf!(self.logger, "bluetooth: disconnect {}: failed", fields[1]);
                }
            }
        }
    }

    async fn close_bluetooth_window(&self, generation: u64) {
        let on = {
            let state = self.lock_state();
            if state.bt_generation != generation {
                return;
            }
            state.bt_mode
        };
        if !on {
            return;
        }
        let (_, ok) = bluetoothctl(&["discoverable", "off"]).await;
        if !ok {
            logf!(self.logger, "bluetooth: discoverable off: failed");
        }
        let (_, ok) = bluetoothctl(&["pairable", "off"]).await;
        if !ok {
            logf!(self.logger, "bluetooth: pairable off: failed");
        }
        logf!(
            self.logger,
            "bluetooth: pairing window closed; paired devices keep their A2DP link"
        );
        self.wifi_reconnect().await;
    }

    pub async fn disable_bluetooth(&self) {
        {
            let mut state = self.lock_state();
            state.bt_mode = false;
            // Invalidate the pending window task.
            state.bt_generation = state.bt_generation.wrapping_add(1);
        }
        let (_, ok) = bluetoothctl(&["discoverable", "off"]).await;
        if !ok {
            logf!(self.logger, "bluetooth: discoverable off: failed");
        }
        let (_, ok) = bluetoothctl(&["pairable", "off"]).await;
        if !ok {
            logf!(self.logger, "bluetooth: pairable off: failed");
        }
        self.disconnect_bluetooth_devices().await;
        logf!(self.logger, "bluetooth: pairing off");
        self.wifi_reconnect().await;
    }

    pub async fn enable_bluetooth(self: &std::sync::Arc<Self>) {
        ensure_bluetooth_audio(self).await;
        let (_, ok) = bluetoothctl(&["power", "on"]).await;
        if !ok {
            logf!(self.logger, "bluetooth: power on: failed");
        }
        let (_, ok) = bluetoothctl(&["pairable", "on"]).await;
        if !ok {
            logf!(self.logger, "bluetooth: pairable on: failed");
        }
        let (_, ok) = bluetoothctl(&["discoverable", "on"]).await;
        if !ok {
            logf!(self.logger, "bluetooth: discoverable on: failed");
            return;
        }
        let generation = {
            let mut state = self.lock_state();
            state.bt_mode = true;
            state.bt_generation = state.bt_generation.wrapping_add(1);
            state.bt_generation
        };
        let gateway = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(BLUETOOTH_WINDOW).await;
            gateway.close_bluetooth_window(generation).await;
        });
        logf!(
            self.logger,
            "bluetooth: pairing on for {BLUETOOTH_WINDOW:?} (A2DP sink; bluealsad/bluealsa-aplay/bt-agent ready)"
        );
    }

    pub async fn toggle_bluetooth(self: &std::sync::Arc<Self>) {
        let on = self.lock_state().bt_mode;
        if on {
            self.disable_bluetooth().await;
            return;
        }
        self.enable_bluetooth().await;
    }
}
