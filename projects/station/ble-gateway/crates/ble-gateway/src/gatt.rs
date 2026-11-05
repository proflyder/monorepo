//! BlueZ GATT application, advertisement and characteristic interfaces.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use zbus::Connection;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};

use crate::gateway::Gateway;
use crate::logf;

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
const ADAPTER_PATH: &str = "/org/bluez/hci0";

/// D-Bus errors BlueZ expects from characteristic implementations.
#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.bluez.Error")]
pub enum BluezError {
    InvalidOffset,
    NotPermitted,
}

pub fn characteristic_name(uuid: &str) -> &'static str {
    match uuid {
        CHAR_SSID => "ssid",
        CHAR_PSK => "psk",
        CHAR_TOKEN => "token",
        CHAR_COMMIT => "commit",
        CHAR_STATUS => "status",
        _ => "unknown",
    }
}

pub fn characteristic_path(index: usize) -> OwnedObjectPath {
    OwnedObjectPath::try_from(format!("{CHAR_BASE}{index}")).expect("valid characteristic path")
}

struct CharacteristicInner {
    value: Mutex<Vec<u8>>,
    notifying: AtomicBool,
}

#[derive(Clone)]
pub struct Characteristic {
    gateway: Arc<Gateway>,
    index: usize,
    uuid: String,
    path: OwnedObjectPath,
    flags: Vec<String>,
    inner: Arc<CharacteristicInner>,
}

impl Characteristic {
    pub fn new(gateway: Arc<Gateway>, index: usize, uuid: &str, flags: &[&str]) -> Arc<Self> {
        Arc::new(Self {
            gateway,
            index,
            uuid: uuid.to_string(),
            path: characteristic_path(index),
            flags: flags.iter().map(|flag| (*flag).to_string()).collect(),
            inner: Arc::new(CharacteristicInner {
                value: Mutex::new(Vec::new()),
                notifying: AtomicBool::new(false),
            }),
        })
    }

    pub fn path(&self) -> &OwnedObjectPath {
        &self.path
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn uuid(&self) -> &str {
        &self.uuid
    }

    pub fn name(&self) -> &'static str {
        characteristic_name(&self.uuid)
    }

    pub fn value(&self) -> Vec<u8> {
        self.inner
            .value
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn set_value(&self, value: &[u8]) {
        *self.inner.value.lock().unwrap_or_else(|e| e.into_inner()) = value.to_vec();
    }

    pub fn notifying(&self) -> bool {
        self.inner.notifying.load(Ordering::SeqCst)
    }

    fn set_notifying(&self, value: bool) {
        self.inner.notifying.store(value, Ordering::SeqCst);
    }

    fn offset(options: &HashMap<String, OwnedValue>) -> i64 {
        let Some(value) = options.get("offset") else {
            return 0;
        };
        // BlueZ sends the GATT write offset as uint16 (`q`). zvariant's numeric
        // conversions are strict, so try u16 before the wider types.
        if let Ok(value) = u16::try_from(value.clone()) {
            return i64::from(value);
        }
        if let Ok(value) = i32::try_from(value.clone()) {
            return i64::from(value);
        }
        if let Ok(value) = u32::try_from(value.clone()) {
            return i64::from(value);
        }
        i64::try_from(value.clone()).unwrap_or(0)
    }
}

#[zbus::interface(name = "org.bluez.GattCharacteristic1")]
impl Characteristic {
    #[zbus(property, name = "UUID")]
    fn prop_uuid(&self) -> String {
        self.uuid.clone()
    }

    #[zbus(property, name = "Service")]
    fn prop_service(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from(SVC_PATH).expect("valid service path")
    }

    #[zbus(property, name = "Flags")]
    fn prop_flags(&self) -> Vec<String> {
        self.flags.clone()
    }

    #[zbus(property, name = "Value")]
    fn prop_value(&self) -> Vec<u8> {
        self.value()
    }

    #[zbus(property, name = "Notifying")]
    fn prop_notifying(&self) -> bool {
        self.notifying()
    }

    #[zbus(property, name = "Descriptors")]
    fn prop_descriptors(&self) -> Vec<OwnedObjectPath> {
        Vec::new()
    }

    #[zbus(name = "ReadValue")]
    async fn read_value(
        &self,
        options: HashMap<String, OwnedValue>,
    ) -> Result<Vec<u8>, BluezError> {
        let offset = Self::offset(&options);
        let value = self.value();
        if offset < 0 || offset as usize > value.len() {
            return Err(BluezError::InvalidOffset);
        }
        Ok(value[offset as usize..].to_vec())
    }

    #[zbus(name = "WriteValue")]
    async fn write_value(
        &self,
        value: Vec<u8>,
        options: HashMap<String, OwnedValue>,
    ) -> Result<(), BluezError> {
        let offset = Self::offset(&options);
        if !(0..=512).contains(&offset) {
            return Err(BluezError::InvalidOffset);
        }

        match self.uuid.as_str() {
            CHAR_SSID | CHAR_PSK | CHAR_TOKEN => {
                {
                    let mut current = self.inner.value.lock().unwrap_or_else(|e| e.into_inner());
                    if offset == 0 {
                        *current = value.clone();
                    } else {
                        let offset = offset as usize;
                        if offset > current.len() {
                            return Err(BluezError::InvalidOffset);
                        }
                        current.truncate(offset);
                        current.extend_from_slice(&value);
                    }
                }
                logf!(
                    self.gateway.logger,
                    "write {}: {} bytes (offset {})",
                    self.name(),
                    value.len(),
                    offset
                );
                Ok(())
            }
            CHAR_COMMIT => {
                self.gateway.handle_commit(&value).await;
                Ok(())
            }
            _ => Err(BluezError::NotPermitted),
        }
    }

    #[zbus(name = "StartNotify")]
    async fn start_notify(&self) -> Result<(), BluezError> {
        self.set_notifying(true);
        let value = self.value();
        logf!(self.gateway.logger, "notify enabled for {}", self.name());
        self.gateway.emit_value(self, &value).await;
        Ok(())
    }

    #[zbus(name = "StopNotify")]
    async fn stop_notify(&self) -> Result<(), BluezError> {
        self.set_notifying(false);
        Ok(())
    }
}

#[derive(Clone)]
pub struct Service {
    characteristics: Vec<OwnedObjectPath>,
}

impl Service {
    fn new(characteristics: Vec<OwnedObjectPath>) -> Self {
        Self { characteristics }
    }
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

pub struct Advertisement {
    gateway: Arc<Gateway>,
}

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

    #[zbus(property, name = "Includes")]
    fn prop_includes(&self) -> Vec<String> {
        vec!["tx-power".to_string()]
    }

    #[zbus(name = "Release")]
    fn release(&self) {
        logf!(self.gateway.logger, "advertisement released by BlueZ");
    }
}

fn dbus_path(path: &str) -> ObjectPath<'_> {
    ObjectPath::try_from(path).expect("valid object path")
}

async fn bluez_proxy<'a>(
    connection: &'a Connection,
    interface: &'a str,
) -> zbus::Result<zbus::Proxy<'a>> {
    zbus::Proxy::new(connection, "org.bluez", ADAPTER_PATH, interface).await
}

async fn remove_gatt_objects(connection: &Connection) {
    let server = connection.object_server();
    for index in 0..5 {
        let _ = server
            .remove::<Characteristic, _>(characteristic_path(index))
            .await;
    }
    let _ = server.remove::<Service, _>(dbus_path(SVC_PATH)).await;
    let _ = server
        .remove::<zbus::fdo::ObjectManager, _>(dbus_path(APP_PATH))
        .await;
}

impl Gateway {
    pub async fn register_gatt(self: &Arc<Self>) -> Result<(), String> {
        let connection = self
            .connection()
            .cloned()
            .ok_or_else(|| "no dbus connection".to_string())?;
        // A previous attempt may have exported objects before BlueZ rejected
        // RegisterApplication. zbus does not overwrite an existing interface,
        // so clear the paths first instead of serving stale characteristics.
        remove_gatt_objects(&connection).await;
        let result = self.register_gatt_objects(&connection).await;
        if result.is_err() {
            remove_gatt_objects(&connection).await;
        }
        result
    }

    async fn register_gatt_objects(
        self: &Arc<Self>,
        connection: &Connection,
    ) -> Result<(), String> {
        let chars = vec![
            Characteristic::new(self.clone(), 0, CHAR_SSID, &["read", "write"]),
            Characteristic::new(self.clone(), 1, CHAR_PSK, &["write"]),
            Characteristic::new(self.clone(), 2, CHAR_TOKEN, &["write"]),
            Characteristic::new(self.clone(), 3, CHAR_COMMIT, &["write"]),
            Characteristic::new(self.clone(), 4, CHAR_STATUS, &["read", "notify"]),
        ];
        let server = connection.object_server();

        for (index, characteristic) in chars.iter().enumerate() {
            server
                .at(
                    characteristic.path().clone(),
                    characteristic.as_ref().clone(),
                )
                .await
                .map_err(|err| format!("export char {index}: {err}"))?;
        }
        server
            .at(
                dbus_path(SVC_PATH),
                Service::new(chars.iter().map(|c| c.path().clone()).collect()),
            )
            .await
            .map_err(|err| format!("export service: {err}"))?;
        // The ObjectManager must be added after its children so
        // GetManagedObjects sees the complete application tree.
        server
            .at(dbus_path(APP_PATH), zbus::fdo::ObjectManager)
            .await
            .map_err(|err| format!("export object manager: {err}"))?;

        {
            let mut state = self.lock_state();
            state.chars.clone_from(&chars);
            state.by_name.clear();
            for characteristic in &chars {
                state
                    .by_name
                    .insert(characteristic.name().to_string(), characteristic.clone());
            }
        }

        let options: HashMap<String, OwnedValue> = HashMap::new();
        bluez_proxy(connection, "org.bluez.GattManager1")
            .await
            .map_err(|err| format!("RegisterApplication: {err}"))?
            .call_method("RegisterApplication", &(dbus_path(APP_PATH), options))
            .await
            .map_err(|err| format!("RegisterApplication: {err}"))?;
        logf!(self.logger, "GATT application registered");
        Ok(())
    }

    pub async fn unregister_gatt(&self) {
        let Some(connection) = self.connection().cloned() else {
            return;
        };
        let result = async {
            let proxy = bluez_proxy(&connection, "org.bluez.GattManager1").await?;
            proxy
                .call_method("UnregisterApplication", &dbus_path(APP_PATH))
                .await
        }
        .await;
        if let Err(err) = result {
            logf!(self.logger, "UnregisterApplication: {err}");
        }
        remove_gatt_objects(&connection).await;
        let mut state = self.lock_state();
        state.chars.clear();
        state.by_name.clear();
    }

    pub async fn register_advertisement(self: &Arc<Self>) -> Result<(), String> {
        let connection = self
            .connection()
            .cloned()
            .ok_or_else(|| "no dbus connection".to_string())?;
        let _ = connection
            .object_server()
            .remove::<Advertisement, _>(dbus_path(ADV_PATH))
            .await;
        let result = async {
            connection
                .object_server()
                .at(
                    dbus_path(ADV_PATH),
                    Advertisement {
                        gateway: self.clone(),
                    },
                )
                .await
                .map_err(|err| format!("export advertisement: {err}"))?;
            let options: HashMap<String, OwnedValue> = HashMap::new();
            bluez_proxy(&connection, "org.bluez.LEAdvertisingManager1")
                .await
                .map_err(|err| format!("RegisterAdvertisement: {err}"))?
                .call_method("RegisterAdvertisement", &(dbus_path(ADV_PATH), options))
                .await
                .map_err(|err| format!("RegisterAdvertisement: {err}"))?;
            Ok::<(), String>(())
        }
        .await;
        if result.is_err() {
            let _ = connection
                .object_server()
                .remove::<Advertisement, _>(dbus_path(ADV_PATH))
                .await;
        } else {
            logf!(self.logger, "advertisement registered");
        }
        result
    }

    pub async fn unregister_advertisement(&self) {
        let Some(connection) = self.connection().cloned() else {
            return;
        };
        let result = async {
            let proxy = bluez_proxy(&connection, "org.bluez.LEAdvertisingManager1").await?;
            proxy
                .call_method("UnregisterAdvertisement", &dbus_path(ADV_PATH))
                .await
        }
        .await;
        if let Err(err) = result {
            logf!(self.logger, "UnregisterAdvertisement: {err}");
        }
        let _ = connection
            .object_server()
            .remove::<Advertisement, _>(dbus_path(ADV_PATH))
            .await;
    }

    pub async fn set_adapter_connectable(&self, value: bool) {
        let Some(connection) = self.connection().cloned() else {
            return;
        };
        let result = async {
            let properties =
                zbus::fdo::PropertiesProxy::new(&connection, "org.bluez", ADAPTER_PATH).await?;
            properties
                .set(
                    zbus::names::InterfaceName::try_from("org.bluez.Adapter1")
                        .expect("valid interface"),
                    "Connectable",
                    Value::Bool(value),
                )
                .await
        }
        .await;
        if let Err(err) = result {
            logf!(self.logger, "set Connectable={value}: {err}");
        }
    }

    pub async fn emit_value(&self, characteristic: &Characteristic, value: &[u8]) {
        let Some(connection) = self.connection() else {
            return;
        };
        let emitter = match SignalEmitter::new(connection, characteristic.path().clone()) {
            Ok(emitter) => emitter,
            Err(err) => {
                logf!(self.logger, "emit PropertiesChanged: {err}");
                return;
            }
        };
        let mut changed: HashMap<&str, Value<'_>> = HashMap::new();
        changed.insert("Value", Value::from(value.to_vec()));
        let invalidated: Vec<&str> = Vec::new();
        let body = ("org.bluez.GattCharacteristic1", changed, invalidated);
        if let Err(err) = emitter
            .emit(
                "org.freedesktop.DBus.Properties",
                "PropertiesChanged",
                &body,
            )
            .await
        {
            logf!(self.logger, "emit PropertiesChanged: {err}");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::gateway::Gateway;
    use crate::test_support::{new_gateway, read_file};

    async fn wait_status(gateway: &Gateway, path: &Path, expected: &str) {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let status = std::fs::read_to_string(path).unwrap_or_default();
            if status.trim() == expected {
                return;
            }
            if Instant::now() >= deadline {
                panic!("status = {:?}, want {expected:?}", status.trim());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
            let _ = gateway;
        }
    }

    fn insert(gateway: &Arc<Gateway>, name: &str, uuid: &str) {
        let characteristic = Characteristic::new(gateway.clone(), 0, uuid, &[]);
        gateway
            .lock_state()
            .by_name
            .insert(name.to_string(), characteristic);
    }

    #[tokio::test]
    async fn characteristic_read_write_offsets() {
        let context = new_gateway("gatt-offset", Path::new("/nonexistent/wpa_cli"));
        let gateway = &context.gateway;
        let characteristic = Characteristic::new(gateway.clone(), 0, CHAR_SSID, &["read", "write"]);
        assert_eq!(characteristic.name(), "ssid");
        assert_eq!(characteristic.prop_uuid(), CHAR_SSID);
        characteristic
            .write_value(b"Home".to_vec(), HashMap::new())
            .await
            .expect("write initial");
        let offset4 = HashMap::from([("offset".to_string(), OwnedValue::from(4i32))]);
        characteristic
            .write_value(b"WiFi".to_vec(), offset4.clone())
            .await
            .expect("write offset");
        assert_eq!(characteristic.value(), b"HomeWiFi");
        let read = characteristic
            .read_value(offset4)
            .await
            .expect("read offset");
        assert_eq!(read, b"WiFi");
        let offset99 = HashMap::from([("offset".to_string(), OwnedValue::from(99i32))]);
        assert!(characteristic.read_value(offset99).await.is_err());
        let offset600 = HashMap::from([("offset".to_string(), OwnedValue::from(600i32))]);
        assert!(
            characteristic
                .write_value(b"x".to_vec(), offset600)
                .await
                .is_err()
        );
        let offset20 = HashMap::from([("offset".to_string(), OwnedValue::from(20i32))]);
        assert!(
            characteristic
                .write_value(b"x".to_vec(), offset20)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn long_write_uint16_offsets() {
        let context = new_gateway("gatt-long-write", Path::new("/nonexistent/wpa_cli"));
        let characteristic = Characteristic::new(context.gateway.clone(), 1, CHAR_PSK, &["write"]);

        // BlueZ long write: first chunk at offset 0, second at the current length.
        characteristic
            .write_value(b"password".to_vec(), HashMap::new())
            .await
            .expect("first chunk");
        let offset8 = HashMap::from([(
            "offset".to_string(),
            OwnedValue::try_from(Value::U16(8)).expect("owned u16"),
        )]);
        characteristic
            .write_value(b"123".to_vec(), offset8.clone())
            .await
            .expect("second chunk with uint16 offset");
        assert_eq!(characteristic.value(), b"password123");

        let read = characteristic
            .read_value(offset8)
            .await
            .expect("read with uint16 offset");
        assert_eq!(read, b"123");

        let past_end = HashMap::from([(
            "offset".to_string(),
            OwnedValue::try_from(Value::U16(20)).expect("owned u16"),
        )]);
        assert!(characteristic.read_value(past_end.clone()).await.is_err());
        assert!(
            characteristic
                .write_value(b"x".to_vec(), past_end)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn characteristic_commit_and_notify() {
        let context = new_gateway("gatt-notify", Path::new("/nonexistent/wpa_cli"));
        let gateway = &context.gateway;
        let characteristic =
            Characteristic::new(gateway.clone(), 4, CHAR_STATUS, &["read", "notify"]);
        characteristic.start_notify().await.expect("start notify");
        assert!(characteristic.notifying());
        characteristic.stop_notify().await.expect("stop notify");
        assert!(!characteristic.notifying());
        assert!(
            characteristic
                .write_value(b"x".to_vec(), HashMap::new())
                .await
                .is_err()
        );

        gateway.set_status("IDLE").await;
        let commit = Characteristic::new(gateway.clone(), 3, CHAR_COMMIT, &["write"]);
        commit
            .write_value(b"CANCEL".to_vec(), HashMap::new())
            .await
            .expect("commit cancel");
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(read_file(&context.dir.join("status")).trim(), "IDLE");
    }

    #[tokio::test]
    async fn handle_commit_validation() {
        let context = new_gateway("gatt-commit", Path::new("/nonexistent/wpa_cli"));
        let gateway = &context.gateway;
        for name in ["ssid", "psk", "token"] {
            insert(gateway, name, CHAR_SSID);
        }
        gateway.handle_commit(&[]).await;
        wait_status(gateway, &context.dir.join("status"), "ERROR:EMPTY_COMMIT").await;

        gateway
            .lock_state()
            .by_name
            .get("token")
            .expect("token")
            .set_value(b"wrong");
        gateway.handle_commit(b"1").await;
        wait_status(gateway, &context.dir.join("status"), "ERROR:BAD_TOKEN").await;

        gateway
            .lock_state()
            .by_name
            .get("token")
            .expect("token")
            .set_value(b"123456");
        gateway
            .lock_state()
            .by_name
            .get("ssid")
            .expect("ssid")
            .set_value(b"Home");
        gateway
            .lock_state()
            .by_name
            .get("psk")
            .expect("psk")
            .set_value(b"");
        gateway.handle_commit(b"1").await;
        wait_status(gateway, &context.dir.join("status"), "ERROR:EMPTY_PSK").await;
    }

    #[tokio::test]
    async fn handle_commit_dry_run() {
        let context = new_gateway("gatt-dry-run", Path::new("/nonexistent/wpa_cli"));
        let gateway = &context.gateway;
        insert(gateway, "ssid", CHAR_SSID);
        insert(gateway, "psk", CHAR_PSK);
        insert(gateway, "token", CHAR_TOKEN);
        gateway
            .lock_state()
            .by_name
            .get("ssid")
            .expect("ssid")
            .set_value(b"Home");
        gateway
            .lock_state()
            .by_name
            .get("psk")
            .expect("psk")
            .set_value(b"password123");
        gateway
            .lock_state()
            .by_name
            .get("token")
            .expect("token")
            .set_value(b"123456");
        gateway.handle_commit(b"1").await;
        wait_status(gateway, &context.dir.join("status"), "DRY_RUN_OK").await;
    }

    #[tokio::test]
    async fn properties_and_objects() {
        let context = new_gateway("gatt-props", Path::new("/nonexistent/wpa_cli"));
        let gateway = &context.gateway;
        let characteristic = Characteristic::new(gateway.clone(), 0, CHAR_SSID, &["read"]);
        characteristic.set_value(b"abc");
        assert_eq!(characteristic.prop_uuid(), CHAR_SSID);
        assert_eq!(
            characteristic.prop_service(),
            OwnedObjectPath::try_from(SVC_PATH).unwrap()
        );
        assert_eq!(characteristic.prop_value(), b"abc");
        assert_eq!(characteristic.prop_flags(), vec!["read".to_string()]);
        assert!(characteristic.prop_descriptors().is_empty());

        let service = Service::new(vec![characteristic.path().clone()]);
        assert_eq!(service.prop_uuid(), SERVICE_UUID);
        assert!(service.prop_primary());
        assert_eq!(service.prop_characteristics().len(), 1);
        assert!(gateway.enable_ble().await.is_err());
    }

    #[tokio::test]
    async fn advertisement_properties() {
        let context = new_gateway("gatt-adv", Path::new("/nonexistent/wpa_cli"));
        let gateway = &context.gateway;
        let advertisement = Advertisement {
            gateway: gateway.clone(),
        };
        assert_eq!(advertisement.prop_local_name(), "Station-Gateway");
        assert_eq!(advertisement.prop_type(), "peripheral");
        assert_eq!(
            advertisement.prop_service_uuids(),
            vec![SERVICE_UUID.to_string()]
        );
        assert_eq!(advertisement.prop_includes(), vec!["tx-power".to_string()]);
        advertisement.release();
    }

    #[tokio::test]
    async fn set_characteristic_value_copies() {
        let context = new_gateway("gatt-copy", Path::new("/nonexistent/wpa_cli"));
        let gateway = &context.gateway;
        let characteristic = Characteristic::new(gateway.clone(), 0, CHAR_SSID, &[]);
        let mut source = b"abc".to_vec();
        gateway
            .set_characteristic_value(&characteristic, &source)
            .await;
        source[0] = b'x';
        assert_eq!(characteristic.value(), b"abc");
    }

    #[tokio::test]
    async fn open_input_file() {
        let context = new_gateway("gatt-input", Path::new("/nonexistent/wpa_cli"));
        let path = context.dir.join("event");
        std::fs::write(&path, b"x").expect("write event file");
        let file = tokio::fs::File::open(&path).await.expect("open input");
        drop(file);
    }

    #[test]
    fn characteristic_names() {
        assert_eq!(characteristic_name(CHAR_SSID), "ssid");
        assert_eq!(characteristic_name(CHAR_PSK), "psk");
        assert_eq!(characteristic_name(CHAR_TOKEN), "token");
        assert_eq!(characteristic_name(CHAR_COMMIT), "commit");
        assert_eq!(characteristic_name(CHAR_STATUS), "status");
        assert_eq!(characteristic_name("other"), "unknown");
    }
}
