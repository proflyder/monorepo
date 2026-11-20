use tracing::warn;

use crate::types::{Ncp, NetworkConfig, NetworkKey, SimpleDescriptor};

pub const ENDPOINT_SLOTS: usize = 4;

pub const OFFLINE: u8 = 0;
pub const JOINING: u8 = 1;
pub const CONNECTED: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Formation {
    None,
    AwaitErased,
    Forming,
}

pub struct HostView {
    pub online: bool,
    pub new_network: bool,
    pub formation: Formation,
    pub pending: NetworkConfig,
    pub channel_mask: u32,
    pub update_id: u8,
    pub pan: Option<u16>,
    pub ext_pan: Option<u64>,
    pub ieee: Option<u64>,
    pub key: Option<NetworkKey>,
    pub frame_counter: u32,
    pub slots: [Option<SimpleDescriptor>; ENDPOINT_SLOTS],
}

pub(crate) fn simple(ep: u8, profile: u16, device: u16, i: &[u16], o: &[u16]) -> SimpleDescriptor {
    SimpleDescriptor {
        endpoint: ep,
        profile,
        device,
        version: 0,
        in_clusters: i.to_vec(),
        out_clusters: o.to_vec(),
    }
}

pub fn bit(ch: u8) -> u32 {
    1u32.checked_shl(u32::from(ch)).unwrap_or(0)
}

impl HostView {
    pub fn new() -> Self {
        Self {
            online: true,
            new_network: false,
            formation: Formation::None,
            pending: NetworkConfig::default(),
            channel_mask: 0,
            update_id: 0,
            pan: None,
            ext_pan: None,
            ieee: None,
            key: None,
            frame_counter: 0,
            slots: [
                Some(simple(1, 0x0104, 0x0005, &[0x0000], &[])),
                Some(simple(2, 0xC05E, 0x0840, &[0x0000], &[])),
                Some(simple(0xF2, 0xA1E0, 0x0061, &[0x0021], &[0x0021])),
                None,
            ],
        }
    }

    pub fn attach(&self, coord: &mut dyn Ncp) {
        for d in self.slots.iter().flatten() {
            coord.set_endpoint(d);
        }
    }

    pub fn forget_client(&mut self) {
        self.new_network = false;
        self.pan = None;
        self.ext_pan = None;
        self.ieee = None;
        self.key = None;
        self.frame_counter = 0;
        self.channel_mask = 0;
        self.update_id = 0;
        self.online = true;
    }

    pub fn set_slot(&mut self, coord: &mut dyn Ncp, slot: usize, d: &SimpleDescriptor) {
        if let Some(old) = self.slots[slot].as_ref().map(|s| s.endpoint)
            && old != d.endpoint
        {
            self.slots[slot] = None;
            if !self.slots.iter().flatten().any(|s| s.endpoint == old) {
                coord.remove_endpoint(old);
            }
        }
        self.slots[slot] = Some(d.clone());
        coord.set_endpoint(d);
    }

    pub fn network_state(&self, coord: &dyn Ncp) -> u8 {
        if !self.online {
            return OFFLINE;
        }
        if self.formation != Formation::None {
            return JOINING;
        }
        if coord.ready() && coord.network().formed {
            CONNECTED
        } else {
            OFFLINE
        }
    }

    pub fn next_network(&self, coord: &dyn Ncp) -> NetworkConfig {
        let net = coord.network();
        NetworkConfig {
            channel: self.form_channel(),
            pan: self
                .pan
                .unwrap_or(if net.formed { net.pan } else { 0xFFFF }),
            ext_pan: self
                .ext_pan
                .unwrap_or(if net.formed { net.ext_pan } else { 0 }),
            key: self
                .key
                .unwrap_or(if net.formed { net.key } else { [0; 16] }),
            ieee: self.ieee.unwrap_or(net.ieee),
            frame_counter: self.frame_counter.max(if net.formed {
                net.frame_counter.wrapping_add(1024)
            } else {
                0
            }),
            update_id: self.update_id,
        }
    }

    pub fn form_channel(&self) -> u8 {
        (11..=26)
            .find(|&ch| self.channel_mask & bit(ch) != 0)
            .unwrap_or(15)
    }

    pub fn change_network_state(&mut self, coord: &mut dyn Ncp, target: u8) {
        let (formed, channel) = (coord.network().formed, coord.network().channel);
        if target == OFFLINE {
            self.online = false;
        } else if target == CONNECTED {
            self.online = true;
            if self.new_network || !formed {
                self.pending = self.next_network(coord);
                if formed {
                    coord.factory_reset();
                    self.formation = if self.channel_mask != 0 {
                        Formation::AwaitErased
                    } else {
                        Formation::None
                    };
                } else if self.channel_mask != 0 {
                    self.formation = Formation::AwaitErased;
                }
                self.new_network = false;
            } else if self.channel_mask.count_ones() == 1 && self.channel_mask & bit(channel) == 0 {
                let ch = self.form_channel();
                coord.change_channel(ch, self.update_id);
            }
        }
    }

    pub fn on_changed(&mut self, coord: &dyn Ncp) -> Change {
        if coord.ready() && self.formation == Formation::AwaitErased && !coord.network().formed {
            return Change::Form(self.pending.clone());
        }
        if !coord.ready() && self.formation == Formation::None {
            return Change::DropClient;
        }
        Change::None
    }

    pub fn formed(&mut self, ok: bool) {
        if !ok {
            warn!("zigbee: the radio failed to form the network");
        }
        self.formation = Formation::None;
    }
}

pub enum Change {
    None,
    Form(NetworkConfig),
    DropClient,
}

impl Default for HostView {
    fn default() -> Self {
        Self::new()
    }
}
