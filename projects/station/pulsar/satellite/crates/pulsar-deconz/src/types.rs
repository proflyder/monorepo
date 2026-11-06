//! APS/NWK values shared by the deCONZ server and the zigbee coordinator: frames, network
//! state, lightweight descriptors, and the two ports between them.
//!
//! The server must not know the radio's implementation and the coordinator must not know the
//! socket, so the server talks to [`Ncp`] (the network co-processor) and the coordinator reports
//! through [`Hooks`] — the same split as the C++ objects, with the callbacks turned around.

use pulsar_zbhci::bytes::{get_le16, put_le16};

/// APS NO_ACK: the delivery status of frames the radio lost (restart, no confirm, refused).
pub const STATUS_NO_ACK: u8 = 0xA7;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ApsFrame {
    /// NWK addresses (dst: group when `group`); dst >= 0xFFF8 is a broadcast.
    pub src: u16,
    pub dst: u16,
    pub group: bool,
    pub src_ep: u8,
    pub dst_ep: u8,
    pub profile: u16,
    pub cluster: u16,
    pub data: Vec<u8>,
    pub lqi: u8,
    pub rssi: i8,
    /// Sending: deCONZ transmit options (0x04 = APS ACK).
    pub tx_options: u8,
    /// Sending: 0 = default.
    pub radius: u8,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SimpleDescriptor {
    pub endpoint: u8,
    pub profile: u16,
    pub device: u16,
    pub version: u8,
    pub in_clusters: Vec<u16>,
    pub out_clusters: Vec<u16>,
}

/// `[endpoint][profile LE][device LE][version][n in][in…][n out][out…]`: the layout of ZDO
/// Simple_Desc_rsp, deCONZ endpoint parameters and the radio's 0x0A04.
pub fn put_descriptor(v: &mut Vec<u8>, d: &SimpleDescriptor) {
    v.push(d.endpoint);
    put_le16(v, d.profile);
    put_le16(v, d.device);
    v.push(d.version);
    for list in [&d.in_clusters, &d.out_clusters] {
        // A count byte: lists never hold more than the radio's 32 clusters.
        v.push(list.len() as u8);
        for &c in list {
            put_le16(v, c);
        }
    }
}

pub fn get_descriptor(p: &[u8]) -> Option<SimpleDescriptor> {
    if p.len() < 7 {
        return None;
    }
    let mut d = SimpleDescriptor {
        endpoint: p[0],
        profile: get_le16(&p[1..]),
        device: get_le16(&p[3..]),
        version: p[5],
        ..SimpleDescriptor::default()
    };
    let mut i = 6;
    for list in [&mut d.in_clusters, &mut d.out_clusters] {
        if i >= p.len() {
            return None;
        }
        let count = usize::from(p[i]);
        i += 1;
        if i + 2 * count > p.len() {
            return None;
        }
        list.extend(
            p[i..i + 2 * count]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes(*c)),
        );
        i += 2 * count;
    }
    Some(d)
}

pub type NetworkKey = [u8; 16];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Network {
    pub formed: bool,
    pub forming: bool,
    pub pan: u16,
    pub nwk: u16,
    pub ext_pan: u64,
    pub ieee: u64,
    pub channel: u8,
    pub update_id: u8,
    pub key: NetworkKey,
    pub key_seq: u8,
    /// Outgoing NWK frame counter.
    pub frame_counter: u32,
}

impl Default for Network {
    fn default() -> Self {
        Self {
            formed: false,
            forming: false,
            pan: 0xFFFF,
            nwk: 0xFFFE,
            ext_pan: 0,
            ieee: 0,
            channel: 0,
            update_id: 0,
            key: [0; 16],
            key_seq: 0,
            frame_counter: 0,
        }
    }
}

/// A network to form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkConfig {
    pub channel: u8,
    /// 0xFFFF: the stack picks.
    pub pan: u16,
    /// 0: the coordinator's IEEE address.
    pub ext_pan: u64,
    /// All zero: the stack picks.
    pub key: NetworkKey,
    /// 0: the radio's own.
    pub ieee: u64,
    pub frame_counter: u32,
    pub update_id: u8,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            channel: 15,
            pan: 0xFFFF,
            ext_pan: 0,
            key: [0; 16],
            ieee: 0,
            frame_counter: 0,
            update_id: 0,
        }
    }
}

/// What the deCONZ server needs from the coordinator (the C++ `ZigbeeCoordinator` reference).
/// Every method is the operation the server performed through its pointer.
pub trait Ncp {
    /// The radio reported its network state.
    fn ready(&self) -> bool;
    fn network(&self) -> &Network;
    /// Frames waiting in the link's queue (in flight included).
    fn link_queued(&self) -> usize;
    /// Data requests whose 0x8A10 is due.
    fn pending(&self) -> usize;
    /// Queues an APS data request (dst by NWK address or group, or by IEEE address when
    /// `dst_ieee != 0`). false: it cannot be sent at all.
    fn send(&mut self, handle: u32, frame: ApsFrame, dst_ieee: u64) -> bool;
    fn permit_join(&mut self, seconds: u8);
    fn set_endpoint(&mut self, d: &SimpleDescriptor);
    fn remove_endpoint(&mut self, endpoint: u8);
    /// Erases the network in the radio (devices must join again).
    fn factory_reset(&mut self);
    /// Forms a network (none may exist); [`Hooks::formed`] reports the result.
    fn form(&mut self, cfg: &NetworkConfig, hooks: &mut dyn Hooks);
    /// Moves the formed network to another channel.
    fn change_channel(&mut self, channel: u8, update_id: u8);
}

/// The coordinator's owner: the deCONZ server and the role service. Every hook gets the
/// coordinator back and may call into it, as the C++ hooks called their `coord_` reference.
pub trait Hooks {
    /// An APS indication, or a ZDO answer of the coordinator itself.
    fn received(&mut self, ncp: &mut dyn Ncp, frame: &ApsFrame);
    /// Delivery result of [`Ncp::send`]: 0 = delivered (APS ACK for unicasts), else a Zigbee
    /// status.
    fn sent(&mut self, ncp: &mut dyn Ncp, handle: u32, status: u8);
    /// `ready()` or `network()` changed.
    fn changed(&mut self, ncp: &mut dyn Ncp);
    /// Result of [`Ncp::form`]: true once the network is up, false when the radio refused or
    /// failed, or the formation was superseded or cut by a radio restart.
    fn formed(&mut self, ncp: &mut dyn Ncp, ok: bool);
}
