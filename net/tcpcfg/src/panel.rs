// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The CFGTCP panels' data: the variables and list entries of the
//! `QTOC*` panel groups, named as the groups name them. Pure, so the
//! panels' contents are tested without a display.

use crate::addr::IfcAddr;
use crate::config::Config;
use crate::model::rte::{Dest, Route};
use crate::model::{Tos, yes};
use crate::state::{IfcStatus, State};

/// One list entry: `(variable, value)` pairs.
pub type Row = Vec<(String, String)>;

/// A panel's variables and its named lists.
pub type Data = (Vec<(String, String)>, Vec<(String, Vec<Row>)>);

fn row(pairs: &[(&str, String)]) -> Row {
    pairs.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect()
}

fn vars(pairs: &[(&str, String)]) -> Vec<(String, String)> {
    row(pairs)
}

fn blank_star(t: &str) -> String {
    if t.is_empty() { "*BLANK".into() } else { t.to_string() }
}

/// The panel data `what` names (`IFC`, `IFC5`, `RTE`, `RTE5`, `PORT`,
/// `HTE`, `HTE5`, `SRV`, `PCL`, `NET`); `None` for an unknown one or a
/// display whose entry is gone.
pub fn data(cfg: &Config, st: &State, what: &str, args: &[String]) -> Option<Data> {
    let arg = |i: usize| args.get(i).map_or("", String::as_str);
    Some(match what {
        "IFC" => (Vec::new(), vec![("IFCS".into(), ifcs(cfg, st))]),
        "IFC5" => (ifc_detail(cfg, st, arg(0))?, Vec::new()),
        "RTE" => (Vec::new(), vec![("RTES".into(), rtes(cfg, st))]),
        "RTE5" => (rte_detail(cfg, args)?, Vec::new()),
        "PORT" => (Vec::new(), vec![("PORTS".into(), ports(cfg))]),
        "HTE" => (Vec::new(), vec![("HOSTS".into(), hosts(cfg, arg(0) == "ADD"))]),
        "HTE5" => hte_detail(cfg, arg(0))?,
        "SRV" => (Vec::new(), vec![("SVCS".into(), services(cfg))]),
        "PCL" => (Vec::new(), vec![("PCLS".into(), protocols(cfg))]),
        "NET" => (Vec::new(), vec![("NETS".into(), networks(cfg))]),
        _ => return None,
    })
}

/// Option 1's entries: the IPv4 interfaces and `*IP4DHCP`.
pub fn ifcs(cfg: &Config, st: &State) -> Vec<Row> {
    cfg.ifcs
        .sorted()
        .into_iter()
        .filter(|i| !i.addr.is_v6())
        .map(|i| {
            let status = match (i.addr, st.ifc_status(&i.key())) {
                (IfcAddr::Dhcp4, IfcStatus::Inactive) => String::new(),
                (_, s) => s.name().to_string(),
            };
            row(&[
                ("IFCADR", i.key()),
                ("IFCMASK", i.mask.map_or_else(|| "*".into(), |m| m.to_string())),
                ("IFCLIND", i.line.name().to_string()),
                ("IFCLTYPE", i.line_type().to_string()),
                ("IFCSTS", status),
                ("IFCALIAS", i.alias.clone().unwrap_or_else(|| "*NONE".into())),
                ("IFCTEXT", i.text.clone()),
            ])
        })
        .collect()
}

/// Option 5 on an interface.
pub fn ifc_detail(cfg: &Config, st: &State, key: &str) -> Option<Vec<(String, String)>> {
    let i = cfg.ifcs.get(&IfcAddr::parse(key).ok()?)?;
    Some(vars(&[
        ("DADR", i.key()),
        ("DMASK", i.mask.map_or_else(|| "*".into(), |m| m.to_string())),
        ("DLIND", i.line.name().to_string()),
        ("DALIAS", i.alias.clone().unwrap_or_else(|| "*NONE".into())),
        ("DCCSID", if i.alias.is_some() { "37".into() } else { "*NONE".into() }),
        ("DLTYPE", i.line_type().to_string()),
        ("DLCLIFC", i.local_ifc.map_or_else(|| "*NONE".into(), |l| l.to_string())),
        ("DSTS", st.ifc_status(&i.key()).name().to_string()),
        ("DTOS", i.tos.name().to_string()),
        ("DMTU", i.mtu.map_or_else(|| "*LIND".into(), |m| m.to_string())),
        ("DAUTO", yes(i.autostart).to_string()),
        ("DTEXT", blank_star(&i.text)),
    ]))
}

/// Option 2's entries: the IPv4 routes.
pub fn rtes(cfg: &Config, st: &State) -> Vec<Row> {
    cfg.rtes
        .sorted()
        .into_iter()
        .filter(|r| !r.dest.is_v6())
        .map(|r| {
            row(&[
                ("RTEDEST", r.dest.to_string()),
                ("RTEMASK", r.mask_text()),
                ("RTEHOP", r.next_hop.to_string()),
                ("RTEIFC", r.bind_ifc.map_or_else(|| "*NONE".into(), |b| b.to_string())),
                ("RTETOS", r.tos.name().to_string()),
                ("RTESTS", st.rte_status(&r.key()).name().to_string()),
                ("RTETEXT", r.text.clone()),
            ])
        })
        .collect()
}

/// A route from the four values a list entry shows.
pub fn route_of<'c>(cfg: &'c Config, args: &[String]) -> Option<&'c Route> {
    let [dest, mask, tos, hop] = args else {
        return None;
    };
    let dest = Dest::parse(dest).ok()?;
    let prefix = if mask == "*NONE" {
        0
    } else {
        crate::addr::mask_prefix(mask.parse().ok()?)?
    };
    let tos = Tos::parse(tos).ok()?;
    let hop: std::net::IpAddr = hop.parse().ok()?;
    cfg.rtes
        .list
        .iter()
        .find(|r| r.dest == dest && r.prefix == prefix && r.tos == tos && r.next_hop == hop)
}

/// Option 5 on a route.
pub fn rte_detail(cfg: &Config, args: &[String]) -> Option<Vec<(String, String)>> {
    let r = route_of(cfg, args)?;
    Some(vars(&[
        ("DDEST", r.dest.to_string()),
        ("DMASK", r.mask_text()),
        ("DTOS", r.tos.name().to_string()),
        ("DHOP", r.next_hop.to_string()),
        ("DIFC", r.bind_ifc.map_or_else(|| "*NONE".into(), |b| b.to_string())),
        ("DMTU", r.mtu.map_or_else(|| "*IFC".into(), |m| m.to_string())),
        ("DDUP", r.dup_pty.to_string()),
        ("DMETRIC", r.metric.to_string()),
        ("DREDST", yes(r.redist).to_string()),
        ("DTEXT", blank_star(&r.text)),
    ]))
}

/// Option 4's entries.
pub fn ports(cfg: &Config) -> Vec<Row> {
    cfg.ports
        .sorted()
        .into_iter()
        .map(|p| {
            row(&[
                ("PLOWER", p.lower.to_string()),
                ("PUPPER", p.upper.map_or_else(|| "*ONLY".into(), |u| u.to_string())),
                ("PPROTO", p.proto.name().to_string()),
                ("PUSER", p.user.clone()),
            ])
        })
        .collect()
}

/// Option 10's entries: a row per host name, the address on the
/// first; `by_add` keeps the order entries were added (F13).
pub fn hosts(cfg: &Config, by_add: bool) -> Vec<Row> {
    let list: Vec<_> = if by_add {
        cfg.hosts.list.iter().collect()
    } else {
        cfg.hosts.sorted()
    };
    let mut out = Vec::new();
    for e in list {
        for (n, name) in e.names.iter().enumerate() {
            out.push(row(&[
                ("HTEADR", if n == 0 { e.addr.to_string() } else { String::new() }),
                ("HTENAME", name.clone()),
                ("HTEKEY", e.addr.to_string()),
            ]));
        }
    }
    out
}

/// Option 5 on a host table entry: its address and text, its names.
pub fn hte_detail(cfg: &Config, addr: &str) -> Option<Data> {
    let e = cfg.hosts.get(addr.trim().parse().ok()?)?;
    let names = e.names.iter().map(|n| row(&[("DNAME", n.clone())])).collect();
    Some((
        vars(&[("DADR", e.addr.to_string()), ("DTEXT", e.text.clone())]),
        vec![("NAMES".into(), names)],
    ))
}

/// The services table's entries.
pub fn services(cfg: &Config) -> Vec<Row> {
    cfg.services
        .sorted()
        .into_iter()
        .map(|e| {
            row(&[
                ("SVCNAME", e.name.clone()),
                ("SVCPORT", e.port.to_string()),
                ("SVCPROTO", e.proto.clone()),
            ])
        })
        .collect()
}

/// The protocol table's entries.
pub fn protocols(cfg: &Config) -> Vec<Row> {
    let mut v: Vec<_> = cfg.protocols.iter().collect();
    v.sort_by_key(|p| p.name.to_ascii_lowercase());
    v.into_iter()
        .map(|p| row(&[("PCLNAME", p.name.clone()), ("PCLNBR", p.number.to_string())]))
        .collect()
}

/// The network table's entries.
pub fn networks(cfg: &Config) -> Vec<Row> {
    let mut v: Vec<_> = cfg.networks.iter().collect();
    v.sort_by_key(|n| n.name.to_ascii_lowercase());
    v.into_iter()
        .map(|n| row(&[("NETNAME", n.name.clone()), ("NETADR", n.addr.to_string())]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::host::HostEntry;

    #[test]
    fn panel_data() {
        let mut cfg = Config::default();
        cfg.hosts
            .add(HostEntry::new("185.113.5.134".parse().unwrap(), &["PUB400.COM".into(), "WWW.PUB400.COM".into()], "").unwrap())
            .unwrap();
        let st = State::default();
        let ifc = ifcs(&cfg, &st);
        assert_eq!(ifc.len(), 1);
        assert_eq!(ifc[0][0], ("IFCADR".into(), "127.0.0.1".into()));
        assert_eq!(ifc[0][5].1, "LOCALHOST");
        let d = ifc_detail(&cfg, &st, "127.0.0.1").unwrap();
        assert!(d.contains(&("DMTU".into(), "576".into())));
        assert!(d.contains(&("DTEXT".into(), "*BLANK".into())));
        let h = hosts(&cfg, false);
        let addrs: Vec<&str> = h.iter().map(|r| r[0].1.as_str()).collect();
        assert_eq!(addrs, vec!["::1", "", "127.0.0.1", "", "185.113.5.134", ""]);
        assert_eq!(h[5][2].1, "185.113.5.134");
        let (v, lists) = hte_detail(&cfg, "::1").unwrap();
        assert_eq!(v[0].1, "::1");
        assert_eq!(lists[0].1.len(), 2);
        assert!(data(&cfg, &st, "RTE5", &[]).is_none());
        assert_eq!(protocols(&cfg)[0][0].1, "icmp");
    }
}
