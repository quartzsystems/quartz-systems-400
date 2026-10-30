// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Routes as `QATOCRTE` (IPv4) and `QATOCRTE6` records.

use std::net::IpAddr;

use super::defs::{QATOCRTE, QATOCRTE6};
use super::layout::{Layout, Rec};
use crate::addr::{mask_prefix, prefix_mask, v4_bits};
use crate::model::rte::{Dest, Route, Routes};
use crate::model::{Tos, yes};

pub(super) fn encode(t: &Routes) -> Vec<(&'static Layout, Vec<Rec>)> {
    let mut v4 = Vec::new();
    let mut v6 = Vec::new();
    for r in &t.list {
        if r.dest.is_v6() {
            v6.push(encode6(r));
        } else {
            v4.push(encode4(r));
        }
    }
    vec![(&QATOCRTE, v4), (&QATOCRTE6, v6)]
}

fn bits(ip: IpAddr) -> i64 {
    match ip {
        IpAddr::V4(v) => v4_bits(v),
        IpAddr::V6(_) => 0,
    }
}

fn encode4(r: &Route) -> Rec {
    let mut x = QATOCRTE.blank();
    let mask = prefix_mask(r.prefix);
    x.set("RTEDEST", &r.dest.to_string())
        .set_bin("BINRTEDEST", bits(r.dest.ip()))
        .set("SUBNETMASK", &r.mask_text())
        .set_bin("BINSUBMASK", v4_bits(mask))
        .set("TOS", r.tos.name())
        .set("NEXTHOP", &r.next_hop.to_string())
        .set_bin("BINNEXTHOP", bits(r.next_hop))
        .set_bin("NETWORKID", bits(r.dest.ip()))
        .set_bin("MTU", i64::from(r.mtu.unwrap_or(0)))
        .set("RTETYPE", if r.dest == Dest::DftRoute { "D" } else { "S" })
        .set("BINDIFC", &r.bind_ifc.map_or("*NONE".into(), |b| b.to_string()))
        .set_bin("BINBINDIFC", r.bind_ifc.map_or(0, v4_bits))
        .set_bin("PRECEDENCE", i64::from(r.dup_pty))
        .set_bin("RIPMETRIC", i64::from(r.metric))
        .set("RIPREDIST", yes(r.redist))
        .set_g("TEXT", &r.text);
    x
}

fn encode6(r: &Route) -> Rec {
    let mut x = QATOCRTE6.blank();
    let (line, vlan) = r.bind_line.clone().unwrap_or_default();
    x.set("RTEDEST", &r.dest.to_string())
        .set_bin("PFXLEN", i64::from(r.prefix))
        .set("TOS", r.tos.name())
        .set("NEXTHOP", &r.next_hop.to_string())
        .set("BINDLIND", &line)
        .set_bin("BINDVLAN", i64::from(vlan.unwrap_or(0)))
        .set_bin("MTU", i64::from(r.mtu.unwrap_or(0)))
        .set_bin("METRIC", i64::from(r.metric))
        .set("REDST", yes(r.redist))
        .set_bin("DUPRTEPTY", i64::from(r.dup_pty))
        .set("TEXT", &r.text);
    x
}

pub(super) fn decode(v4: &[Rec], v6: &[Rec]) -> Routes {
    let mut list = Vec::new();
    for x in v4 {
        let (Ok(dest), Ok(hop)) = (Dest::parse(&x.get("RTEDEST")), x.get("NEXTHOP").parse::<IpAddr>()) else {
            continue;
        };
        let mask = x.get("SUBNETMASK");
        let prefix = if mask == "*NONE" {
            0
        } else {
            mask.parse().ok().and_then(mask_prefix).unwrap_or(32)
        };
        let mut r = Route::new(dest, prefix, hop);
        r.tos = Tos::parse(&x.get("TOS")).unwrap_or_default();
        r.mtu = u32::try_from(x.bin("MTU")).ok().filter(|m| *m != 0);
        r.bind_ifc = x.get("BINDIFC").parse().ok();
        r.dup_pty = u8::try_from(x.bin("PRECEDENCE")).ok().filter(|p| *p != 0).unwrap_or(5);
        r.metric = u8::try_from(x.bin("RIPMETRIC")).ok().filter(|p| *p != 0).unwrap_or(1);
        r.redist = x.get("RIPREDIST") == "*YES";
        r.text = x.get_g("TEXT");
        list.push(r);
    }
    for x in v6 {
        let (Ok(dest), Ok(hop)) = (Dest::parse(&x.get("RTEDEST")), x.get("NEXTHOP").parse::<IpAddr>()) else {
            continue;
        };
        let mut r = Route::new(dest, u8::try_from(x.bin("PFXLEN")).unwrap_or(0), hop);
        r.tos = Tos::parse(&x.get("TOS")).unwrap_or_default();
        let line = x.get("BINDLIND");
        if !line.is_empty() {
            r.bind_line = Some((line, u16::try_from(x.bin("BINDVLAN")).ok().filter(|v| *v != 0)));
        }
        r.mtu = u32::try_from(x.bin("MTU")).ok().filter(|m| *m != 0);
        r.metric = u8::try_from(x.bin("METRIC")).ok().filter(|p| *p != 0).unwrap_or(1);
        r.redist = x.get("REDST") == "*YES";
        r.dup_pty = u8::try_from(x.bin("DUPRTEPTY")).ok().filter(|p| *p != 0).unwrap_or(5);
        r.text = x.get("TEXT");
        list.push(r);
    }
    Routes { list }
}
