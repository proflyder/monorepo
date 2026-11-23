use std::net::Ipv4Addr;

use rusqlite::{OptionalExtension, Row, params};

use super::{Store, StoreError};

pub const MAX_VPN_PEERS: usize = 64;
pub const WIREGUARD_KEY_LENGTH: usize = 44;
const FIRST_CLIENT_HOST: u32 = 2;
const MAX_NAME_BYTES: usize = 64;
const MIN_DEVICE_ID_BYTES: usize = 8;
const MAX_DEVICE_ID_BYTES: usize = 128;
const BASE64_ALPHABET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const KEY_LAST_SYMBOLS: &str = "AEIMQUYcgkosw048";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VpnPool {
    network: u32,
    prefix: u8,
}

impl VpnPool {
    pub fn parse(cidr: &str) -> Result<Self, String> {
        let (address, prefix) = cidr
            .trim()
            .split_once('/')
            .ok_or_else(|| format!("pool {cidr} is not a CIDR"))?;
        let address: Ipv4Addr = address
            .parse()
            .map_err(|_| format!("pool {cidr} has an invalid address"))?;
        let prefix: u8 = prefix
            .parse()
            .map_err(|_| format!("pool {cidr} has an invalid prefix"))?;
        if !(16..=29).contains(&prefix) {
            return Err(format!("pool {cidr} prefix must be within /16../29"));
        }
        let mask = u32::MAX << (32 - u32::from(prefix));
        let network = u32::from(address);
        if network & !mask != 0 {
            return Err(format!("pool {cidr} is not a network address"));
        }
        Ok(Self { network, prefix })
    }

    #[must_use]
    pub fn server_address(&self) -> Ipv4Addr {
        Ipv4Addr::from(self.network + 1)
    }

    #[must_use]
    pub fn address(&self, host: u32) -> Ipv4Addr {
        Ipv4Addr::from(self.network + host)
    }

    #[must_use]
    pub const fn prefix(&self) -> u8 {
        self.prefix
    }

    fn last_client_host(&self) -> u32 {
        (1_u32 << (32 - u32::from(self.prefix))) - 2
    }

    fn contains_host(&self, host: u32) -> bool {
        (FIRST_CLIENT_HOST..=self.last_client_host()).contains(&host)
    }
}

impl std::fmt::Display for VpnPool {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{}/{}",
            Ipv4Addr::from(self.network),
            self.prefix
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VpnPeer {
    pub device_id: String,
    pub public_key: String,
    pub host: u32,
    pub device_name: String,
    pub token_id: String,
    pub created_unix: i64,
    pub updated_unix: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VpnPeerInput {
    pub device_id: String,
    pub public_key: String,
    pub device_name: String,
}

const COLS: &str = "device_id, public_key, host, device_name, token_id, created_unix, updated_unix";

fn scan_peer(row: &Row<'_>) -> rusqlite::Result<VpnPeer> {
    Ok(VpnPeer {
        device_id: row.get(0)?,
        public_key: row.get(1)?,
        host: row.get(2)?,
        device_name: row.get(3)?,
        token_id: row.get(4)?,
        created_unix: row.get(5)?,
        updated_unix: row.get(6)?,
    })
}

pub fn is_wireguard_key(key: &str) -> bool {
    let bytes = key.as_bytes();
    if bytes.len() != WIREGUARD_KEY_LENGTH || bytes[WIREGUARD_KEY_LENGTH - 1] != b'=' {
        return false;
    }
    let body = &key[..WIREGUARD_KEY_LENGTH - 1];
    body.chars().all(|symbol| BASE64_ALPHABET.contains(symbol))
        && body
            .chars()
            .last()
            .is_some_and(|symbol| KEY_LAST_SYMBOLS.contains(symbol))
}

fn clean_device_id(device_id: &str) -> Result<String, StoreError> {
    let device_id = device_id.trim();
    if device_id.len() < MIN_DEVICE_ID_BYTES || device_id.len() > MAX_DEVICE_ID_BYTES {
        return Err(StoreError::invalid("device_id has an invalid length"));
    }
    let printable = device_id
        .chars()
        .all(|symbol| symbol.is_ascii_alphanumeric() || "-_.:".contains(symbol));
    if !printable {
        return Err(StoreError::invalid("device_id has invalid characters"));
    }
    Ok(device_id.to_owned())
}

fn clean_input(input: &VpnPeerInput) -> Result<VpnPeerInput, StoreError> {
    let public_key = input.public_key.trim();
    if !is_wireguard_key(public_key) {
        return Err(StoreError::invalid("public_key is not a WireGuard key"));
    }
    let device_name = input.device_name.trim();
    if device_name.len() > MAX_NAME_BYTES || device_name.chars().any(char::is_control) {
        return Err(StoreError::invalid("device_name is invalid"));
    }
    Ok(VpnPeerInput {
        device_id: clean_device_id(&input.device_id)?,
        public_key: public_key.to_owned(),
        device_name: device_name.to_owned(),
    })
}

impl Store {
    #[cfg(test)]
    pub fn register_vpn_peer(
        &self,
        pool: VpnPool,
        input: &VpnPeerInput,
        now_unix: i64,
    ) -> Result<VpnPeer, StoreError> {
        self.register_vpn_peer_for(pool, input, "", true, now_unix)
    }

    pub fn register_vpn_peer_for(
        &self,
        pool: VpnPool,
        input: &VpnPeerInput,
        token_id: &str,
        allow_claim: bool,
        now_unix: i64,
    ) -> Result<VpnPeer, StoreError> {
        let peer = clean_input(input)?;
        let token_id = token_id.to_owned();
        self.with_tx(move |transaction| {
            let owner: Option<String> = transaction
                .query_row(
                    "SELECT device_id FROM vpn_peers WHERE public_key = ?1",
                    params![peer.public_key],
                    |row| row.get(0),
                )
                .optional()?;
            if owner.as_deref().is_some_and(|owner| owner != peer.device_id) {
                return Err(StoreError::precondition(
                    "public_key belongs to another device",
                ));
            }
            let existing = transaction
                .query_row(
                    &format!("SELECT {COLS} FROM vpn_peers WHERE device_id = ?1"),
                    params![peer.device_id],
                    scan_peer,
                )
                .optional()?;
            if let Some(current) = existing.as_ref()
                && !current.token_id.is_empty()
                && !token_id.is_empty()
                && current.token_id != token_id
                && current.public_key != peer.public_key
                && !allow_claim
            {
                return Err(StoreError::precondition(
                    "vpn peer belongs to another application token",
                ));
            }
            let host = match existing.as_ref() {
                Some(current) if pool.contains_host(current.host) => current.host,
                _ => free_host(transaction, pool, existing.is_some())?,
            };
            transaction.execute(
                "INSERT INTO vpn_peers (device_id, public_key, host, device_name, token_id, created_unix, updated_unix)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
                 ON CONFLICT(device_id) DO UPDATE SET
                    public_key = excluded.public_key,
                    host = excluded.host,
                    device_name = CASE WHEN excluded.device_name = ''
                        THEN vpn_peers.device_name ELSE excluded.device_name END,
                    token_id = CASE WHEN excluded.token_id = ''
                        THEN vpn_peers.token_id ELSE excluded.token_id END,
                    updated_unix = excluded.updated_unix",
                params![
                    peer.device_id,
                    peer.public_key,
                    host,
                    peer.device_name,
                    token_id,
                    now_unix
                ],
            )?;
            transaction
                .query_row(
                    &format!("SELECT {COLS} FROM vpn_peers WHERE device_id = ?1"),
                    params![peer.device_id],
                    scan_peer,
                )
                .map_err(StoreError::from)
        })
    }

    pub fn backfill_vpn_peer_owners(&self, legacy_id: &str) -> Result<usize, StoreError> {
        let legacy_id = legacy_id.to_owned();
        self.call(move |connection| {
            let changed = connection.execute(
                "UPDATE vpn_peers SET token_id = ?1 WHERE token_id = ''",
                params![legacy_id],
            )?;
            Ok(changed)
        })
    }

    pub fn remove_vpn_peer_for(
        &self,
        device_id: &str,
        token_id: Option<&str>,
    ) -> Result<bool, StoreError> {
        let device_id = device_id.trim().to_owned();
        let token_id = token_id.map(str::to_owned);
        self.call(move |connection| {
            let changed = match token_id.as_deref() {
                Some(token) => connection.execute(
                    "DELETE FROM vpn_peers WHERE device_id = ?1 AND token_id = ?2",
                    params![device_id, token],
                )?,
                None => connection.execute(
                    "DELETE FROM vpn_peers WHERE device_id = ?1",
                    params![device_id],
                )?,
            };
            Ok(changed > 0)
        })
    }

    pub fn vpn_peer(&self, device_id: &str) -> Result<Option<VpnPeer>, StoreError> {
        let device_id = device_id.trim().to_owned();
        self.call(move |connection| {
            connection
                .query_row(
                    &format!("SELECT {COLS} FROM vpn_peers WHERE device_id = ?1"),
                    params![device_id],
                    scan_peer,
                )
                .optional()
        })
    }

    pub fn list_vpn_peers(&self) -> Result<Vec<VpnPeer>, StoreError> {
        self.call(|connection| {
            let mut statement =
                connection.prepare(&format!("SELECT {COLS} FROM vpn_peers ORDER BY host ASC"))?;
            let rows = statement.query_map([], scan_peer)?;
            rows.collect()
        })
    }

    #[cfg(test)]
    pub fn remove_vpn_peer(&self, device_id: &str) -> Result<bool, StoreError> {
        let device_id = clean_device_id(device_id)?;
        self.call(move |connection| {
            let deleted = connection.execute(
                "DELETE FROM vpn_peers WHERE device_id = ?1",
                params![device_id],
            )?;
            Ok(deleted > 0)
        })
    }
}

fn free_host(
    transaction: &rusqlite::Transaction<'_>,
    pool: VpnPool,
    replacing: bool,
) -> Result<u32, StoreError> {
    let mut statement = transaction.prepare("SELECT host FROM vpn_peers")?;
    let used = statement
        .query_map([], |row| row.get::<_, u32>(0))?
        .collect::<rusqlite::Result<std::collections::BTreeSet<u32>>>()?;
    if !replacing && used.len() >= MAX_VPN_PEERS {
        return Err(StoreError::limit("too many VPN peers"));
    }
    (FIRST_CLIENT_HOST..=pool.last_client_host())
        .find(|host| !used.contains(host))
        .ok_or_else(|| StoreError::limit("VPN pool is exhausted"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Config;

    const KEY_A: &str = "aGVsbG8td29ybGQtaGVsbG8td29ybGQtaGVsbG8tdzA=";
    const KEY_B: &str = "YmJiYmJiYmJiYmJiYmJiYmJiYmJiYmJiYmJiYmJiYmI=";

    fn store() -> Store {
        Store::open_in_memory(Config::default()).expect("store")
    }

    fn pool() -> VpnPool {
        VpnPool::parse("10.77.2.0/24").expect("pool")
    }

    fn input(device_id: &str, key: &str) -> VpnPeerInput {
        VpnPeerInput {
            device_id: device_id.to_owned(),
            public_key: key.to_owned(),
            device_name: "Pixel".to_owned(),
        }
    }

    #[test]
    fn keys_are_validated_like_wireguard() {
        assert!(is_wireguard_key(KEY_A));
        assert!(is_wireguard_key(KEY_B));
        assert!(!is_wireguard_key("short="));
        assert!(!is_wireguard_key(
            "aGVsbG8td29ybGQtaGVsbG8td29ybGQtaGVsbG8tdzB="
        ));
        assert!(!is_wireguard_key(
            "aGVsbG8td29ybGQtaGVsbG8td29ybGQtaGVsbG8td!A="
        ));
    }

    #[test]
    fn pools_are_parsed_strictly() {
        let pool = pool();
        assert_eq!(pool.to_string(), "10.77.2.0/24");
        assert_eq!(pool.server_address(), Ipv4Addr::new(10, 77, 2, 1));
        assert_eq!(pool.last_client_host(), 254);
        assert!(VpnPool::parse("10.77.2.1/24").is_err());
        assert!(VpnPool::parse("10.77.2.0/31").is_err());
        assert!(VpnPool::parse("10.77.2.0").is_err());
    }

    #[test]
    fn a_device_keeps_its_address_when_its_key_rotates() {
        let store = store();
        let first = store
            .register_vpn_peer(pool(), &input("device-aaaaaaaa", KEY_A), 100)
            .expect("register");
        assert_eq!(first.host, 2);
        let rotated = store
            .register_vpn_peer(pool(), &input("device-aaaaaaaa", KEY_B), 200)
            .expect("rotate");
        assert_eq!(rotated.host, 2);
        assert_eq!(rotated.public_key, KEY_B);
        assert_eq!(rotated.created_unix, 100);
        assert_eq!(rotated.updated_unix, 200);
        assert_eq!(store.list_vpn_peers().expect("list").len(), 1);
    }

    #[test]
    fn devices_get_distinct_addresses_and_freed_ones_are_reused() {
        let store = store();
        store
            .register_vpn_peer(pool(), &input("device-aaaaaaaa", KEY_A), 1)
            .expect("a");
        let second = store
            .register_vpn_peer(pool(), &input("device-bbbbbbbb", KEY_B), 1)
            .expect("b");
        assert_eq!(second.host, 3);
        assert!(store.remove_vpn_peer("device-aaaaaaaa").expect("remove"));
        assert!(!store.remove_vpn_peer("device-aaaaaaaa").expect("again"));
        let third = store
            .register_vpn_peer(pool(), &input("device-cccccccc", KEY_A), 2)
            .expect("c");
        assert_eq!(third.host, 2);
    }

    #[test]
    fn a_key_cannot_be_claimed_by_another_device() {
        let store = store();
        store
            .register_vpn_peer(pool(), &input("device-aaaaaaaa", KEY_A), 1)
            .expect("a");
        assert!(
            store
                .register_vpn_peer(pool(), &input("device-bbbbbbbb", KEY_A), 1)
                .is_err()
        );
    }

    #[test]
    fn an_empty_name_keeps_the_stored_one() {
        let store = store();
        store
            .register_vpn_peer(pool(), &input("device-aaaaaaaa", KEY_A), 1)
            .expect("a");
        let refreshed = store
            .register_vpn_peer(
                pool(),
                &VpnPeerInput {
                    device_name: String::new(),
                    ..input("device-aaaaaaaa", KEY_A)
                },
                2,
            )
            .expect("refresh");
        assert_eq!(refreshed.device_name, "Pixel");
    }

    #[test]
    fn a_small_pool_is_exhausted_cleanly() {
        let store = store();
        let tiny = VpnPool::parse("10.77.2.0/29").expect("pool");
        for index in 0..5 {
            let key = format!("{index}{}", &KEY_A[1..]);
            store
                .register_vpn_peer(tiny, &input(&format!("device-{index:08}"), &key), 1)
                .expect("fits");
        }
        let key = format!("Z{}", &KEY_A[1..]);
        assert!(
            store
                .register_vpn_peer(tiny, &input("device-overflow", &key), 1)
                .is_err()
        );
    }

    #[test]
    fn invalid_device_ids_are_rejected() {
        let store = store();
        assert!(
            store
                .register_vpn_peer(pool(), &input("short", KEY_A), 1)
                .is_err()
        );
        assert!(store.remove_vpn_peer("bad id with spaces").is_err());
    }
    #[test]
    fn vpn_peers_are_scoped_to_the_registering_token() {
        let store = store();
        store
            .register_vpn_peer_for(pool(), &input("device-aaaaaaaa", KEY_A), "owner", true, 100)
            .expect("owner");
        store
            .register_vpn_peer_for(pool(), &input("device-bbbbbbbb", KEY_B), "mama", false, 101)
            .expect("mama");
        let error = store
            .register_vpn_peer_for(pool(), &input("device-aaaaaaaa", KEY_B), "mama", false, 102)
            .expect_err("a foreign key cannot take the peer");
        assert_eq!(error.kind(), Some(crate::domain::ErrorKind::Precondition));
        assert!(
            !store
                .remove_vpn_peer_for("device-aaaaaaaa", Some("mama"))
                .expect("scoped remove")
        );
        assert!(
            store
                .remove_vpn_peer_for("device-bbbbbbbb", Some("mama"))
                .expect("own remove")
        );
    }
}
