// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the sockets layer reads of the configuration: the resolver's
//! [`HostConfig`], the port restrictions every `bind()` is checked
//! against, and the services table.

use std::net::IpAddr;

use crate::config::Config;
use crate::model::Proto;
use crate::model::dmn::SearchPriority;
use crate::model::port::PortRestriction;
use crate::model::tables::ServicesTable;

/// The resolver's configuration: `CHGTCPDMN` and the host table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostConfig {
    /// The host table: each address and its names, in table order.
    pub hosts: Vec<(IpAddr, Vec<String>)>,
    /// `HOSTNAME`.
    pub host_name: String,
    /// `DMNNAME`.
    pub dmn_name: String,
    /// `DMNSCHLIST` in effect (the local domain when none is set).
    pub dmn_sch_list: Vec<String>,
    /// `HOSTSCHPTY`: `true` for `*LOCAL` (host table first).
    pub host_sch_pty_local: bool,
    /// `INTNETADR`: the name servers, in order.
    pub dns: Vec<IpAddr>,
    /// `PORT`.
    pub port: u16,
    /// `PROTOCOL`: `true` for `*TCP`.
    pub tcp: bool,
    /// `INITQRYINT`: seconds before the first retry (`DMNSVRRTY`'s
    /// interval on this release's command).
    pub init_qry_int: u32,
    /// `QRYTIMOUT`: seconds a query may take in all (attempts times the
    /// interval, per server).
    pub qry_timeout: u32,
}

impl HostConfig {
    /// From the configuration.
    pub fn from_config(cfg: &Config) -> Self {
        let d = &cfg.domain;
        Self {
            hosts: cfg.hosts.list.iter().map(|e| (e.addr, e.names.clone())).collect(),
            host_name: d.host_name.clone(),
            dmn_name: d.domain_name.clone(),
            dmn_sch_list: d.search(),
            host_sch_pty_local: d.priority == SearchPriority::Local,
            dns: d.dns.clone(),
            port: d.port,
            tcp: d.protocol == Proto::Tcp,
            init_qry_int: u32::from(d.retry_interval),
            qry_timeout: u32::from(d.retry_interval) * u32::from(d.retry_attempts.max(1)),
        }
    }

    /// The host table's addresses for a name (case ignored; a trailing
    /// dot or the local domain's suffix tried as well).
    pub fn lookup(&self, name: &str) -> Vec<IpAddr> {
        let n = name.trim_end_matches('.');
        let mut tries = vec![n.to_string()];
        if !n.contains('.') {
            tries.extend(self.dmn_sch_list.iter().map(|d| format!("{n}.{d}")));
        }
        for t in tries {
            let found: Vec<IpAddr> = self
                .hosts
                .iter()
                .filter(|(_, names)| names.iter().any(|x| x.eq_ignore_ascii_case(&t)))
                .map(|(a, _)| *a)
                .collect();
            if !found.is_empty() {
                return found;
            }
        }
        Vec::new()
    }

    /// The first name the host table has for an address.
    pub fn name_of(&self, addr: IpAddr) -> Option<&str> {
        self.hosts
            .iter()
            .find(|(a, _)| *a == addr)
            .and_then(|(_, n)| n.first())
            .map(String::as_str)
    }
}

/// The port restrictions (`ADDTCPPORT`), for the sockets layer's
/// `PortRules`.
pub fn port_rules(cfg: &Config) -> Vec<PortRestriction> {
    cfg.ports.list.clone()
}

/// The services table, for `getservbyname`.
pub fn services(cfg: &Config) -> ServicesTable {
    cfg.services.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::host::HostEntry;

    #[test]
    fn lookups() {
        let mut cfg = Config::default();
        cfg.domain.domain_name = "EXAMPLE.COM".into();
        cfg.hosts
            .add(HostEntry::new("10.0.2.15".parse().unwrap(), &["qs400.example.com".into()], "").unwrap())
            .unwrap();
        let h = HostConfig::from_config(&cfg);
        assert_eq!(h.lookup("localhost"), vec!["127.0.0.1".parse::<IpAddr>().unwrap()]);
        assert_eq!(h.lookup("qs400"), vec!["10.0.2.15".parse::<IpAddr>().unwrap()]);
        assert_eq!(h.lookup("QS400.EXAMPLE.COM."), h.lookup("qs400"));
        assert!(h.lookup("nothere").is_empty());
        assert_eq!(h.name_of("::1".parse().unwrap()), Some("IPV6-LOOPBACK"));
        assert!(h.host_sch_pty_local);
        assert_eq!((h.port, h.tcp, h.init_qry_int, h.qry_timeout), (53, false, 2, 4));
    }
}
