//! BlueZ GATT application, advertisement and characteristic interfaces.
//!
//! Objects are exported through `#[zbus::interface]`; BlueZ managers are
//! reached through `#[zbus::proxy]`. `zbus` stays pure Rust and does not link
//! libdbus, unlike `bluer`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Context, Result, anyhow};
use tokio::sync::mpsc;
use tracing::{debug, warn};
use zbus::{
    Connection, ObjectPath, OwnedObjectPath, OwnedVariant, Variant, object_server::SignalEmitter,
};

use crate::credentials::{CredentialField, Credentials};

pub const SERVICE_UUID: &str = "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d";
pub const CHAR_SSID: &str = "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c01";
pub const CHAR_PSK: &str = "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c02";
pub const CHAR_TOKEN: &str = "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c03";
pub const CHAR_COMMIT: &str = "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c04";
pub const CHAR_STATUS: &str = "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c05";
pub const APP_PATH: &str = "/com/station/gateway";
pub const ADV_PATH: &str = "/com/station/gateway/advertisement";
pub const SVC_PATH: &str = "/com/station/gateway/service0";
const CHAR_BASE: &str = "/com/station/gateway/service0/char";

#[rustfmt::skip] const CHARS: [(usize, &str, &[&str], Source); 5] = [
    (0, CHAR_SSID, &["read", "write"], Source::Credential(CredentialField::Ssid)),
    (1, CHAR_PSK, &["write"], Source::Credential(CredentialField::Psk)),
    (2, CHAR_TOKEN, &["write"], Source::Credential(CredentialField::Token)),
    (3, CHAR_COMMIT, &["write"], Source::Commit),
    (4, CHAR_STATUS, &["read", "notify"], Source::Status),
];

#[derive(Debug, zbus::DBusError)] #[zbus(prefix = "org.bluez.Error")] #[rustfmt::skip] pub enum BluezError { InvalidOffset, NotPermitted }

pub fn characteristic_path(index: usize) -> OwnedObjectPath {
    OwnedObjectPath::try_from(format!("{CHAR_BASE}{index}")).expect("valid characteristic path")
}

#[rustfmt::skip]
#[zbus::proxy(interface = "org.bluez.GattManager1", default_service = "org.bluez", default_path = "/org/bluez/hci0")]
trait GattManager1 {
    fn register_application(
        &self,
        application: ObjectPath<'_>,
        options: HashMap<&str, Variant<'_>>,
    ) -> zbus::Result<()>;
    fn unregister_application(&self, application: ObjectPath<'_>) -> zbus::Result<()>;
}

#[rustfmt::skip]
#[zbus::proxy(interface = "org.bluez.LEAdvertisingManager1", default_service = "org.bluez", default_path = "/org/bluez/hci0")]
trait LeAdvertisingManager1 {
    fn register_advertisement(
        &self,
        advertisement: ObjectPath<'_>,
        options: HashMap<&str, Variant<'_>>,
    ) -> zbus::Result<()>;
    fn unregister_advertisement(&self, advertisement: ObjectPath<'_>) -> zbus::Result<()>;
}

#[derive(Clone, Copy)] #[rustfmt::skip]
enum Source { Credential(CredentialField), Commit, Status }

#[rustfmt::skip]
struct Inner { index: usize, uuid: String, flags: Vec<String>, source: Source, credentials: Arc<Credentials>, commit: mpsc::Sender<Vec<u8>>, connection: Option<Connection>, value: Mutex<Vec<u8>>, notifying: AtomicBool }

#[derive(Clone)] #[rustfmt::skip]
struct Characteristic { inner: Arc<Inner> }

impl Characteristic {
    fn new(
        connection: Option<Connection>,
        (index, uuid, flags, source): (usize, &str, &[&str], Source),
        credentials: Arc<Credentials>,
        commit: mpsc::Sender<Vec<u8>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            inner: Arc::new(Inner {
                index,
                uuid: uuid.to_string(),
                flags: flags.iter().map(|flag| (*flag).to_string()).collect(),
                source,
                credentials,
                commit,
                connection,
                value: Mutex::new(Vec::new()),
                notifying: AtomicBool::new(false),
            }),
        })
    }

    async fn emit(&self, value: &[u8]) {
        let Some(connection) = self.inner.connection.as_ref() else {
            return;
        };
        let Ok(emitter) = SignalEmitter::new(connection, characteristic_path(self.inner.index))
        else {
            return;
        };
        let changed = HashMap::from([("Value", Variant::from(value.to_vec()))]);
        if let Err(error) = emitter
            .emit(
                "org.freedesktop.DBus.Properties",
                "PropertiesChanged",
                &("org.bluez.GattCharacteristic1", changed, Vec::<&str>::new()),
            )
            .await
        {
            warn!(%error, "PropertiesChanged");
        }
    }

    fn offset(options: &HashMap<String, OwnedVariant>) -> i64 {
        let Some(value) = options.get("offset") else {
            return 0;
        };
        // BlueZ sends the GATT write offset as uint16 (`q`).
        u16::try_from(value.clone())
            .ok()
            .map(i64::from)
            .or_else(|| i32::try_from(value.clone()).ok().map(i64::from))
            .unwrap_or(0)
    }
}

#[zbus::interface(name = "org.bluez.GattCharacteristic1")]
impl Characteristic {
    #[zbus(property, name = "UUID")]
    fn prop_uuid(&self) -> String {
        self.inner.uuid.clone()
    }

    #[zbus(property, name = "Service")]
    fn prop_service(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from(SVC_PATH).expect("valid service path")
    }

    #[zbus(property, name = "Flags")]
    fn prop_flags(&self) -> Vec<String> {
        self.inner.flags.clone()
    }

    #[zbus(property, name = "Value")]
    fn value(&self) -> Vec<u8> {
        match self.inner.source {
            Source::Credential(field) => self.inner.credentials.get(field),
            Source::Commit | Source::Status => self
                .inner
                .value
                .lock()
                .map_or_else(|_| Vec::new(), |value| value.clone()),
        }
    }

    #[zbus(property, name = "Notifying")]
    fn prop_notifying(&self) -> bool {
        self.inner.notifying.load(Ordering::SeqCst)
    }

    #[allow(clippy::needless_pass_by_value)]
    #[zbus(name = "ReadValue")]
    fn read_value(&self, options: HashMap<String, OwnedVariant>) -> Result<Vec<u8>, BluezError> {
        let value = self.value();
        let offset = Self::offset(&options);
        let offset = usize::try_from(offset).map_err(|_| BluezError::InvalidOffset)?;
        value
            .get(offset..)
            .map(<[u8]>::to_vec)
            .ok_or(BluezError::InvalidOffset)
    }

    #[zbus(name = "WriteValue")]
    async fn write_value(
        &self,
        value: Vec<u8>,
        options: HashMap<String, OwnedVariant>,
    ) -> Result<(), BluezError> {
        match self.inner.source {
            Source::Credential(field) => {
                let offset = usize::try_from(Self::offset(&options))
                    .map_err(|_| BluezError::InvalidOffset)?;
                if self.inner.credentials.write(field, &value, offset) {
                    Ok(())
                } else {
                    Err(BluezError::InvalidOffset)
                }
            }
            Source::Commit => self
                .inner
                .commit
                .send(value)
                .await
                .map_err(|_| BluezError::NotPermitted),
            Source::Status => Err(BluezError::NotPermitted),
        }
    }

    #[zbus(name = "StartNotify")]
    async fn start_notify(&self) -> Result<(), BluezError> {
        self.inner.notifying.store(true, Ordering::SeqCst);
        let value = self.value();
        self.emit(&value).await;
        Ok(())
    }

    #[zbus(name = "StopNotify")]
    fn stop_notify(&self) -> Result<(), BluezError> {
        self.inner.notifying.store(false, Ordering::SeqCst);
        Ok(())
    }
}

struct Service {
    characteristics: Vec<OwnedObjectPath>,
}

#[zbus::interface(name = "org.bluez.GattService1")]
impl Service {
    #[zbus(property, name = "UUID")]
    fn prop_uuid(&self) -> String {
        SERVICE_UUID.to_string()
    }

    #[zbus(property, name = "Primary")]
    fn prop_primary(&self) -> bool {
        true
    }

    #[zbus(property, name = "Characteristics")]
    fn prop_characteristics(&self) -> Vec<OwnedObjectPath> {
        self.characteristics.clone()
    }
}

struct Advertisement;

#[zbus::interface(name = "org.bluez.LEAdvertisement1")]
impl Advertisement {
    #[zbus(property, name = "Type")]
    fn prop_type(&self) -> String {
        "peripheral".to_string()
    }

    #[zbus(property, name = "ServiceUUIDs")]
    fn prop_service_uuids(&self) -> Vec<String> {
        vec![SERVICE_UUID.to_string()]
    }

    #[zbus(property, name = "LocalName")]
    fn prop_local_name(&self) -> String {
        "Station-Gateway".to_string()
    }

    #[zbus(name = "Release")]
    fn release(&self) {}
}

/// BlueZ GATT application controller.
pub struct Ble {
    commit: mpsc::Sender<Vec<u8>>,
    credentials: Arc<Credentials>,
    connection: OnceLock<Connection>,
    advertising: AtomicBool,
    status: Mutex<Option<Arc<Characteristic>>>,
}

impl Ble {
    pub fn new(credentials: Arc<Credentials>, commit: mpsc::Sender<Vec<u8>>) -> Self {
        Self {
            commit,
            credentials,
            connection: OnceLock::new(),
            advertising: AtomicBool::new(false),
            status: Mutex::new(None),
        }
    }

    pub fn set_connection(&self, connection: Connection) {
        if self.connection.set(connection).is_err() {
            warn!("D-Bus connection already set");
        }
    }

    pub(crate) fn advertising(&self) -> bool {
        self.advertising.load(Ordering::SeqCst)
    }

    fn connection(&self) -> Result<Connection> {
        self.connection
            .get()
            .cloned()
            .ok_or_else(|| anyhow!("no D-Bus connection"))
    }

    pub(crate) async fn enable(&self) -> Result<()> {
        let connection = self.connection()?;
        if self.advertising() {
            return Ok(());
        }
        if self.status.lock().is_ok_and(|status| status.is_none()) {
            remove_gatt_objects(&connection).await;
            self.export_gatt(&connection).await?;
        }
        if let Ok(properties) =
            zbus::fdo::PropertiesProxy::new(&connection, "org.bluez", "/org/bluez/hci0").await
            && let Ok(interface) = zbus::names::InterfaceName::try_from("org.bluez.Adapter1")
            && let Err(error) = properties
                .set(interface, "Connectable", &Variant::Bool(true))
                .await
        {
            warn!(%error, "set Connectable");
        }
        self.register_advertisement(&connection).await?;
        self.advertising.store(true, Ordering::SeqCst);
        Ok(())
    }

    pub(crate) async fn disable(&self) {
        if !self.advertising.swap(false, Ordering::SeqCst) {
            return;
        }
        let connection = match self.connection() {
            Ok(connection) => connection,
            Err(error) => {
                warn!(%error, "BLE disable without D-Bus connection");
                return;
            }
        };
        self.unregister_advertisement(&connection).await;
        if self.status.lock().is_ok_and(|status| status.is_some()) {
            self.unregister_gatt(&connection).await;
        }
    }

    pub(crate) async fn notify_status(&self, value: &[u8]) {
        let status = match self.status.lock() {
            Ok(status) => status.clone(),
            Err(error) => {
                warn!(%error, "GATT status lock poisoned");
                None
            }
        };
        if let Some(characteristic) = status {
            if let Ok(mut current) = characteristic.inner.value.lock() {
                current.clear();
                current.extend_from_slice(value);
            }
            if characteristic.inner.notifying.load(Ordering::SeqCst) {
                characteristic.emit(value).await;
            }
        }
    }

    async fn export_gatt(&self, connection: &Connection) -> Result<()> {
        let chars: Vec<_> = CHARS
            .into_iter()
            .map(|spec| {
                Characteristic::new(
                    Some(connection.clone()),
                    spec,
                    self.credentials.clone(),
                    self.commit.clone(),
                )
            })
            .collect();
        let server = connection.object_server();
        for characteristic in &chars {
            server
                .at(
                    characteristic_path(characteristic.inner.index),
                    characteristic.as_ref().clone(),
                )
                .await
                .context("export characteristic")?;
        }
        server
            .at(
                object_path(SVC_PATH),
                Service {
                    characteristics: chars
                        .iter()
                        .map(|characteristic| characteristic_path(characteristic.inner.index))
                        .collect(),
                },
            )
            .await
            .context("export service")?;
        server
            .at(object_path(APP_PATH), zbus::fdo::ObjectManager)
            .await
            .context("export object manager")?;
        *self
            .status
            .lock()
            .map_err(|_| anyhow!("GATT state poisoned"))? = chars.last().cloned();
        GattManager1Proxy::new(connection)
            .await?
            .register_application(
                ObjectPath::try_from(APP_PATH).expect("valid path"),
                HashMap::new(),
            )
            .await?;
        Ok(())
    }

    async fn unregister_gatt(&self, connection: &Connection) {
        match GattManager1Proxy::new(connection).await {
            Ok(manager) => {
                if let Err(error) = manager
                    .unregister_application(ObjectPath::try_from(APP_PATH).expect("valid path"))
                    .await
                {
                    warn!(%error, "UnregisterApplication");
                }
            }
            Err(error) => warn!(%error, "GattManager1 proxy"),
        }
        remove_gatt_objects(connection).await;
        if let Ok(mut status) = self.status.lock() {
            *status = None;
        }
    }

    async fn register_advertisement(&self, connection: &Connection) -> Result<()> {
        if let Err(error) = connection
            .object_server()
            .remove::<Advertisement, _>(object_path(ADV_PATH))
            .await
        {
            debug!(%error, "remove advertisement object");
        }
        connection
            .object_server()
            .at(object_path(ADV_PATH), Advertisement)
            .await
            .context("export advertisement")?;
        LeAdvertisingManager1Proxy::new(connection)
            .await?
            .register_advertisement(
                ObjectPath::try_from(ADV_PATH).expect("valid path"),
                HashMap::new(),
            )
            .await?;
        Ok(())
    }

    async fn unregister_advertisement(&self, connection: &Connection) {
        match LeAdvertisingManager1Proxy::new(connection).await {
            Ok(manager) => {
                if let Err(error) = manager
                    .unregister_advertisement(ObjectPath::try_from(ADV_PATH).expect("valid path"))
                    .await
                {
                    warn!(%error, "UnregisterAdvertisement");
                }
            }
            Err(error) => warn!(%error, "LEAdvertisingManager1 proxy"),
        }
        if let Err(error) = connection
            .object_server()
            .remove::<Advertisement, _>(object_path(ADV_PATH))
            .await
        {
            debug!(%error, "remove advertisement object");
        }
    }
}

fn object_path(path: &str) -> ObjectPath<'_> {
    ObjectPath::try_from(path).expect("valid object path")
}

async fn remove_gatt_objects(connection: &Connection) {
    let server = connection.object_server();
    for index in 0..CHARS.len() {
        if let Err(error) = server
            .remove::<Characteristic, _>(characteristic_path(index))
            .await
        {
            debug!(%error, "remove characteristic object");
        }
    }
    if let Err(error) = server.remove::<Service, _>(object_path(SVC_PATH)).await {
        debug!(%error, "remove service object");
    }
    if let Err(error) = server
        .remove::<zbus::fdo::ObjectManager, _>(object_path(APP_PATH))
        .await
    {
        debug!(%error, "remove object manager");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn characteristic(
        spec: (usize, &str, &[&str], Source),
    ) -> (
        Arc<Characteristic>,
        Arc<Credentials>,
        mpsc::Receiver<Vec<u8>>,
    ) {
        let credentials = Arc::new(Credentials::default());
        let (sender, receiver) = mpsc::channel(4);
        (
            Characteristic::new(None, spec, credentials.clone(), sender),
            credentials,
            receiver,
        )
    }

    fn ssid() -> (usize, &'static str, &'static [&'static str], Source) {
        CHARS[0]
    }

    fn offset(value: i32) -> HashMap<String, OwnedVariant> {
        HashMap::from([(
            "offset".to_string(),
            OwnedVariant::try_from(Variant::I32(value)).expect("owned value"),
        )])
    }

    #[tokio::test]
    async fn characteristic_read_write_offsets() {
        let (characteristic, _, _) = characteristic(ssid());
        characteristic
            .write_value(b"Home".to_vec(), HashMap::new())
            .await
            .expect("write initial");
        characteristic
            .write_value(b"WiFi".to_vec(), offset(4))
            .await
            .expect("write at offset");
        assert_eq!(characteristic.value(), b"HomeWiFi");
        assert_eq!(
            characteristic.read_value(offset(4)).expect("read offset"),
            b"WiFi"
        );
        assert!(characteristic.read_value(offset(99)).is_err());
        assert!(
            characteristic
                .write_value(b"x".to_vec(), offset(600))
                .await
                .is_err()
        );
        assert!(
            characteristic
                .write_value(b"x".to_vec(), offset(20))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn long_write_uses_uint16_offsets() {
        let (characteristic, _, _) = characteristic(CHARS[1]);
        characteristic
            .write_value(b"password".to_vec(), HashMap::new())
            .await
            .expect("first chunk");
        let offset8 = HashMap::from([(
            "offset".to_string(),
            OwnedVariant::try_from(Variant::U16(8)).expect("owned u16"),
        )]);
        characteristic
            .write_value(b"123".to_vec(), offset8.clone())
            .await
            .expect("second chunk");
        assert_eq!(characteristic.value(), b"password123");
        assert_eq!(characteristic.read_value(offset8).expect("read"), b"123");
        let past_end = HashMap::from([(
            "offset".to_string(),
            OwnedVariant::try_from(Variant::U16(20)).expect("owned u16"),
        )]);
        assert!(characteristic.read_value(past_end.clone()).is_err());
        assert!(
            characteristic
                .write_value(b"x".to_vec(), past_end)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn commit_writes_are_delivered_to_the_worker() {
        let (characteristic, _, mut receiver) = characteristic(CHARS[3]);
        characteristic
            .write_value(b"1".to_vec(), HashMap::new())
            .await
            .expect("commit");
        assert_eq!(receiver.recv().await, Some(b"1".to_vec()));
    }

    #[tokio::test]
    async fn commit_channel_closed_is_an_error() {
        let (characteristic, _, receiver) = characteristic(CHARS[3]);
        drop(receiver);
        assert!(
            characteristic
                .write_value(b"1".to_vec(), HashMap::new())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn status_notify_and_properties() {
        let (characteristic, _, _) = characteristic(CHARS[4]);
        characteristic.start_notify().await.expect("start notify");
        assert!(characteristic.prop_notifying());
        characteristic.stop_notify().expect("stop notify");
        assert!(!characteristic.prop_notifying());
        assert!(
            characteristic
                .write_value(b"x".to_vec(), HashMap::new())
                .await
                .is_err()
        );
        *characteristic.inner.value.lock().expect("value") = b"READY".to_vec();
        assert_eq!(characteristic.value(), b"READY");
        assert_eq!(characteristic.prop_flags(), vec!["read", "notify"]);
    }

    #[test]
    fn properties_match_the_bluez_contract() {
        let (characteristic, _, _) = characteristic(ssid());
        assert_eq!(characteristic.prop_uuid(), CHAR_SSID);
        assert_eq!(
            characteristic.prop_service(),
            OwnedObjectPath::try_from(SVC_PATH).expect("path")
        );
        let service = Service {
            characteristics: vec![characteristic_path(0)],
        };
        assert_eq!(service.prop_uuid(), SERVICE_UUID);
        assert!(service.prop_primary());
        assert_eq!(service.prop_characteristics().len(), 1);
        let advertisement = Advertisement;
        assert_eq!(advertisement.prop_local_name(), "Station-Gateway");
        assert_eq!(advertisement.prop_type(), "peripheral");
        assert_eq!(
            advertisement.prop_service_uuids(),
            vec![SERVICE_UUID.to_string()]
        );
        advertisement.release();
    }

    #[tokio::test]
    async fn ble_without_a_bus_never_claims_to_advertise() {
        let (sender, _receiver) = mpsc::channel(1);
        let ble = Ble::new(Arc::new(Credentials::default()), sender);
        assert!(!ble.advertising());
        assert!(ble.enable().await.is_err());
        ble.disable().await;
        ble.notify_status(b"IDLE").await;
        assert!(!ble.advertising());
    }

    #[test]
    fn characteristic_paths_are_stable() {
        assert_eq!(
            characteristic_path(4),
            OwnedObjectPath::try_from("/com/station/gateway/service0/char4").expect("path")
        );
    }
}
