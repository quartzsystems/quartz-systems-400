// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Interface addresses: `ifaddrmsg`.

use std::net::{IpAddr, Ipv4Addr};

#[allow(clippy::wildcard_imports)] // the numbers module
use super::consts::*;
use crate::codec::{Attrs, Builder, NLM_F_CREATE, NLM_F_DUMP, NLM_F_EXCL, NLM_F_REQUEST, ne_u32};
use crate::{AF_UNSPEC, Error, Family, Result};

/// Lifetime value meaning forever.
pub const INFINITY_LIFE_TIME: u32 = u32::MAX;

/// One interface address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Address {
    /// Interface index.
    pub index: u32,
    /// The address (`IFA_LOCAL` for IPv4, `IFA_ADDRESS` for IPv6).
    pub addr: IpAddr,
    /// Prefix length.
    pub prefix: u8,
    /// Peer address of a point-to-point link, if different.
    pub peer: Option<IpAddr>,
    /// IPv4 broadcast address.
    pub broadcast: Option<Ipv4Addr>,
    /// Label (IPv4 aliases such as `eth0:1`).
    pub label: Option<String>,
    /// `RT_SCOPE_*`.
    pub scope: u8,
    /// `IFA_F_*` flags.
    pub flags: u32,
    /// Preferred lifetime in seconds ([`INFINITY_LIFE_TIME`] = forever).
    pub preferred: Option<u32>,
    /// Valid lifetime in seconds.
    pub valid: Option<u32>,
}

impl Address {
    /// `addr/prefix` on `index`, global scope, no flags.
    pub fn new(index: u32, addr: IpAddr, prefix: u8) -> Self {
        Self {
            index,
            addr,
            prefix,
            peer: None,
            broadcast: None,
            label: None,
            scope: RT_SCOPE_UNIVERSE,
            flags: 0,
            preferred: None,
            valid: None,
        }
    }

    /// The address family.
    pub fn family(&self) -> Family {
        Family::of(self.addr)
    }

    /// The IPv4 broadcast address of `addr/prefix`.
    pub fn default_broadcast(addr: Ipv4Addr, prefix: u8) -> Ipv4Addr {
        let host = if prefix >= 32 {
            0
        } else {
            u32::MAX >> prefix
        };
        Ipv4Addr::from(u32::from(addr) | host)
    }
}

fn ifaddrmsg(af: u8, prefix: u8, scope: u8, index: u32) -> [u8; IFADDRMSG_LEN] {
    let mut b = [0u8; IFADDRMSG_LEN];
    b[0] = af;
    b[1] = prefix;
    b[3] = scope;
    b[4..8].copy_from_slice(&index.to_ne_bytes());
    b
}

fn ip_bytes(ip: IpAddr) -> Vec<u8> {
    match ip {
        IpAddr::V4(a) => a.octets().to_vec(),
        IpAddr::V6(a) => a.octets().to_vec(),
    }
}

/// Dump addresses of one family, or all.
pub fn dump_request(family: Option<Family>) -> Builder {
    let mut b = Builder::new(RTM_GETADDR, NLM_F_REQUEST | NLM_F_DUMP);
    b.bytes(&ifaddrmsg(family.map_or(AF_UNSPEC, Family::af), 0, 0, 0));
    b
}

/// Add (`add`) or delete an address.
pub fn request(a: &Address, add: bool) -> Result<Builder> {
    let max = if a.addr.is_ipv4() { 32 } else { 128 };
    if a.prefix > max {
        return Err(Error::Invalid(format!("prefix length {}", a.prefix)));
    }
    let mut b = if add {
        Builder::new(RTM_NEWADDR, NLM_F_REQUEST | NLM_F_CREATE | NLM_F_EXCL)
    } else {
        Builder::new(RTM_DELADDR, NLM_F_REQUEST)
    };
    let low = u8::try_from(a.flags & 0xff).unwrap_or(0);
    let mut hdr = ifaddrmsg(a.family().af(), a.prefix, a.scope, a.index);
    hdr[2] = low;
    b.bytes(&hdr);
    let addr = ip_bytes(a.addr);
    match a.addr {
        IpAddr::V4(v4) => {
            b.attr(IFA_LOCAL, &addr);
            b.attr(IFA_ADDRESS, &a.peer.map_or(addr.clone(), ip_bytes));
            if add {
                let bc = a
                    .broadcast
                    .unwrap_or_else(|| Address::default_broadcast(v4, a.prefix));
                if a.prefix < 31 {
                    b.attr(IFA_BROADCAST, &bc.octets());
                }
            }
        }
        IpAddr::V6(_) => {
            b.attr(IFA_ADDRESS, &addr);
        }
    }
    if let Some(l) = &a.label {
        b.str(IFA_LABEL, l);
    }
    if add {
        b.u32(IFA_FLAGS, a.flags);
        if a.preferred.is_some() || a.valid.is_some() {
            let mut ci = [0u8; 16];
            ci[0..4].copy_from_slice(&a.preferred.unwrap_or(INFINITY_LIFE_TIME).to_ne_bytes());
            ci[4..8].copy_from_slice(&a.valid.unwrap_or(INFINITY_LIFE_TIME).to_ne_bytes());
            b.attr(IFA_CACHEINFO, &ci);
        }
    }
    Ok(b)
}

/// Parse an `RTM_NEWADDR` payload. `None` for families other than IP.
pub fn parse(payload: &[u8]) -> Result<Option<Address>> {
    if payload.len() < IFADDRMSG_LEN {
        return Err(Error::Malformed("ifaddrmsg"));
    }
    if Family::from_af(payload[0]).is_none() {
        return Ok(None);
    }
    let attrs = Attrs::new(&payload[IFADDRMSG_LEN..]);
    let address = attrs.get(IFA_ADDRESS).and_then(|a| a.ip());
    let local = attrs.get(IFA_LOCAL).and_then(|a| a.ip());
    let Some(addr) = local.or(address) else {
        return Ok(None);
    };
    let ci = attrs.get(IFA_CACHEINFO);
    Ok(Some(Address {
        index: ne_u32(payload, 4).unwrap_or(0),
        addr,
        prefix: payload[1],
        peer: address.filter(|p| local.is_some() && *p != addr),
        broadcast: attrs.get(IFA_BROADCAST).and_then(|a| match a.ip() {
            Some(IpAddr::V4(v)) => Some(v),
            _ => None,
        }),
        label: attrs.get(IFA_LABEL).map(|a| a.str()),
        scope: payload[3],
        flags: attrs
            .get(IFA_FLAGS)
            .and_then(|a| a.u32())
            .unwrap_or(u32::from(payload[2])),
        preferred: ci.and_then(|a| ne_u32(a.data, 0)),
        valid: ci.and_then(|a| ne_u32(a.data, 4)),
    }))
}
