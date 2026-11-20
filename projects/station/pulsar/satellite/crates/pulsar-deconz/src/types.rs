use pulsar_zbhci::bytes::{get_le16, put_le16};

pub const STATUS_NO_ACK: u8 = 0xA7;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ApsFrame {
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
    pub tx_options: u8,
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

pub fn put_descriptor(v: &mut Vec<u8>, d: &SimpleDescriptor) {
    v.push(d.endpoint);
    put_le16(v, d.profile);
    put_le16(v, d.device);
    v.push(d.version);
    for list in [&d.in_clusters, &d.out_clusters] {
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkConfig {
    pub channel: u8,
    pub pan: u16,
    pub ext_pan: u64,
    pub key: NetworkKey,
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

pub trait Ncp {
    fn ready(&self) -> bool;
    fn network(&self) -> &Network;
    fn link_queued(&self) -> usize;
    fn pending(&self) -> usize;
    fn send(&mut self, handle: u32, frame: ApsFrame, dst_ieee: u64) -> bool;
    fn permit_join(&mut self, seconds: u8);
    fn set_endpoint(&mut self, d: &SimpleDescriptor);
    fn remove_endpoint(&mut self, endpoint: u8);
    fn factory_reset(&mut self);
    fn form(&mut self, cfg: &NetworkConfig, hooks: &mut dyn Hooks);
    fn change_channel(&mut self, channel: u8, update_id: u8);
}

pub trait Hooks {
    fn received(&mut self, ncp: &mut dyn Ncp, frame: &ApsFrame);
    fn sent(&mut self, ncp: &mut dyn Ncp, handle: u32, status: u8);
    fn changed(&mut self, ncp: &mut dyn Ncp);
    fn formed(&mut self, ncp: &mut dyn Ncp, ok: bool);
}
