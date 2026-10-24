// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The neighbor cache (ARP and NDP): `ndmsg`.

use std::net::IpAddr;

#[allow(clippy::wildcard_imports)] // the numbers module
use super::consts::*;
use crate::codec::{Attrs, Builder, NLM_F_DUMP, NLM_F_REQUEST, ne_u16, ne_u32};
use crate::{AF_UNSPEC, Error, Family, Result};

/// One neighbor cache entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Neighbor {
    /// Interface index.
    pub index: u32,
    /// Protocol address.
    pub addr: IpAddr,
    /// Link-layer address, once resolved.
    pub lladdr: Option<Vec<u8>>,
    /// `NUD_*` state bits.
    pub state: u16,
    /// `NTF_*` flags.
    pub flags: u8,
}

impl Neighbor {
    /// The state as one word.
    pub fn state_name(&self) -> &'static str {
        match self.state {
            s if s & NUD_PERMANENT != 0 => "permanent",
            s if s & NUD_NOARP != 0 => "noarp",
            s if s & NUD_REACHABLE != 0 => "reachable",
            s if s & NUD_STALE != 0 => "stale",
            s if s & NUD_DELAY != 0 => "delay",
            s if s & NUD_PROBE != 0 => "probe",
            s if s & NUD_FAILED != 0 => "failed",
            s if s & NUD_INCOMPLETE != 0 => "incomplete",
            _ => "none",
        }
    }
}

/// Dump the cache of one family, or all.
pub fn dump_request(family: Option<Family>) -> Builder {
    let mut b = Builder::new(RTM_GETNEIGH, NLM_F_REQUEST | NLM_F_DUMP);
    let mut hdr = [0u8; NDMSG_LEN];
    hdr[0] = family.map_or(AF_UNSPEC, Family::af);
    b.bytes(&hdr);
    b
}

/// Parse an `RTM_NEWNEIGH` payload. `None` for non-IP entries.
pub fn parse(payload: &[u8]) -> Result<Option<Neighbor>> {
    if payload.len() < NDMSG_LEN {
        return Err(Error::Malformed("ndmsg"));
    }
    if Family::from_af(payload[0]).is_none() {
        return Ok(None);
    }
    let attrs = Attrs::new(&payload[NDMSG_LEN..]);
    let Some(addr) = attrs.get(NDA_DST).and_then(|a| a.ip()) else {
        return Ok(None);
    };
    Ok(Some(Neighbor {
        index: ne_u32(payload, 4).unwrap_or(0),
        addr,
        lladdr: attrs.get(NDA_LLADDR).map(|a| a.data.to_vec()),
        state: ne_u16(payload, 8).unwrap_or(0),
        flags: payload[10],
    }))
}
