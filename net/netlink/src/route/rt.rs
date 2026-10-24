// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Routes: `rtmsg`.

use std::net::IpAddr;

#[allow(clippy::wildcard_imports)] // the numbers module
use super::consts::*;
use crate::codec::{Attrs, Builder, NLM_F_CREATE, NLM_F_DUMP, NLM_F_EXCL, NLM_F_REQUEST, ne_u32};
use crate::{AF_UNSPEC, Error, Family, Result};

/// One route.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Route {
    /// Family.
    pub family: Family,
    /// Destination; `None` is the default route.
    pub dst: Option<IpAddr>,
    /// Destination prefix length.
    pub dst_len: u8,
    /// Next hop.
    pub gateway: Option<IpAddr>,
    /// Output interface index.
    pub oif: Option<u32>,
    /// Preferred source address.
    pub prefsrc: Option<IpAddr>,
    /// Metric (`RTA_PRIORITY`).
    pub metric: Option<u32>,
    /// Table (254 main).
    pub table: u32,
    /// Path MTU (`RTAX_MTU`).
    pub mtu: Option<u32>,
    /// Type of service.
    pub tos: u8,
    /// `RTPROT_*`: who installed it.
    pub protocol: u8,
    /// `RT_SCOPE_*`.
    pub scope: u8,
    /// `RTN_*`.
    pub kind: u8,
}

impl Route {
    /// A unicast route to `dst/dst_len` in the main table, installed by
    /// QS/400 ([`RTPROT_QS400`]).
    pub fn new(family: Family, dst: Option<IpAddr>, dst_len: u8) -> Self {
        Self {
            family,
            dst,
            dst_len,
            gateway: None,
            oif: None,
            prefsrc: None,
            metric: None,
            table: RT_TABLE_MAIN,
            mtu: None,
            tos: 0,
            protocol: RTPROT_QS400,
            scope: RT_SCOPE_UNIVERSE,
            kind: RTN_UNICAST,
        }
    }

    /// The default route.
    pub fn is_default(&self) -> bool {
        self.dst_len == 0
    }
}

fn rtmsg(r: &Route) -> [u8; RTMSG_LEN] {
    let mut b = [0u8; RTMSG_LEN];
    b[0] = r.family.af();
    b[1] = r.dst_len;
    b[3] = r.tos;
    b[4] = u8::try_from(r.table).unwrap_or(0);
    b[5] = r.protocol;
    b[6] = r.scope;
    b[7] = r.kind;
    b
}

fn ip_bytes(ip: IpAddr) -> Vec<u8> {
    match ip {
        IpAddr::V4(a) => a.octets().to_vec(),
        IpAddr::V6(a) => a.octets().to_vec(),
    }
}

/// Dump routes of one family, or all.
pub fn dump_request(family: Option<Family>) -> Builder {
    let mut b = Builder::new(RTM_GETROUTE, NLM_F_REQUEST | NLM_F_DUMP);
    let mut hdr = [0u8; RTMSG_LEN];
    hdr[0] = family.map_or(AF_UNSPEC, Family::af);
    b.bytes(&hdr);
    b
}

/// Add (`add`) or delete a route. A route without a gateway is
/// link-scoped unless the caller set a scope.
pub fn request(r: &Route, add: bool) -> Result<Builder> {
    let max = if r.family == Family::V4 { 32 } else { 128 };
    if r.dst_len > max {
        return Err(Error::Invalid(format!("prefix length {}", r.dst_len)));
    }
    for ip in [r.dst, r.gateway, r.prefsrc].into_iter().flatten() {
        if Family::of(ip) != r.family {
            return Err(Error::Invalid(format!("{ip} is not in the route's family")));
        }
    }
    let mut b = if add {
        Builder::new(RTM_NEWROUTE, NLM_F_REQUEST | NLM_F_CREATE | NLM_F_EXCL)
    } else {
        Builder::new(RTM_DELROUTE, NLM_F_REQUEST)
    };
    let mut r2 = r.clone();
    if add && r.gateway.is_none() && r.scope == RT_SCOPE_UNIVERSE && r.oif.is_some() {
        r2.scope = RT_SCOPE_LINK;
    }
    if !add {
        // Deletes match on what is given; leave scope and protocol open.
        r2.scope = RT_SCOPE_NOWHERE;
        r2.protocol = 0;
    }
    b.bytes(&rtmsg(&r2));
    if let Some(d) = r.dst {
        b.attr(RTA_DST, &ip_bytes(d));
    }
    if let Some(g) = r.gateway {
        b.attr(RTA_GATEWAY, &ip_bytes(g));
    }
    if let Some(o) = r.oif {
        b.u32(RTA_OIF, o);
    }
    if let Some(s) = r.prefsrc {
        b.attr(RTA_PREFSRC, &ip_bytes(s));
    }
    if let Some(m) = r.metric {
        b.u32(RTA_PRIORITY, m);
    }
    if r.table > 255 {
        b.u32(RTA_TABLE, r.table);
    }
    if let (true, Some(mtu)) = (add, r.mtu) {
        b.nest(RTA_METRICS, |b| {
            b.u32(RTAX_MTU, mtu);
        });
    }
    Ok(b)
}

/// `RT_SCOPE_NOWHERE`: on delete, any scope.
const RT_SCOPE_NOWHERE: u8 = 255;

/// Parse an `RTM_NEWROUTE` payload. `None` for families other than IP.
pub fn parse(payload: &[u8]) -> Result<Option<Route>> {
    if payload.len() < RTMSG_LEN {
        return Err(Error::Malformed("rtmsg"));
    }
    let Some(family) = Family::from_af(payload[0]) else {
        return Ok(None);
    };
    let attrs = Attrs::new(&payload[RTMSG_LEN..]);
    Ok(Some(Route {
        family,
        dst: attrs.get(RTA_DST).and_then(|a| a.ip()),
        dst_len: payload[1],
        gateway: attrs.get(RTA_GATEWAY).and_then(|a| a.ip()),
        oif: attrs.get(RTA_OIF).and_then(|a| a.u32()),
        prefsrc: attrs.get(RTA_PREFSRC).and_then(|a| a.ip()),
        metric: attrs.get(RTA_PRIORITY).and_then(|a| a.u32()),
        table: attrs
            .get(RTA_TABLE)
            .and_then(|a| a.u32())
            .unwrap_or(u32::from(payload[4])),
        mtu: attrs
            .get(RTA_METRICS)
            .and_then(|m| m.attrs().get(RTAX_MTU))
            .and_then(|a| ne_u32(a.data, 0)),
        tos: payload[3],
        protocol: payload[5],
        scope: payload[6],
        kind: payload[7],
    }))
}
