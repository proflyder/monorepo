//! The host's view of the network: what a deCONZ client wrote over the radio's current state,
//! deCONZ's endpoint slots, and the erase-then-form sequence zigpy drives.

use tracing::warn;

use crate::types::{Ncp, NetworkConfig, NetworkKey, SimpleDescriptor};

/// deCONZ endpoint slots: zigpy-deconz and zigbee-herdsman write the coordinator's endpoints by
/// slot; the coordinator gets the endpoints they hold.
pub const ENDPOINT_SLOTS: usize = 4;

/// Network states and device state flags (in the device state byte, above the 2-bit state).
pub const OFFLINE: u8 = 0;
pub const JOINING: u8 = 1;
pub const CONNECTED: u8 = 2;

/// Forming a new network: wait for the old one to be erased, then form; JOINING meanwhile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Formation {
    None,
    AwaitErased,
    Forming,
}

/// Values a deCONZ client wrote for the network it is about to form; `None` keeps the radio's.
pub struct HostView {
    /// The host's change_network_state: CONNECTED (true) / OFFLINE.
    pub online: bool,
    /// The host wrote network parameters the radio does not have.
    pub new_network: bool,
    pub formation: Formation,
    /// The network to form, fixed before the erase.
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

/// Bit `ch` of a channel mask; channels past 31 are in no mask.
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
            // A ConBee's slots, until the host writes its own.
            slots: [
                Some(simple(1, 0x0104, 0x0005, &[0x0000], &[])), // HA
                Some(simple(2, 0xC05E, 0x0840, &[0x0000], &[])), // ZLL
                Some(simple(0xF2, 0xA1E0, 0x0061, &[0x0021], &[0x0021])), // Green Power
                None,
            ],
        }
    }

    /// Sets the coordinator's endpoints to the slots' (before the coordinator starts).
    pub fn attach(&self, coord: &mut dyn Ncp) {
        for d in self.slots.iter().flatten() {
            coord.set_endpoint(d);
        }
    }

    /// A new client: network parameters written by a previous one are not its own (a formation
    /// it started goes on with `pending`).
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
        // A slot taking another endpoint number drops the old endpoint unless another slot holds
        // it (zigbee-herdsman writes endpoint 0xF2 into slot 1).
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

    /// The network the host asked for, from the values it wrote over the radio's current ones.
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
            // Hosts read it before forming and keep it in backups.
            ieee: self.ieee.unwrap_or(net.ieee),
            // The erase restores the counter last saved to NV, behind the running one: devices
            // that keep the key would drop frames until it catches up. 1024: the stack's own NV
            // margin.
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

    /// change_network_state. The reply is the server's.
    pub fn change_network_state(&mut self, coord: &mut dyn Ncp, target: u8) {
        let (formed, channel) = (coord.network().formed, coord.network().channel);
        if target == OFFLINE {
            self.online = false;
        } else if target == CONNECTED {
            self.online = true;
            // zigpy forms a network by writing its parameters, then OFFLINE -> CONNECTED; a
            // channel mask of 0 (with PAN ID 0xFFFF) only erases the old one. The network to
            // form is fixed now: the erase takes the current values with it.
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
                    self.formation = Formation::AwaitErased; // already erased: forms on the next step
                }
                self.new_network = false;
            } else if self.channel_mask.count_ones() == 1 && self.channel_mask & bit(channel) == 0 {
                let ch = self.form_channel();
                coord.change_channel(ch, self.update_id);
            }
        }
    }

    /// After the coordinator reported a change: what the server must do next.
    pub fn on_changed(&mut self, coord: &dyn Ncp) -> Change {
        if coord.ready() && self.formation == Formation::AwaitErased && !coord.network().formed {
            // Forming is set by the caller after form(): form() first supersedes a formation
            // still pending, whose formed(false) resets the state. (C++ set it before, so a
            // superseded formation left the host in NONE while the new one ran.)
            return Change::Form(self.pending.clone());
        }
        // The radio restarted outside a formation: drop the host, it reconnects once the state is
        // known.
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

/// What [`HostView::on_changed`] asks the server to do.
pub enum Change {
    None,
    /// Form `pending`, then set the formation state to forming.
    Form(NetworkConfig),
    /// The radio restarted: the client reconnects once its state is known.
    DropClient,
}

impl Default for HostView {
    fn default() -> Self {
        Self::new()
    }
}
