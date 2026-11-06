//! deCONZ network parameters: the table a ConBee answers, and what the host may write.
//!
//! Only the global trust center link key is supported (no install codes). Values the host does
//! not write keep the radio's current ones.

use pulsar_zbhci::bytes::{get_le16, get_le32, get_le64, put_le16, put_le32, put_le64};

use crate::host::{HostView, bit};
use crate::types::{Ncp, NetworkKey, get_descriptor, put_descriptor};

pub const MAC_ADDRESS: u8 = 0x01;
pub const NWK_PAN_ID: u8 = 0x05;
pub const NWK_ADDRESS: u8 = 0x07;
pub const NWK_EXT_PAN_ID: u8 = 0x08;
pub const DESIGNED_COORDINATOR: u8 = 0x09;
pub const CHANNEL_MASK: u8 = 0x0A;
pub const APS_EXT_PAN_ID: u8 = 0x0B;
pub const TRUST_CENTER_ADDRESS: u8 = 0x0E;
pub const SECURITY_MODE: u8 = 0x10;
pub const CONFIGURE_ENDPOINT: u8 = 0x13;
pub const PREDEFINED_PAN_ID: u8 = 0x15;
pub const NETWORK_KEY: u8 = 0x18;
pub const LINK_KEY: u8 = 0x19;
pub const CURRENT_CHANNEL: u8 = 0x1C;
pub const PERMIT_JOIN: u8 = 0x21;
pub const PROTOCOL_VERSION: u8 = 0x22;
pub const NWK_UPDATE_ID: u8 = 0x24;
pub const WATCHDOG_TTL: u8 = 0x26;
pub const FRAME_COUNTER: u8 = 0x27;
pub const ZDP_RESPONSE_HANDLING: u8 = 0x28;

/// ConBee III-compatible protocol version: keeps zigpy off the commands this radio has no
/// equivalent for (update_neighbor, watchdog TTL, source routes).
pub const PROTOCOL: u16 = 0x0106;
pub const SECURITY_ONLY_TCLK: u8 = 0x03;
pub const TC_LINK_KEY: [u8; 16] = *b"ZigBeeAlliance09";

pub const SUCCESS: u8 = 0;
pub const UNSUPPORTED: u8 = 4;
pub const INVALID_VALUE: u8 = 7;

/// `p`: the parameter id, then whatever the command carries. `(status, value without the id)`.
pub fn read(id: u8, p: &[u8], coord: &dyn Ncp, host: &HostView) -> (u8, Vec<u8>) {
    let net = coord.network();
    let formed = coord.ready() && net.formed;
    let mut v = Vec::new();
    let mut status = SUCCESS;
    match id {
        MAC_ADDRESS | TRUST_CENTER_ADDRESS => put_le64(&mut v, net.ieee),
        NWK_PAN_ID => put_le16(&mut v, if formed { net.pan } else { 0xFFFF }),
        NWK_ADDRESS => put_le16(&mut v, if formed { net.nwk } else { 0xFFFF }),
        NWK_EXT_PAN_ID | APS_EXT_PAN_ID => {
            put_le64(&mut v, if formed { net.ext_pan } else { !0 });
        }
        DESIGNED_COORDINATOR | PREDEFINED_PAN_ID => v.push(1),
        CHANNEL_MASK => {
            let mask = if host.channel_mask != 0 {
                host.channel_mask
            } else if formed {
                bit(net.channel)
            } else {
                0
            };
            put_le32(&mut v, mask);
        }
        SECURITY_MODE => v.push(SECURITY_ONLY_TCLK),
        CONFIGURE_ENDPOINT => {
            let slot = p.get(1).map_or(0xFF, |&s| usize::from(s));
            if slot >= host.slots.len() {
                status = UNSUPPORTED;
            } else {
                v.push(slot as u8);
                put_descriptor(&mut v, &host.slots[slot].clone().unwrap_or_default());
            }
        }
        NETWORK_KEY => {
            v.push(p.get(1).copied().unwrap_or(0));
            if formed {
                v.extend_from_slice(&net.key);
            } else {
                v.extend_from_slice(&[0xFF; 16]);
            }
        }
        LINK_KEY => {
            if p.len() < 9 {
                status = INVALID_VALUE;
            } else {
                v.extend_from_slice(&p[1..9]);
                v.extend_from_slice(&TC_LINK_KEY);
            }
        }
        CURRENT_CHANNEL => v.push(if formed { net.channel } else { 0 }),
        PERMIT_JOIN => v.push(0),
        PROTOCOL_VERSION => put_le16(&mut v, PROTOCOL),
        NWK_UPDATE_ID => v.push(if formed {
            net.update_id
        } else {
            host.update_id
        }),
        FRAME_COUNTER => put_le32(&mut v, net.frame_counter),
        ZDP_RESPONSE_HANDLING => put_le16(&mut v, 0),
        _ => status = UNSUPPORTED,
    }
    if status != SUCCESS {
        v.clear();
    }
    (status, v)
}

/// `v`: the value bytes after the parameter id. Returns the status.
pub fn write(id: u8, v: &[u8], coord: &mut dyn Ncp, host: &mut HostView) -> u8 {
    let (formed, net) = {
        let net = coord.network();
        (coord.ready() && net.formed, net.clone())
    };
    let mut status = SUCCESS;
    let need = |k: usize, status: &mut u8| {
        if v.len() >= k {
            return true;
        }
        *status = INVALID_VALUE;
        false
    };
    match id {
        MAC_ADDRESS => {
            if need(8, &mut status) {
                let ieee = get_le64(v);
                host.ieee = Some(ieee);
                if ieee != net.ieee {
                    host.new_network = true;
                }
            }
        }
        NWK_ADDRESS => {
            if need(2, &mut status) && get_le16(v) != 0x0000 {
                status = UNSUPPORTED;
            }
        }
        DESIGNED_COORDINATOR => {
            if need(1, &mut status) && v[0] != 1 {
                status = UNSUPPORTED;
            }
        }
        FRAME_COUNTER => {
            if need(4, &mut status) {
                host.frame_counter = get_le32(v);
            }
        }
        NWK_PAN_ID => {
            if need(2, &mut status) {
                let pan = get_le16(v);
                host.pan = Some(pan);
                if !formed || pan != net.pan {
                    host.new_network = true;
                }
            }
        }
        NWK_EXT_PAN_ID | APS_EXT_PAN_ID => {
            if need(8, &mut status) {
                let ext = get_le64(v);
                host.ext_pan = Some(ext);
                if !formed || ext != net.ext_pan {
                    host.new_network = true;
                }
            }
        }
        // [index][key]
        NETWORK_KEY => {
            if need(17, &mut status) {
                let mut key: NetworkKey = [0u8; 16];
                key.copy_from_slice(&v[1..17]);
                host.key = Some(key);
                if !formed || key != net.key {
                    host.new_network = true;
                }
            }
        }
        CHANNEL_MASK => {
            if need(4, &mut status) {
                host.channel_mask = get_le32(v);
            }
        }
        NWK_UPDATE_ID => {
            if need(1, &mut status) {
                host.update_id = v[0];
            }
        }
        // Only the global trust center link key; no install-code keys.
        LINK_KEY => {
            if need(24, &mut status) && v[8..24] != TC_LINK_KEY {
                status = UNSUPPORTED;
            }
        }
        CONFIGURE_ENDPOINT => {
            let d = if need(2, &mut status) && usize::from(v[0]) < host.slots.len() {
                get_descriptor(&v[1..])
            } else {
                None
            };
            match d {
                Some(d) => host.set_slot(coord, usize::from(v[0]), &d),
                None if status == SUCCESS => status = INVALID_VALUE,
                None => {}
            }
        }
        PERMIT_JOIN => {
            if need(1, &mut status) {
                coord.permit_join(v[0]);
            }
        }
        TRUST_CENTER_ADDRESS
        | SECURITY_MODE
        | PREDEFINED_PAN_ID
        | WATCHDOG_TTL
        | ZDP_RESPONSE_HANDLING => {}
        _ => status = UNSUPPORTED,
    }
    status
}
