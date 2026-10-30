// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The host table, attributes and domain, port restrictions, servers
//! and the related tables as records.

use std::net::IpAddr;

use super::defs::{
    QATOCHOST, QATOCHST6, QATOCPN, QATOCPORT, QATOCPP, QATOCPS, QATOCSTART, QATOCSVRA, QATOCTCPIP,
};
use super::layout::{Layout, Rec};
use crate::addr::v4_bits;
use crate::model::attr::{Attributes, SPEC};
use crate::model::dmn::{Domain, InitialServer, SearchPriority};
use crate::model::host::{HostEntry, HostTable};
use crate::model::port::{PortRestriction, PortRestrictions};
use crate::model::svr::{Servers, UserServer};
use crate::model::tables::{NetworkEntry, ProtocolEntry, ServiceEntry, ServicesTable};
use crate::model::{Proto, yes};

const NAME_FIELDS: [&str; 4] = ["HOSTNME1", "HOSTNME2", "HOSTNME3", "HOSTNME4"];
const ALIAS_FIELDS: [&str; 4] = ["ALIAS1", "ALIAS2", "ALIAS3", "ALIAS4"];

pub(super) fn encode_hosts(t: &HostTable) -> Vec<(&'static Layout, Vec<Rec>)> {
    let mut v4 = Vec::new();
    let mut v6 = Vec::new();
    for e in &t.list {
        let layout = if e.addr.is_ipv4() { &QATOCHOST } else { &QATOCHST6 };
        for (n, chunk) in e.names.chunks(4).enumerate() {
            let mut r = layout.blank();
            r.set("INTERNET", &e.addr.to_string());
            for (f, name) in NAME_FIELDS.iter().zip(chunk) {
                r.set(f, name);
            }
            if n == 0 {
                r.set("TXTDESC", &e.text);
            }
            if let IpAddr::V4(v) = e.addr {
                r.set_bin("IPINTGER", v4_bits(v));
                v4.push(r);
            } else {
                v6.push(r);
            }
        }
    }
    vec![(&QATOCHOST, v4), (&QATOCHST6, v6)]
}

pub(super) fn decode_hosts(v4: &[Rec], v6: &[Rec]) -> HostTable {
    let mut list: Vec<HostEntry> = Vec::new();
    for r in v4.iter().chain(v6) {
        let Ok(addr) = r.get("INTERNET").parse::<IpAddr>() else {
            continue;
        };
        let names: Vec<String> = NAME_FIELDS
            .iter()
            .map(|f| r.get(f).to_ascii_uppercase())
            .filter(|n| !n.is_empty())
            .collect();
        let text = r.get("TXTDESC");
        if let Some(e) = list.iter_mut().find(|e| e.addr == addr) {
            for n in names {
                if !e.names.contains(&n) {
                    e.names.push(n);
                }
            }
            if e.text.is_empty() {
                e.text = text;
            }
        } else if !names.is_empty() {
            list.push(HostEntry { addr, names, text });
        }
    }
    HostTable { list }
}

/// The domain's keywords in `QATOCTCPIP`.
const DMN_KEYS: [&str; 10] = [
    "HOSTNAME",
    "DMNNAME",
    "DMNSCHLIST",
    "HOSTSCHPTY",
    "INTNETADR",
    "PORT",
    "PROTOCOL",
    "INLDMNSVR",
    "DMNSVRRTY",
    "DNSSEC",
];

pub(super) fn encode_tcpip(a: &Attributes, d: &Domain) -> Vec<(&'static Layout, Vec<Rec>)> {
    let mut out = Vec::new();
    let mut put = |k: &str, v: &str| {
        let mut r = QATOCTCPIP.blank();
        r.set("KEYWORD", k).set("DATA", v);
        out.push(r);
    };
    for s in SPEC {
        put(s.kwd, &a.get(s.kwd));
    }
    let dns: Vec<String> = d.dns.iter().map(ToString::to_string).collect();
    put("HOSTNAME", &d.host_name);
    put("DMNNAME", &d.domain_name);
    put("DMNSCHLIST", &d.search_list.join(" "));
    put("HOSTSCHPTY", d.priority.name());
    put("INTNETADR", &dns.join(" "));
    put("PORT", &d.port.to_string());
    put("PROTOCOL", d.protocol.name());
    put("INLDMNSVR", d.initial.name());
    put("DMNSVRRTY", &format!("{} {}", d.retry_attempts, d.retry_interval));
    put("DNSSEC", yes(d.dnssec));
    vec![(&QATOCTCPIP, out)]
}

pub(super) fn decode_tcpip(recs: &[Rec]) -> (Attributes, Domain) {
    let mut attrs = Attributes::default();
    let mut d = Domain::default();
    for rec in recs {
        let k = rec.get("KEYWORD");
        let v = rec.get("DATA");
        if DMN_KEYS.contains(&k.as_str()) {
            let words = || v.split_whitespace().map(str::to_string);
            match k.as_str() {
                "HOSTNAME" => d.host_name.clone_from(&v),
                "DMNNAME" => d.domain_name.clone_from(&v),
                "DMNSCHLIST" => d.search_list = words().collect(),
                "HOSTSCHPTY" => {
                    d.priority = if v == "*REMOTE" { SearchPriority::Remote } else { SearchPriority::Local };
                }
                "INTNETADR" => d.dns = v.split_whitespace().filter_map(|x| x.parse().ok()).collect(),
                "PORT" => d.port = v.parse().unwrap_or(53),
                "PROTOCOL" => d.protocol = Proto::parse(&v).unwrap_or(Proto::Udp),
                "INLDMNSVR" => {
                    d.initial = if v == "*ROTATE" { InitialServer::Rotate } else { InitialServer::First };
                }
                "DMNSVRRTY" => {
                    let n: Vec<u8> = v.split_whitespace().filter_map(|x| x.parse().ok()).collect();
                    if n.len() == 2 {
                        d.retry_attempts = n[0];
                        d.retry_interval = n[1];
                    }
                }
                _ => d.dnssec = v == "*YES",
            }
        } else if attrs.get(&k) != v {
            let _ = attrs.set(&k, &v);
        }
    }
    (attrs, d)
}

pub(super) fn encode_ports(t: &PortRestrictions) -> Vec<(&'static Layout, Vec<Rec>)> {
    let recs = t
        .list
        .iter()
        .map(|p| {
            let mut r = QATOCPORT.blank();
            r.set_bin("LOWERPORT", i64::from(p.lower))
                .set_bin("UPPERPORT", p.upper.map_or(-1, i64::from))
                .set("PROTOCOL", p.proto.name())
                .set("USERPROF", &p.user);
            r
        })
        .collect();
    vec![(&QATOCPORT, recs)]
}

pub(super) fn decode_ports(recs: &[Rec]) -> PortRestrictions {
    let list = recs
        .iter()
        .filter_map(|r| {
            Some(PortRestriction {
                lower: u16::try_from(r.bin("LOWERPORT")).ok()?,
                upper: u16::try_from(r.bin("UPPERPORT")).ok(),
                proto: Proto::parse(&r.get("PROTOCOL")).ok()?,
                user: r.get("USERPROF"),
            })
        })
        .collect();
    PortRestrictions { list }
}

pub(super) fn encode_servers(s: &Servers) -> Vec<(&'static Layout, Vec<Rec>)> {
    let mut start = Vec::new();
    let mut bag = Vec::new();
    for a in &s.attrs {
        let mut r = QATOCSTART.blank();
        r.set("SERVERTYPE", "S").set("SERVER", &a.server).set("AUTOSTART", yes(a.autostart));
        start.push(r);
        for (k, v) in &a.bag {
            let mut b = QATOCSVRA.blank();
            b.set("SERVER", &a.server).set("KEYWORD", k).set("DATA", v);
            bag.push(b);
        }
    }
    for u in &s.user {
        let mut r = QATOCSTART.blank();
        r.set("SERVERTYPE", "U")
            .set("SERVER", &u.special)
            .set("AUTOSTART", yes(u.autostart))
            .set("LIBRARY", &u.pgm.1)
            .set("PROGRAM", &u.pgm.0)
            .set_at("EXTSTRCMD", 0, 30, &u.name)
            .set_at("EXTSTRCMD", 30, 30, &u.kind)
            .set("EXTENDCMD", &u.text);
        start.push(r);
    }
    vec![(&QATOCSTART, start), (&QATOCSVRA, bag)]
}

pub(super) fn decode_servers(start: &[Rec], bag: &[Rec]) -> Servers {
    let mut s = Servers::default();
    for r in start {
        let server = r.get("SERVER");
        let auto = r.get("AUTOSTART") == "*YES";
        if r.get("SERVERTYPE") == "U" {
            s.user.push(UserServer {
                special: server,
                pgm: (r.get("PROGRAM"), r.get("LIBRARY")),
                name: r.get_at("EXTSTRCMD", 0, 30),
                kind: r.get_at("EXTSTRCMD", 30, 30),
                autostart: auto,
                text: r.get("EXTENDCMD"),
            });
        } else if !server.is_empty() {
            s.get_mut(&server).autostart = auto;
        }
    }
    for r in bag {
        let server = r.get("SERVER");
        if !server.is_empty() {
            s.get_mut(&server).bag.push((r.get("KEYWORD"), r.get("DATA")));
        }
    }
    s
}

fn set_aliases(r: &mut Rec, aliases: &[String]) {
    for (f, a) in ALIAS_FIELDS.iter().zip(aliases) {
        r.set(f, a);
    }
}

fn aliases(r: &Rec) -> Vec<String> {
    ALIAS_FIELDS.iter().map(|f| r.get(f)).filter(|a| !a.is_empty()).collect()
}

pub(super) fn encode_services(t: &ServicesTable) -> Vec<(&'static Layout, Vec<Rec>)> {
    let recs = t
        .list
        .iter()
        .map(|e| {
            let mut r = QATOCPS.blank();
            r.set("SERVICE", &e.name)
                .set_bin("PORT", i64::from(e.port))
                .set("PROTOCOL", &e.proto)
                .set("TEXT", &e.text);
            set_aliases(&mut r, &e.aliases);
            r
        })
        .collect();
    vec![(&QATOCPS, recs)]
}

pub(super) fn decode_services(recs: &[Rec]) -> ServicesTable {
    let list = recs
        .iter()
        .filter_map(|r| {
            Some(ServiceEntry {
                name: r.get("SERVICE"),
                port: u16::try_from(r.bin("PORT")).ok()?,
                proto: r.get("PROTOCOL"),
                aliases: aliases(r),
                text: r.get("TEXT"),
            })
        })
        .filter(|e| !e.name.is_empty())
        .collect();
    ServicesTable { list }
}

pub(super) fn encode_protocols(t: &[ProtocolEntry]) -> Vec<(&'static Layout, Vec<Rec>)> {
    let recs = t
        .iter()
        .map(|e| {
            let mut r = QATOCPP.blank();
            r.set("PROTOCOL", &e.name).set_bin("PCLNBR", i64::from(e.number)).set("TEXT", &e.text);
            set_aliases(&mut r, &e.aliases);
            r
        })
        .collect();
    vec![(&QATOCPP, recs)]
}

pub(super) fn decode_protocols(recs: &[Rec]) -> Vec<ProtocolEntry> {
    recs.iter()
        .filter_map(|r| {
            Some(ProtocolEntry {
                name: r.get("PROTOCOL"),
                number: u8::try_from(r.bin("PCLNBR")).ok()?,
                aliases: aliases(r),
                text: r.get("TEXT"),
            })
        })
        .filter(|e| !e.name.is_empty())
        .collect()
}

pub(super) fn encode_networks(t: &[NetworkEntry]) -> Vec<(&'static Layout, Vec<Rec>)> {
    let recs = t
        .iter()
        .map(|e| {
            let mut r = QATOCPN.blank();
            r.set("NETWORK", &e.name)
                .set("ADDRESS", &e.addr.to_string())
                .set("TEXT", &e.text)
                .set_bin("IPINTGER", v4_bits(e.addr));
            set_aliases(&mut r, &e.aliases);
            r
        })
        .collect();
    vec![(&QATOCPN, recs)]
}

pub(super) fn decode_networks(recs: &[Rec]) -> Vec<NetworkEntry> {
    recs.iter()
        .filter_map(|r| {
            Some(NetworkEntry {
                name: r.get("NETWORK"),
                addr: r.get("ADDRESS").parse().ok()?,
                aliases: aliases(r),
                text: r.get("TEXT"),
            })
        })
        .collect()
}
