// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Links: `ifinfomsg` requests and replies, bonds and VLANs.

#[allow(clippy::wildcard_imports)] // the numbers module
use super::consts::*;
use crate::codec::{Attrs, Builder, NLM_F_CREATE, NLM_F_DUMP, NLM_F_EXCL, NLM_F_REQUEST, ne_u16, ne_u32, ne_u64};
use crate::{AF_UNSPEC, Error, Result};

/// `IF_OPER_*`, RFC 2863 operational state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperState {
    /// Unknown (also loopback and dummy links).
    Unknown,
    /// Not present.
    NotPresent,
    /// Down.
    Down,
    /// A lower link is down.
    LowerLayerDown,
    /// Testing.
    Testing,
    /// Dormant.
    Dormant,
    /// Up.
    Up,
}

impl OperState {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => OperState::NotPresent,
            2 => OperState::Down,
            3 => OperState::LowerLayerDown,
            4 => OperState::Testing,
            5 => OperState::Dormant,
            6 => OperState::Up,
            _ => OperState::Unknown,
        }
    }
}

/// Bonding modes (`BOND_MODE_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BondMode {
    /// `balance-rr`.
    RoundRobin = 0,
    /// `active-backup`.
    ActiveBackup = 1,
    /// `balance-xor`.
    Xor = 2,
    /// `broadcast`.
    Broadcast = 3,
    /// `802.3ad` (LACP).
    Lacp = 4,
    /// `balance-tlb`.
    Tlb = 5,
    /// `balance-alb`.
    Alb = 6,
}

impl BondMode {
    /// From the kernel's number.
    pub fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            0 => BondMode::RoundRobin,
            1 => BondMode::ActiveBackup,
            2 => BondMode::Xor,
            3 => BondMode::Broadcast,
            4 => BondMode::Lacp,
            5 => BondMode::Tlb,
            6 => BondMode::Alb,
            _ => return None,
        })
    }
}

/// How to create a bond.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BondSpec {
    /// Mode.
    pub mode: BondMode,
    /// MII link monitoring interval, milliseconds (0 = off).
    pub miimon: u32,
    /// `xmit_hash_policy` (0 layer2, 1 layer3+4, 2 layer2+3), if set.
    pub xmit_hash: Option<u8>,
    /// LACP rate: `true` fast, `false` slow; LACP mode only.
    pub lacp_fast: Option<bool>,
}

impl BondSpec {
    /// A bond in `mode` with 100 ms link monitoring.
    pub fn new(mode: BondMode) -> Self {
        Self {
            mode,
            miimon: 100,
            xmit_hash: None,
            lacp_fast: None,
        }
    }
}

/// Interface counters (`rtnl_link_stats64`, first eight fields).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LinkStats {
    /// Packets received.
    pub rx_packets: u64,
    /// Packets sent.
    pub tx_packets: u64,
    /// Bytes received.
    pub rx_bytes: u64,
    /// Bytes sent.
    pub tx_bytes: u64,
    /// Receive errors.
    pub rx_errors: u64,
    /// Send errors.
    pub tx_errors: u64,
    /// Received packets dropped.
    pub rx_dropped: u64,
    /// Sent packets dropped.
    pub tx_dropped: u64,
}

/// One link.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Link {
    /// Interface index.
    pub index: u32,
    /// Name.
    pub name: String,
    /// `ARPHRD_*` (1 Ethernet, 772 loopback).
    pub link_type: u16,
    /// `IFF_*` flags.
    pub flags: u32,
    /// MTU.
    pub mtu: u32,
    /// Hardware address.
    pub mac: Vec<u8>,
    /// Permanent hardware address, when the driver reports one.
    pub perm_mac: Vec<u8>,
    /// Index of the bond this link is enslaved to.
    pub master: Option<u32>,
    /// Index of the parent link (VLANs).
    pub parent: Option<u32>,
    /// `IF_OPER_*` state.
    pub oper: Option<OperState>,
    /// Carrier, when reported.
    pub carrier: Option<bool>,
    /// `IFLA_INFO_KIND`: `bond`, `vlan`, `veth`, `dummy`...
    pub kind: Option<String>,
    /// `IFLA_INFO_SLAVE_KIND`: `bond` for a bond member.
    pub slave_kind: Option<String>,
    /// VLAN id, for `vlan` links.
    pub vlan_id: Option<u16>,
    /// Bond mode, for `bond` links.
    pub bond_mode: Option<BondMode>,
    /// Alias (`IFLA_IFALIAS`).
    pub alias: Option<String>,
    /// Counters.
    pub stats: Option<LinkStats>,
}

impl Link {
    /// Administratively up.
    pub fn is_up(&self) -> bool {
        self.flags & IFF_UP != 0
    }

    /// Carrier present (`IFF_LOWER_UP`, or `IFLA_CARRIER`).
    pub fn has_carrier(&self) -> bool {
        self.carrier.unwrap_or(self.flags & IFF_LOWER_UP != 0)
    }

    /// Loopback.
    pub fn is_loopback(&self) -> bool {
        self.flags & IFF_LOOPBACK != 0
    }
}

/// `ifinfomsg` bytes.
pub fn ifinfomsg(index: u32, flags: u32, change: u32) -> [u8; IFINFOMSG_LEN] {
    let mut b = [0u8; IFINFOMSG_LEN];
    b[0] = AF_UNSPEC;
    b[4..8].copy_from_slice(&index.to_ne_bytes());
    b[8..12].copy_from_slice(&flags.to_ne_bytes());
    b[12..16].copy_from_slice(&change.to_ne_bytes());
    b
}

/// Dump every link.
pub fn dump_request() -> Builder {
    let mut b = Builder::new(RTM_GETLINK, NLM_F_REQUEST | NLM_F_DUMP);
    b.bytes(&ifinfomsg(0, 0, 0));
    b
}

/// Get one link by index.
pub fn get_by_index(index: u32) -> Builder {
    let mut b = Builder::new(RTM_GETLINK, NLM_F_REQUEST);
    b.bytes(&ifinfomsg(index, 0, 0));
    b
}

/// Get one link by name.
pub fn get_by_name(name: &str) -> Builder {
    let mut b = Builder::new(RTM_GETLINK, NLM_F_REQUEST);
    b.bytes(&ifinfomsg(0, 0, 0)).str(IFLA_IFNAME, name);
    b
}

/// What [`set_request`] changes; `None` leaves a field alone.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LinkChange {
    /// Up or down.
    pub up: Option<bool>,
    /// MTU.
    pub mtu: Option<u32>,
    /// Hardware address.
    pub mac: Option<Vec<u8>>,
    /// Bond to enslave to; `Some(0)` releases.
    pub master: Option<u32>,
    /// Alias text.
    pub alias: Option<String>,
}

/// Change a link (`RTM_NEWLINK` on an existing index).
pub fn set_request(index: u32, ch: &LinkChange) -> Builder {
    let (flags, change) = match ch.up {
        Some(true) => (IFF_UP, IFF_UP),
        Some(false) => (0, IFF_UP),
        None => (0, 0),
    };
    let mut b = Builder::new(RTM_NEWLINK, NLM_F_REQUEST);
    b.bytes(&ifinfomsg(index, flags, change));
    if let Some(mtu) = ch.mtu {
        b.u32(IFLA_MTU, mtu);
    }
    if let Some(mac) = &ch.mac {
        b.attr(IFLA_ADDRESS, mac);
    }
    if let Some(m) = ch.master {
        b.u32(IFLA_MASTER, m);
    }
    if let Some(a) = &ch.alias {
        b.attr(IFLA_IFALIAS, a.as_bytes());
    }
    b
}

/// Create a bond device.
pub fn new_bond_request(name: &str, spec: &BondSpec) -> Builder {
    let mut b = Builder::new(RTM_NEWLINK, NLM_F_REQUEST | NLM_F_CREATE | NLM_F_EXCL);
    b.bytes(&ifinfomsg(0, 0, 0)).str(IFLA_IFNAME, name);
    b.nest(IFLA_LINKINFO, |b| {
        b.str(IFLA_INFO_KIND, "bond");
        b.nest(IFLA_INFO_DATA, |b| {
            b.u8(IFLA_BOND_MODE, spec.mode as u8);
            b.u32(IFLA_BOND_MIIMON, spec.miimon);
            if let Some(h) = spec.xmit_hash {
                b.u8(IFLA_BOND_XMIT_HASH_POLICY, h);
            }
            if let Some(f) = spec.lacp_fast {
                b.u8(IFLA_BOND_AD_LACP_RATE, u8::from(f));
            }
        });
    });
    b
}

/// Create an 802.1Q sub-interface `name` of `parent` with `vlan_id`.
pub fn new_vlan_request(name: &str, parent: u32, vlan_id: u16) -> Result<Builder> {
    if !(1..=4094).contains(&vlan_id) {
        return Err(Error::Invalid(format!("VLAN id {vlan_id}")));
    }
    let mut b = Builder::new(RTM_NEWLINK, NLM_F_REQUEST | NLM_F_CREATE | NLM_F_EXCL);
    b.bytes(&ifinfomsg(0, 0, 0))
        .str(IFLA_IFNAME, name)
        .u32(IFLA_LINK, parent);
    b.nest(IFLA_LINKINFO, |b| {
        b.str(IFLA_INFO_KIND, "vlan");
        b.nest(IFLA_INFO_DATA, |b| {
            b.u16(IFLA_VLAN_ID, vlan_id);
            b.be16(IFLA_VLAN_PROTOCOL, ETH_P_8021Q);
        });
    });
    Ok(b)
}

/// Create a link of a kind that needs no data (`dummy`, test use).
pub fn new_simple_request(name: &str, kind: &str) -> Builder {
    let mut b = Builder::new(RTM_NEWLINK, NLM_F_REQUEST | NLM_F_CREATE | NLM_F_EXCL);
    b.bytes(&ifinfomsg(0, 0, 0)).str(IFLA_IFNAME, name);
    b.nest(IFLA_LINKINFO, |b| {
        b.str(IFLA_INFO_KIND, kind);
    });
    b
}

/// Delete a link.
pub fn del_request(index: u32) -> Builder {
    let mut b = Builder::new(RTM_DELLINK, NLM_F_REQUEST);
    b.bytes(&ifinfomsg(index, 0, 0));
    b
}

/// Parse an `RTM_NEWLINK`/`RTM_DELLINK` payload.
pub fn parse(payload: &[u8]) -> Result<Link> {
    if payload.len() < IFINFOMSG_LEN {
        return Err(Error::Malformed("ifinfomsg"));
    }
    let mut l = Link {
        link_type: ne_u16(payload, 2).unwrap_or(0),
        index: ne_u32(payload, 4).unwrap_or(0),
        flags: ne_u32(payload, 8).unwrap_or(0),
        ..Link::default()
    };
    for a in Attrs::new(&payload[IFINFOMSG_LEN..]) {
        match a.kind {
            IFLA_IFNAME => l.name = a.str(),
            IFLA_MTU => l.mtu = a.u32().unwrap_or(0),
            IFLA_ADDRESS => l.mac = a.data.to_vec(),
            IFLA_PERM_ADDRESS => l.perm_mac = a.data.to_vec(),
            IFLA_MASTER => l.master = a.u32().filter(|m| *m != 0),
            IFLA_LINK => l.parent = a.u32().filter(|p| *p != 0 && *p != l.index),
            IFLA_OPERSTATE => l.oper = a.u8().map(OperState::from_u8),
            IFLA_CARRIER => l.carrier = a.u8().map(|c| c != 0),
            IFLA_IFALIAS => l.alias = Some(a.str()),
            IFLA_STATS64 => l.stats = parse_stats(a.data),
            IFLA_LINKINFO => parse_linkinfo(&mut l, &a.attrs()),
            _ => {}
        }
    }
    Ok(l)
}

fn parse_linkinfo(l: &mut Link, info: &Attrs<'_>) {
    for a in info.clone() {
        match a.kind {
            IFLA_INFO_KIND => l.kind = Some(a.str()),
            IFLA_INFO_SLAVE_KIND => l.slave_kind = Some(a.str()),
            _ => {}
        }
    }
    let Some(data) = info.get(IFLA_INFO_DATA) else {
        return;
    };
    match l.kind.as_deref() {
        Some("vlan") => l.vlan_id = data.attrs().get(IFLA_VLAN_ID).and_then(|a| a.u16()),
        Some("bond") => {
            l.bond_mode = data
                .attrs()
                .get(IFLA_BOND_MODE)
                .and_then(|a| a.u8())
                .and_then(BondMode::from_u8);
        }
        _ => {}
    }
}

fn parse_stats(b: &[u8]) -> Option<LinkStats> {
    let f = |i: usize| ne_u64(b, i * 8);
    Some(LinkStats {
        rx_packets: f(0)?,
        tx_packets: f(1)?,
        rx_bytes: f(2)?,
        tx_bytes: f(3)?,
        rx_errors: f(4)?,
        tx_errors: f(5)?,
        rx_dropped: f(6)?,
        tx_dropped: f(7)?,
    })
}
