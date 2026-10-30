// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The configuration files in `QUSRSYS` and the records each table
//! becomes. The storage service creates the files ([`defs::ALL`]) at
//! the first IPL, writes a table's records whole when a command changes
//! it ([`encode`]) and reads them all back at start ([`decode`]).

pub mod defs;
mod ifc;
pub mod layout;
mod misc;
mod rte;

pub use layout::{Field, Layout, Rec, Ty};

use crate::config::Config;

/// A table of the configuration, the unit a command rewrites.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Table {
    /// Interfaces.
    Ifc,
    /// Routes.
    Rte,
    /// The host table.
    Host,
    /// Attributes and domain.
    TcpIp,
    /// Port restrictions.
    Port,
    /// Servers and their attributes.
    Servers,
    /// The services table.
    Services,
    /// The protocol table.
    Protocols,
    /// The network table.
    Networks,
}

impl Table {
    /// Every table.
    pub const ALL: &'static [Table] = &[
        Table::Ifc,
        Table::Rte,
        Table::Host,
        Table::TcpIp,
        Table::Port,
        Table::Servers,
        Table::Services,
        Table::Protocols,
        Table::Networks,
    ];

    /// The files it is written to.
    pub fn files(self) -> &'static [&'static Layout] {
        use defs::{
            QATOCHOST, QATOCHST6, QATOCIFC, QATOCIFC6, QATOCPN, QATOCPORT, QATOCPP, QATOCPS, QATOCRTE,
            QATOCRTE6, QATOCSTART, QATOCSVRA, QATOCTCPIP,
        };
        match self {
            Table::Ifc => &[&QATOCIFC, &QATOCIFC6],
            Table::Rte => &[&QATOCRTE, &QATOCRTE6],
            Table::Host => &[&QATOCHOST, &QATOCHST6],
            Table::TcpIp => &[&QATOCTCPIP],
            Table::Port => &[&QATOCPORT],
            Table::Servers => &[&QATOCSTART, &QATOCSVRA],
            Table::Services => &[&QATOCPS],
            Table::Protocols => &[&QATOCPP],
            Table::Networks => &[&QATOCPN],
        }
    }
}

/// A table's records, file by file.
pub fn encode(cfg: &Config, t: Table) -> Vec<(&'static str, Vec<Vec<u8>>)> {
    let pairs: Vec<(&'static Layout, Vec<Rec>)> = match t {
        Table::Ifc => ifc::encode(&cfg.ifcs),
        Table::Rte => rte::encode(&cfg.rtes),
        Table::Host => misc::encode_hosts(&cfg.hosts),
        Table::TcpIp => misc::encode_tcpip(&cfg.attrs, &cfg.domain),
        Table::Port => misc::encode_ports(&cfg.ports),
        Table::Servers => misc::encode_servers(&cfg.servers),
        Table::Services => misc::encode_services(&cfg.services),
        Table::Protocols => misc::encode_protocols(&cfg.protocols),
        Table::Networks => misc::encode_networks(&cfg.networks),
    };
    pairs
        .into_iter()
        .map(|(l, recs)| (l.file, recs.into_iter().map(|r| r.bytes).collect()))
        .collect()
}

/// The configuration from the files' records; `read` gives a file's
/// records (none for a file that is missing). Records that do not
/// decode are skipped.
pub fn decode(read: &mut dyn FnMut(&str) -> Vec<Vec<u8>>) -> Config {
    let mut recs = |l: &'static Layout| -> Vec<Rec> { read(l.file).iter().map(|b| l.rec(b)).collect() };
    let v4 = recs(&defs::QATOCIFC);
    let v6 = recs(&defs::QATOCIFC6);
    let ifcs = ifc::decode(&v4, &v6);
    let r4 = recs(&defs::QATOCRTE);
    let r6 = recs(&defs::QATOCRTE6);
    let rtes = rte::decode(&r4, &r6);
    let h4 = recs(&defs::QATOCHOST);
    let h6 = recs(&defs::QATOCHST6);
    let hosts = misc::decode_hosts(&h4, &h6);
    let tcpip = recs(&defs::QATOCTCPIP);
    let (attrs, domain) = misc::decode_tcpip(&tcpip);
    let ports = misc::decode_ports(&recs(&defs::QATOCPORT));
    let start = recs(&defs::QATOCSTART);
    let svra = recs(&defs::QATOCSVRA);
    let servers = misc::decode_servers(&start, &svra);
    let services = misc::decode_services(&recs(&defs::QATOCPS));
    let protocols = misc::decode_protocols(&recs(&defs::QATOCPP));
    let networks = misc::decode_networks(&recs(&defs::QATOCPN));
    Config {
        ifcs,
        rtes,
        hosts,
        domain,
        attrs,
        ports,
        servers,
        services,
        protocols,
        networks,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::addr::IfcAddr;
    use crate::model::LineRef;
    use crate::model::host::HostEntry;
    use crate::model::ifc::Interface;
    use crate::model::rte::{Dest, Route};

    #[test]
    fn every_table_round_trips() {
        let mut cfg = Config::default();
        let mut i = Interface::new(IfcAddr::parse("10.0.2.16").unwrap(), LineRef::Line("ETHLINE".into()));
        i.mask = Some("255.255.255.0".parse().unwrap());
        i.vlan = Some(12);
        i.alias = Some("A-VERY-LONG-ALIAS-NAME-25".into());
        i.text = "Ethernet interface".into();
        i.mtu = Some(1500);
        i.pref_ifc = vec!["10.0.2.17".into()];
        cfg.ifcs.add(i).unwrap();
        let mut d = Interface::new(IfcAddr::Dhcp4, LineRef::Line("ETHLINE".into()));
        d.autostart = false;
        cfg.ifcs.add(d).unwrap();
        let mut v6 = Interface::new(IfcAddr::parse("2001:db8::5").unwrap(), LineRef::Line("ETHLINE".into()));
        v6.prefix_len = Some(64);
        cfg.ifcs.add(v6).unwrap();
        cfg.ifcs.add(Interface::new(IfcAddr::Slaac6, LineRef::Line("ETHLINE".into()))).unwrap();
        let mut r = Route::new(Dest::DftRoute, 0, "10.0.2.2".parse().unwrap());
        r.text = "the way out".into();
        r.dup_pty = 7;
        cfg.rtes.add(r).unwrap();
        let mut r6 = Route::new(Dest::parse("*DFT6ROUTE").unwrap(), 0, "2001:db8::1".parse().unwrap());
        r6.mtu = Some(1400);
        cfg.rtes.add(r6).unwrap();
        let names: Vec<String> = (0..7).map(|n| format!("name{n}")).collect();
        cfg.hosts
            .add(HostEntry::new("10.0.2.16".parse().unwrap(), &names, "seven names").unwrap())
            .unwrap();
        cfg.attrs.set("TCPKEEPALV", "5").unwrap();
        cfg.attrs.set("IPDEADGATE", "*NO").unwrap();
        cfg.domain.host_name = "QS400".into();
        cfg.domain.dns = vec!["10.0.2.3".parse().unwrap(), "2001:db8::53".parse().unwrap()];
        cfg.domain.search_list = vec!["A.COM".into(), "B.COM".into()];
        cfg.ports
            .add(crate::model::port::PortRestriction {
                lower: 8000,
                upper: Some(8100),
                proto: crate::model::Proto::Udp,
                user: "WEB".into(),
            })
            .unwrap();
        cfg.servers.get_mut("*FTP").autostart = false;
        cfg.servers.get_mut("*SSHD").set("PORT", "2222");
        cfg.servers
            .add_user(crate::model::svr::UserServer {
                special: "*MYSVR".into(),
                pgm: ("MYPGM".into(), "QGPL".into()),
                name: "My server".into(),
                kind: "MINE".into(),
                autostart: true,
                text: "mine".into(),
            })
            .unwrap();
        let mut files: HashMap<&str, Vec<Vec<u8>>> = HashMap::new();
        for t in Table::ALL {
            for (f, recs) in encode(&cfg, *t) {
                let l = defs::layout(f).unwrap();
                assert!(recs.iter().all(|r| r.len() == l.record_len()), "{f}");
                files.insert(f, recs);
            }
        }
        assert_eq!(files["QATOCHOST"].len(), 3, "loopback plus two records of seven names");
        let back = decode(&mut |f| files.get(f).cloned().unwrap_or_default());
        // The IPv4 and IPv6 files read back one after the other.
        cfg.hosts.list.sort_by_key(|e| e.addr.is_ipv6());
        cfg.ifcs.list.sort_by_key(|i| i.addr.is_v6());
        cfg.rtes.list.sort_by_key(|r| r.dest.is_v6());
        assert_eq!(back, cfg);
    }

    #[test]
    fn empty_files_read_as_defaults_where_ibm_has_them() {
        let back = decode(&mut |_| Vec::new());
        assert!(back.ifcs.list.is_empty());
        assert_eq!(back.attrs.get("TCPKEEPALV"), "120");
        assert_eq!(back.servers.autostart(), Config::default().servers.autostart());
    }
}
