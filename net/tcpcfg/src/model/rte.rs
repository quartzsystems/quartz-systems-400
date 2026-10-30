// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Routes: `ADDTCPRTE`, `CHGTCPRTE`, `RMVTCPRTE`. A route is found by
//! its destination, mask (or prefix length), type of service and next
//! hop, as the remove command names it.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use super::Tos;
use crate::addr::{bad, network, prefix_mask};
use crate::error::{Error, Result};
use crate::msg;

/// The most routes the table holds.
pub const MAX: usize = 8192;

/// `RTEDEST`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Dest {
    /// `*DFTROUTE`: the IPv4 default route.
    DftRoute,
    /// `*DFTMCAST`: the IPv4 multicast route (224.0.0.0/4).
    DftMcast,
    /// A network or host (`::` is `*DFT6ROUTE`, `FF00::` `*DFT6MCAST`).
    Ip(IpAddr),
}

impl Dest {
    /// From the command's text.
    pub fn parse(s: &str) -> Result<Self> {
        let t = crate::addr::unquote(s);
        Ok(match t.to_ascii_uppercase().as_str() {
            "*DFTROUTE" => Self::DftRoute,
            "*DFTMCAST" => Self::DftMcast,
            "*DFT6ROUTE" => Self::Ip(IpAddr::V6(Ipv6Addr::UNSPECIFIED)),
            "*DFT6MCAST" => Self::Ip(IpAddr::V6(Ipv6Addr::new(0xff00, 0, 0, 0, 0, 0, 0, 0))),
            _ => Self::Ip(crate::addr::parse_ip(t).map_err(|_| bad("RTEDEST", t))?),
        })
    }

    /// The destination address.
    pub fn ip(self) -> IpAddr {
        match self {
            Self::DftRoute => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            Self::DftMcast => IpAddr::V4(Ipv4Addr::new(224, 0, 0, 0)),
            Self::Ip(ip) => ip,
        }
    }

    /// IPv6.
    pub fn is_v6(self) -> bool {
        self.ip().is_ipv6()
    }
}

impl fmt::Display for Dest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DftRoute => f.write_str("*DFTROUTE"),
            Self::DftMcast => f.write_str("*DFTMCAST"),
            Self::Ip(ip) => write!(f, "{ip}"),
        }
    }
}

/// One configured route.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Route {
    /// `RTEDEST`.
    pub dest: Dest,
    /// The prefix length: `SUBNETMASK` for IPv4, `ADRPFXLEN` for IPv6.
    pub prefix: u8,
    /// `TOS`.
    pub tos: Tos,
    /// `NEXTHOP`.
    pub next_hop: IpAddr,
    /// `BINDIFC`: the preferred binding interface.
    pub bind_ifc: Option<Ipv4Addr>,
    /// `BINDLIND`: line and VLAN (IPv6).
    pub bind_line: Option<(String, Option<u16>)>,
    /// `MTU`; `None` is `*IFC`.
    pub mtu: Option<u32>,
    /// `METRIC`.
    pub metric: u8,
    /// `REDST`: route redistribution.
    pub redist: bool,
    /// `DUPRTEPTY`.
    pub dup_pty: u8,
    /// `TEXT`.
    pub text: String,
}

/// What finds a route.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RouteKey {
    /// Destination.
    pub dest: Dest,
    /// Prefix length.
    pub prefix: u8,
    /// Type of service.
    pub tos: Tos,
    /// Next hop.
    pub next_hop: IpAddr,
}

impl fmt::Display for RouteKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} /{} {} via {}", self.dest, self.prefix, self.tos, self.next_hop)
    }
}

impl Route {
    /// A route with the definition's defaults.
    pub fn new(dest: Dest, prefix: u8, next_hop: IpAddr) -> Self {
        Self {
            dest,
            prefix,
            tos: Tos::Normal,
            next_hop,
            bind_ifc: None,
            bind_line: None,
            mtu: None,
            metric: 1,
            redist: false,
            dup_pty: 5,
            text: String::new(),
        }
    }

    /// Its key.
    pub fn key(&self) -> RouteKey {
        RouteKey {
            dest: self.dest,
            prefix: self.prefix,
            tos: self.tos,
            next_hop: self.next_hop,
        }
    }

    /// The `SUBNETMASK` column: `*NONE` for a default route.
    pub fn mask_text(&self) -> String {
        if self.dest.is_v6() {
            return self.prefix.to_string();
        }
        match self.dest {
            Dest::DftRoute => "*NONE".into(),
            _ => prefix_mask(self.prefix).to_string(),
        }
    }

    /// Check the values that do not depend on other entries.
    pub fn validate(&self) -> Result<()> {
        if self.dest.is_v6() != self.next_hop.is_ipv6() {
            return Err(bad("NEXTHOP", &self.next_hop.to_string()));
        }
        let max = if self.dest.is_v6() { 128 } else { 32 };
        if self.prefix > max {
            return Err(bad("SUBNETMASK", &self.prefix.to_string()));
        }
        match self.dest {
            Dest::DftRoute if self.prefix != 0 => return Err(bad("SUBNETMASK", &self.mask_text())),
            Dest::Ip(ip) if !ip.is_unspecified() && self.prefix == 0 => {
                return Err(Error::new(
                    msg::BAD_VALUE,
                    "SUBNETMASK is needed for a route to a network or host.",
                ));
            }
            Dest::Ip(ip) if network(ip, self.prefix) != ip => {
                return Err(bad("RTEDEST", &ip.to_string()));
            }
            _ => {}
        }
        if self.next_hop.is_unspecified() || self.next_hop.is_multicast() {
            return Err(bad("NEXTHOP", &self.next_hop.to_string()));
        }
        if let Some(m) = self.mtu
            && !(576..=16388).contains(&m)
        {
            return Err(bad("MTU", &m.to_string()));
        }
        if !(1..=16).contains(&self.metric) {
            return Err(bad("METRIC", &self.metric.to_string()));
        }
        if !(1..=10).contains(&self.dup_pty) {
            return Err(bad("DUPRTEPTY", &self.dup_pty.to_string()));
        }
        Ok(())
    }
}

/// The route table, in the order entries were added.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Routes {
    /// The entries.
    pub list: Vec<Route>,
}

impl Routes {
    /// The entry with this key.
    pub fn get(&self, key: &RouteKey) -> Option<&Route> {
        self.list.iter().find(|r| &r.key() == key)
    }

    /// `ADDTCPRTE`.
    pub fn add(&mut self, r: Route) -> Result<()> {
        r.validate()?;
        if self.get(&r.key()).is_some() {
            return Err(Error::new(
                msg::EXISTS,
                format!("A route to {} through {} already exists.", r.dest, r.next_hop),
            ));
        }
        if self.list.len() >= MAX {
            return Err(Error::new(msg::FULL, "The route table is full."));
        }
        self.list.push(r);
        Ok(())
    }

    /// `CHGTCPRTE`: replace the entry with the same key.
    pub fn change(&mut self, r: Route) -> Result<()> {
        r.validate()?;
        let key = r.key();
        let slot = self
            .list
            .iter_mut()
            .find(|x| x.key() == key)
            .ok_or_else(|| not_found(&key))?;
        *slot = r;
        Ok(())
    }

    /// `RMVTCPRTE`: the entry removed.
    pub fn remove(&mut self, key: &RouteKey) -> Result<Route> {
        let at = self
            .list
            .iter()
            .position(|r| &r.key() == key)
            .ok_or_else(|| not_found(key))?;
        Ok(self.list.remove(at))
    }

    /// The entries in the panels' order: defaults first, then by
    /// destination.
    pub fn sorted(&self) -> Vec<&Route> {
        let mut v: Vec<&Route> = self.list.iter().collect();
        v.sort_by_key(|r| (r.dest.is_v6(), r.dest != Dest::DftRoute, r.dest.ip(), r.prefix));
        v
    }
}

fn not_found(key: &RouteKey) -> Error {
    Error::new(
        msg::NOT_FOUND,
        format!("No route to {} through {} is configured.", key.dest, key.next_hop),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes() {
        let mut t = Routes::default();
        let gw: IpAddr = "10.0.2.2".parse().unwrap();
        t.add(Route::new(Dest::DftRoute, 0, gw)).unwrap();
        assert_eq!(t.add(Route::new(Dest::DftRoute, 0, gw)).unwrap_err().msgid, msg::EXISTS);
        let net = Dest::parse("192.168.5.0").unwrap();
        assert!(t.add(Route::new(net, 0, gw)).is_err(), "a network needs a mask");
        assert!(t.add(Route::new(Dest::parse("192.168.5.1").unwrap(), 24, gw)).is_err());
        t.add(Route::new(net, 24, gw)).unwrap();
        assert_eq!(t.sorted()[0].dest, Dest::DftRoute);
        assert_eq!(t.list[0].mask_text(), "*NONE");
        assert_eq!(t.list[1].mask_text(), "255.255.255.0");
        let mut c = Route::new(net, 24, gw);
        c.mtu = Some(1400);
        t.change(c).unwrap();
        assert_eq!(t.list[1].mtu, Some(1400));
        t.remove(&Route::new(net, 24, gw).key()).unwrap();
        assert!(t.remove(&Route::new(net, 24, gw).key()).is_err());
        assert!(Route::new(Dest::DftRoute, 0, "::1".parse().unwrap()).validate().is_err());
    }
}
