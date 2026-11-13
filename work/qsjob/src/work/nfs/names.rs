// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Host names for the export table's host lists: the host table and the
//! name servers of `CFGTCP` (`HOSTSCHPTY` decides which first), each
//! answer kept a minute.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use nfs::exports::Resolver;
use sockets::resolver::{self as res, DnsProtocol, DomainConfig, SearchPriority};
use tcpcfg::resolver::HostConfig;

/// How long an answer is kept.
const KEEP: Duration = Duration::from_secs(60);

type Cache<K, V> = Mutex<HashMap<K, (Instant, V)>>;

/// Names through the system's resolver configuration.
pub struct HostNames {
    cfg: HostConfig,
    by_name: Cache<String, Vec<IpAddr>>,
    by_addr: Cache<IpAddr, Vec<String>>,
}

impl HostNames {
    /// Over `cfg`.
    #[must_use]
    pub fn new(cfg: HostConfig) -> Self {
        Self {
            cfg,
            by_name: Mutex::default(),
            by_addr: Mutex::default(),
        }
    }

    fn resolver(&self) -> res::Resolver {
        let hc = &self.cfg;
        let domain = DomainConfig {
            host_name: hc.host_name.clone(),
            domain_name: hc.dmn_name.clone(),
            search_list: hc.dmn_sch_list.clone(),
            priority: if hc.host_sch_pty_local {
                SearchPriority::Local
            } else {
                SearchPriority::Remote
            },
            servers: hc.dns.clone(),
            port: hc.port,
            protocol: if hc.tcp {
                DnsProtocol::Tcp
            } else {
                DnsProtocol::Udp
            },
            initial_query_interval: Duration::from_secs(u64::from(hc.init_qry_int.max(1))),
            query_timeout: Duration::from_secs(u64::from(hc.qry_timeout.max(1))),
            ..DomainConfig::default()
        };
        let cfg = res::StaticHostConfig {
            domain,
            hosts: hc
                .hosts
                .iter()
                .map(|(a, n)| res::HostEntry {
                    addr: *a,
                    names: n.clone(),
                })
                .collect(),
        };
        res::Resolver::new(Box::new(cfg), Box::new(res::DefaultServices))
    }
}

/// A kept answer, or a new one kept.
fn cached<K: std::hash::Hash + Eq + Clone, V: Clone>(
    c: &Cache<K, V>,
    k: &K,
    f: impl FnOnce() -> V,
) -> V {
    if let Some((at, v)) = c.lock().unwrap_or_else(PoisonError::into_inner).get(k)
        && at.elapsed() < KEEP
    {
        return v.clone();
    }
    let v = f();
    c.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(k.clone(), (Instant::now(), v.clone()));
    v
}

impl Resolver for HostNames {
    fn addresses(&self, name: &str) -> Vec<IpAddr> {
        let key = name.to_ascii_lowercase();
        cached(&self.by_name, &key, || {
            let table = self.cfg.lookup(name);
            if !table.is_empty() && (self.cfg.host_sch_pty_local || self.cfg.dns.is_empty()) {
                return table;
            }
            let mut r = self.resolver();
            let mut v: Vec<IpAddr> = Vec::new();
            for family in [sockets::consts::AF_INET, sockets::consts::AF_INET6] {
                if let Ok(h) = r.by_name(name, family) {
                    v.extend(h.addrs);
                }
            }
            if v.is_empty() { table } else { v }
        })
    }

    fn names(&self, addr: IpAddr) -> Vec<String> {
        cached(&self.by_addr, &addr, || {
            let mut out: Vec<String> = self
                .cfg
                .hosts
                .iter()
                .filter(|(a, _)| *a == addr)
                .flat_map(|(_, n)| n.iter().cloned())
                .collect();
            if out.is_empty()
                && let Ok(h) = self.resolver().by_addr(addr)
            {
                out.push(h.name);
                out.extend(h.aliases);
            }
            out
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_host_table_answers_both_ways() {
        let cfg = HostConfig {
            hosts: vec![(
                "10.0.0.7".parse().unwrap(),
                vec!["alpha.lab.example".into(), "alpha".into()],
            )],
            host_sch_pty_local: true,
            ..HostConfig::default()
        };
        let n = HostNames::new(cfg);
        assert_eq!(
            n.addresses("ALPHA"),
            ["10.0.0.7".parse::<IpAddr>().unwrap()]
        );
        assert_eq!(n.names("10.0.0.7".parse().unwrap())[0], "alpha.lab.example");
        assert!(n.names("10.0.0.8".parse().unwrap()).is_empty());
    }
}
