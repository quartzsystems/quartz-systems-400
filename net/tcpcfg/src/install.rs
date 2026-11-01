// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An automatic install's network answers (`ip`, `gateway` in the
//! response file) as the installed system's first IPL applies them: the
//! `ADDTCPIFC` and `ADDTCPRTE *DFTROUTE` an operator would type.

use std::net::{IpAddr, Ipv4Addr};

use crate::addr::{IfcAddr, bad, parse_v4, prefix_mask};
use crate::files::Table;
use crate::model::LineRef;
use crate::model::ifc::Interface;
use crate::model::rte::{Dest, Route};
use crate::{Config, Result};

/// The CL an operator would type for `ip` (`a.b.c.d/n`) on `line` and
/// `gateway`.
pub fn commands(ip: &str, gateway: Option<&str>, line: &str) -> Result<Vec<String>> {
    let (addr, prefix) = split(ip)?;
    let mut v = vec![format!(
        "ADDTCPIFC INTNETADR('{addr}') LIND({}) SUBNETMASK('{}')",
        line.to_ascii_uppercase(),
        prefix_mask(prefix)
    )];
    if let Some(g) = gateway {
        let g = parse_v4(g)?;
        v.push(format!("ADDTCPRTE RTEDEST(*DFTROUTE) NEXTHOP('{g}')"));
    }
    Ok(v)
}

/// Add the interface and the default route to `cfg`, as those commands
/// do; an entry already there is left alone, so a second IPL changes
/// nothing. The tables changed.
pub fn apply(cfg: &mut Config, ip: &str, gateway: Option<&str>, line: &str) -> Result<Vec<Table>> {
    let (addr, prefix) = split(ip)?;
    let mut changed = Vec::new();
    let a = IfcAddr::Ip(IpAddr::V4(addr));
    if cfg.ifcs.get(&a).is_none() {
        let mut i = Interface::new(a, LineRef::Line(line.to_ascii_uppercase()));
        i.mask = Some(prefix_mask(prefix));
        cfg.ifcs.add(i)?;
        changed.push(Table::Ifc);
    }
    if let Some(g) = gateway {
        let r = Route::new(Dest::DftRoute, 0, IpAddr::V4(parse_v4(g)?));
        if cfg.rtes.get(&r.key()).is_none() {
            cfg.rtes.add(r)?;
            changed.push(Table::Rte);
        }
    }
    Ok(changed)
}

/// `a.b.c.d/n`.
fn split(ip: &str) -> Result<(Ipv4Addr, u8)> {
    let (a, n) = ip.trim().split_once('/').ok_or_else(|| bad("ip", ip))?;
    let prefix: u8 = n.parse().map_err(|_| bad("ip", ip))?;
    if prefix == 0 || prefix > 32 {
        return Err(bad("ip", ip));
    }
    Ok((parse_v4(a)?, prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_operators_commands() {
        assert_eq!(
            commands("10.0.2.15/24", Some("10.0.2.2"), "eth0").unwrap(),
            [
                "ADDTCPIFC INTNETADR('10.0.2.15') LIND(ETH0) SUBNETMASK('255.255.255.0')",
                "ADDTCPRTE RTEDEST(*DFTROUTE) NEXTHOP('10.0.2.2')",
            ]
        );
        assert_eq!(commands("192.168.1.9/16", None, "ETHLINE").unwrap().len(), 1);
        assert!(commands("10.0.2.15", None, "ETH0").is_err());
        assert!(commands("10.0.2.15/33", None, "ETH0").is_err());
    }

    #[test]
    fn interface_and_default_route_once() {
        let mut cfg = Config::default();
        let t = apply(&mut cfg, "10.0.2.15/24", Some("10.0.2.2"), "eth0").unwrap();
        assert_eq!(t, [Table::Ifc, Table::Rte]);
        let i = cfg.ifcs.get(&IfcAddr::Ip("10.0.2.15".parse().unwrap())).unwrap();
        assert_eq!(i.line, LineRef::Line("ETH0".into()));
        assert_eq!(i.mask, Some(Ipv4Addr::new(255, 255, 255, 0)));
        assert!(i.autostart);
        let r = cfg.rtes.list.iter().find(|r| r.dest == Dest::DftRoute).unwrap();
        assert_eq!(r.next_hop, IpAddr::V4(Ipv4Addr::new(10, 0, 2, 2)));
        // The next IPL finds them and adds nothing.
        assert!(apply(&mut cfg, "10.0.2.15/24", Some("10.0.2.2"), "eth0").unwrap().is_empty());
        // No gateway, no route.
        let mut cfg = Config::default();
        assert_eq!(apply(&mut cfg, "10.0.2.15/24", None, "ETH0").unwrap(), [Table::Ifc]);
    }
}
