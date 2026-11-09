//! Classic Bluetooth A2DP-sink mode ("mute x4").
//!
//! BlueZ is reached over D-Bus: adapter properties, device disconnects and the
//! pairing window use proxies. `bluealsad`, `bluealsa-aplay` and `bt-agent`
//! stay external processes: they have no D-Bus equivalent that would start the
//! ALSA bridge or the NoInputNoOutput agent for us.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use thiserror::Error;
use tokio::process::Command;
use tracing::warn;
use zbus::Connection;
use zbus::fdo::ObjectManagerProxy;

use crate::wifi::WpaClient;

pub type AdapterFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, AdapterError>> + Send + 'a>>;

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("BlueZ: {0}")]
    Bluez(#[from] zbus::Error),
    #[error("BlueZ: {0}")]
    Fdo(#[from] zbus::fdo::Error),
    #[error("{0}")]
    Message(String),
}

pub trait AdapterOps: Send + Sync {
    fn prepare_audio(&self) -> AdapterFuture<'_, ()>;
    fn set_powered(&self, on: bool) -> AdapterFuture<'_, ()>;
    fn set_pairable(&self, on: bool) -> AdapterFuture<'_, ()>;
    fn set_discoverable(&self, on: bool) -> AdapterFuture<'_, ()>;
    fn disconnect_devices(&self) -> AdapterFuture<'_, ()>;
}

#[rustfmt::skip]
#[zbus::proxy(interface = "org.bluez.Adapter1", default_service = "org.bluez", default_path = "/org/bluez/hci0")]
trait Adapter1 {
    #[zbus(property)]
    fn set_powered(&self, value: bool) -> zbus::Result<()>;
    #[zbus(property)]
    fn set_pairable(&self, value: bool) -> zbus::Result<()>;
    #[zbus(property)]
    fn set_discoverable(&self, value: bool) -> zbus::Result<()>;
}

#[zbus::proxy(interface = "org.bluez.Device1", default_service = "org.bluez")]
trait Device1 {
    fn disconnect(&self) -> zbus::Result<()>;
}

pub struct BlueZAdapter {
    connection: OnceLock<Connection>,
}

impl Default for BlueZAdapter {
    fn default() -> Self {
        Self {
            connection: OnceLock::new(),
        }
    }
}

impl BlueZAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_connection(&self, connection: Connection) {
        if self.connection.set(connection).is_err() {
            warn!("D-Bus connection already set");
        }
    }

    fn connection(&self) -> Result<&Connection, AdapterError> {
        self.connection
            .get()
            .ok_or_else(|| AdapterError::Message("no D-Bus connection".to_string()))
    }
}

macro_rules! adapter_property {
    ($method:ident) => {
        fn $method(&self, on: bool) -> AdapterFuture<'_, ()> {
            Box::pin(async move {
                Adapter1Proxy::new(self.connection()?)
                    .await?
                    .$method(on)
                    .await?;
                Ok(())
            })
        }
    };
}

impl AdapterOps for BlueZAdapter {
    #[rustfmt::skip]
    fn prepare_audio(&self) -> AdapterFuture<'_, ()> {
        Box::pin(async move {
            ensure_process("bluealsad", "/etc/init.d/S52bluealsa", &["start"]).await;
            ensure_process("bluealsa-aplay", "/etc/init.d/S53bluealsa-aplay", &["start"]).await;
            ensure_process("bt-agent", "/usr/bin/bt-agent", &["-c", "NoInputNoOutput", "-d"]).await;
            Ok(())
        })
    }

    adapter_property!(set_powered);
    adapter_property!(set_pairable);
    adapter_property!(set_discoverable);

    fn disconnect_devices(&self) -> AdapterFuture<'_, ()> {
        Box::pin(async move {
            let connection = self.connection()?;
            let manager = ObjectManagerProxy::builder(connection)
                .destination("org.bluez")
                .path("/")
                .build()
                .await?;
            for (path, interfaces) in manager.get_managed_objects().await? {
                let connected = interfaces
                    .get("org.bluez.Device1")
                    .and_then(|properties| properties.get("Connected"))
                    .and_then(|value| bool::try_from(value.clone()).ok())
                    .unwrap_or(false);
                if !connected {
                    continue;
                }
                let device = Device1Proxy::builder(connection)
                    .destination("org.bluez")
                    .path(path)
                    .build()
                    .await?;
                if let Err(error) = device.disconnect().await {
                    warn!(%error, "Bluetooth disconnect");
                }
            }
            Ok(())
        })
    }
}

async fn ensure_process(name: &str, program: &str, arguments: &[&str]) {
    let running = Command::new("/bin/pidof")
        .arg(name)
        .output()
        .await
        .is_ok_and(|output| {
            output.status.success() && !String::from_utf8_lossy(&output.stdout).trim().is_empty()
        });
    if !running {
        match Command::new(program).args(arguments).output().await {
            Ok(output) if output.status.success() => {}
            Ok(output) => warn!(name, "Bluetooth helper failed: {}", output.status),
            Err(error) => warn!(name, %error, "Bluetooth helper failed"),
        }
    }
}

pub struct ClassicBluetooth {
    adapter: Arc<dyn AdapterOps>,
    wpa: WpaClient,
    window: Duration,
    mode: AtomicBool,
    generation: AtomicU64,
}

impl ClassicBluetooth {
    pub fn new(adapter: Arc<dyn AdapterOps>, wpa: WpaClient, window: Duration) -> Self {
        Self {
            adapter,
            wpa,
            window,
            mode: AtomicBool::new(false),
            generation: AtomicU64::new(0),
        }
    }

    pub fn is_on(&self) -> bool {
        self.mode.load(Ordering::SeqCst)
    }

    /// Turn the pairer on for `window`; the task resets `mode` when it closes.
    pub async fn enable(self: &Arc<Self>) {
        for result in [
            self.adapter.prepare_audio().await,
            self.adapter.set_powered(true).await,
            self.adapter.set_pairable(true).await,
        ] {
            if let Err(error) = result {
                warn!(%error, "Bluetooth enable");
            }
        }
        if let Err(error) = self.adapter.set_discoverable(true).await {
            warn!(%error, "Bluetooth discoverable on");
            return;
        }
        self.mode.store(true, Ordering::SeqCst);
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let this = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(this.window).await;
            this.close_window(generation).await;
        });
    }

    /// Close the window opened by `generation`, if it is still current. Resets
    /// `btMode`; the old implementation left it set and inverted the gesture.
    pub async fn close_window(&self, generation: u64) {
        if self.generation.load(Ordering::SeqCst) != generation
            || !self.mode.swap(false, Ordering::SeqCst)
        {
            return;
        }
        self.set_pairable(false).await;
        self.set_discoverable(false).await;
        self.reconnect_wifi().await;
    }

    pub async fn disable(&self) {
        self.mode.store(false, Ordering::SeqCst);
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.set_pairable(false).await;
        self.set_discoverable(false).await;
        if let Err(error) = self.adapter.disconnect_devices().await {
            warn!(%error, "Bluetooth disconnect devices");
        }
        self.reconnect_wifi().await;
    }

    pub async fn toggle(self: &Arc<Self>) {
        if self.is_on() {
            self.disable().await;
        } else {
            self.enable().await;
        }
    }

    async fn set_pairable(&self, on: bool) {
        if let Err(error) = self.adapter.set_pairable(on).await {
            warn!(%error, on, "Bluetooth pairable");
        }
    }

    async fn set_discoverable(&self, on: bool) {
        if let Err(error) = self.adapter.set_discoverable(on).await {
            warn!(%error, on, "Bluetooth discoverable");
        }
    }

    async fn reconnect_wifi(&self) {
        if let Err(error) = self.wpa.reconnect().await {
            warn!(%error, "wpa_supplicant reconnect");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{RecordingAdapter, temp_dir};

    fn adapter() -> Arc<RecordingAdapter> {
        Arc::new(RecordingAdapter::default())
    }

    fn bluetooth(adapter: Arc<RecordingAdapter>) -> Arc<ClassicBluetooth> {
        let _dir = temp_dir("bluetooth");
        Arc::new(ClassicBluetooth::new(
            adapter,
            WpaClient::new("/nonexistent/wpa.sock"),
            Duration::from_secs(90),
        ))
    }

    #[tokio::test(start_paused = true)]
    async fn window_expiry_turns_the_mode_off() {
        let adapter = adapter();
        let bluetooth = bluetooth(adapter.clone());
        bluetooth.enable().await;
        tokio::task::yield_now().await;
        assert!(bluetooth.is_on());
        assert!(adapter.calls().contains(&"discoverable:true".to_string()));
        tokio::time::advance(Duration::from_secs(90)).await;
        tokio::task::yield_now().await;
        assert!(!bluetooth.is_on(), "btMode must reset after the window");
        assert!(adapter.calls().contains(&"discoverable:false".to_string()));
    }

    #[tokio::test(start_paused = true)]
    async fn disabling_invalidates_a_pending_window() {
        let adapter = adapter();
        let bluetooth = bluetooth(adapter.clone());
        bluetooth.enable().await;
        tokio::task::yield_now().await;
        bluetooth.disable().await;
        tokio::time::advance(Duration::from_secs(90)).await;
        tokio::task::yield_now().await;
        assert!(!bluetooth.is_on());
        assert_eq!(
            adapter
                .calls()
                .iter()
                .filter(|call| call.as_str() == "discoverable:false")
                .count(),
            1
        );
    }

    #[tokio::test(start_paused = true)]
    async fn toggle_follows_the_mode() {
        let adapter = adapter();
        let bluetooth = bluetooth(adapter);
        bluetooth.toggle().await;
        assert!(bluetooth.is_on());
        bluetooth.toggle().await;
        assert!(!bluetooth.is_on());
    }
}
