// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Interfaces as `QATOCIFC` (IPv4, `*IP4DHCP`) and `QATOCIFC6` records.

use std::net::{IpAddr, Ipv4Addr};

use super::defs::{QATOCIFC, QATOCIFC6};
use super::layout::{Layout, Rec};
use crate::addr::{IfcAddr, mask_prefix, v4_bits};
use crate::model::ifc::{Interface, Interfaces};
use crate::model::{LineRef, Tos, yes};

/// Where `RESERVED2` keeps what `QIFC` has no field for.
const ALIAS_AT: (usize, usize) = (0, 25);
const PREF_AT: usize = 25;
const PREF_LEN: usize = 15;

pub(super) fn encode(t: &Interfaces) -> Vec<(&'static Layout, Vec<Rec>)> {
    let mut v4 = Vec::new();
    let mut v6 = Vec::new();
    for i in &t.list {
        if i.addr.is_v6() {
            v6.push(encode6(i));
        } else {
            v4.push(encode4(i));
        }
    }
    vec![(&QATOCIFC, v4), (&QATOCIFC6, v6)]
}

fn encode4(i: &Interface) -> Rec {
    let mut r = QATOCIFC.blank();
    let ip = match i.addr {
        IfcAddr::Ip(IpAddr::V4(v)) => Some(v),
        _ => None,
    };
    r.set("INTERNET", &i.addr.to_string())
        .set_bin("BININTADR", ip.map_or(0, v4_bits))
        .set("LIND", i.line.name())
        .set("TOS", i.tos.name())
        .set_bin("MTU", i64::from(i.mtu.unwrap_or(0)))
        .set("AUTOSTART", if i.autostart { "Y" } else { "N" })
        .set("LINETYPE", i.line_type())
        .set("LOCALIFC", &i.local_ifc.map_or("*NONE".into(), |l| l.to_string()))
        .set_bin("BINLOCAL", i.local_ifc.map_or(0, v4_bits))
        .set("IFCNAME", i.alias.as_deref().unwrap_or("*NONE"))
        .set_g("TEXT", &i.text)
        .set("DHCPFLG", if i.addr == IfcAddr::Dhcp4 { "Y" } else { "N" })
        .set("DHCPDYNDNS", yes(i.dhcp_dyndns))
        .set("DHCPCLTID", &i.dhcp_client_id)
        .set_bin("VRTLANID", i64::from(i.vlan.unwrap_or(0)))
        .set_at("RESERVED2", ALIAS_AT.0, ALIAS_AT.1, i.alias.as_deref().unwrap_or(""));
    if let Some(m) = i.mask {
        r.set("SUBNETMASK", &m.to_string()).set_bin("BINSUBNET", v4_bits(m));
        if let Some(ip) = ip {
            r.set_bin("NETWORKID", v4_bits(Ipv4Addr::from(u32::from(ip) & u32::from(m))));
        }
    } else {
        r.set("SUBNETMASK", "*");
    }
    for (n, p) in i.pref_ifc.iter().take(10).enumerate() {
        r.set_at("RESERVED2", PREF_AT + n * PREF_LEN, PREF_LEN, p);
    }
    r
}

fn encode6(i: &Interface) -> Rec {
    let mut r = QATOCIFC6.blank();
    r.set("INTERNET", &i.addr.to_string())
        .set("LIND", i.line.name())
        .set_bin("VRTLANID", i64::from(i.vlan.unwrap_or(0)))
        .set_bin("PFXLEN", i64::from(i.prefix_len.unwrap_or(0)))
        .set("AUTOSTART", if i.autostart { "Y" } else { "N" })
        .set_bin("MTU", i64::from(i.mtu.unwrap_or(0)))
        .set("ALIASNAME", i.alias.as_deref().unwrap_or(""))
        .set("TOS", i.tos.name())
        .set_bin("DADMAXTRN", i64::from(i.dad_max))
        .set("PVYEXN", if i.privacy { "Y" } else { "N" })
        .set("IP6GENGLBA", if i.gen_global { "Y" } else { "N" })
        .set("TEXT", &i.text);
    r
}

pub(super) fn decode(v4: &[Rec], v6: &[Rec]) -> Interfaces {
    let mut list = Vec::new();
    for r in v4 {
        let Ok(addr) = IfcAddr::parse(&r.get("INTERNET")) else {
            continue;
        };
        let Ok(line) = LineRef::parse(&r.get("LIND")) else {
            continue;
        };
        let mut i = Interface::new(addr, line);
        i.mask = r.get("SUBNETMASK").parse().ok();
        i.tos = Tos::parse(&r.get("TOS")).unwrap_or_default();
        i.mtu = u32::try_from(r.bin("MTU")).ok().filter(|m| *m != 0);
        i.autostart = r.get("AUTOSTART") != "N";
        i.local_ifc = r.get("LOCALIFC").parse().ok();
        let alias = r.get_at("RESERVED2", ALIAS_AT.0, ALIAS_AT.1);
        let short = r.get("IFCNAME");
        i.alias = if !alias.is_empty() {
            Some(alias)
        } else if !short.is_empty() && short != "*NONE" {
            Some(short)
        } else {
            None
        };
        i.text = r.get_g("TEXT");
        i.dhcp_dyndns = r.get("DHCPDYNDNS") == "*YES";
        let cltid = r.get("DHCPCLTID");
        if !cltid.is_empty() {
            i.dhcp_client_id = cltid;
        }
        i.vlan = u16::try_from(r.bin("VRTLANID")).ok().filter(|v| *v != 0);
        i.pref_ifc = (0..10)
            .map(|n| r.get_at("RESERVED2", PREF_AT + n * PREF_LEN, PREF_LEN))
            .filter(|p| !p.is_empty())
            .collect();
        let _ = i.mask.and_then(mask_prefix);
        list.push(i);
    }
    for r in v6 {
        let Ok(addr) = IfcAddr::parse(&r.get("INTERNET")) else {
            continue;
        };
        let Ok(line) = LineRef::parse(&r.get("LIND")) else {
            continue;
        };
        let mut i = Interface::new(addr, line);
        i.vlan = u16::try_from(r.bin("VRTLANID")).ok().filter(|v| *v != 0);
        i.prefix_len = u8::try_from(r.bin("PFXLEN")).ok().filter(|p| *p != 0);
        i.autostart = r.get("AUTOSTART") != "N";
        i.mtu = u32::try_from(r.bin("MTU")).ok().filter(|m| *m != 0);
        let alias = r.get("ALIASNAME");
        i.alias = (!alias.is_empty()).then_some(alias);
        i.tos = Tos::parse(&r.get("TOS")).unwrap_or_default();
        i.dad_max = u8::try_from(r.bin("DADMAXTRN")).unwrap_or(1);
        i.privacy = r.get("PVYEXN") == "Y";
        i.gen_global = r.get("IP6GENGLBA") != "N";
        i.text = r.get("TEXT");
        list.push(i);
    }
    Interfaces { list }
}
