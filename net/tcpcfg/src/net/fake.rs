// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A kernel in memory, for tests: links, addresses, routes, sysctls,
//! port rules and the host name, with every operation logged and any
//! operation made to fail.

use std::collections::{BTreeMap, BTreeSet};

use super::{KAddr, KRoute, LinkInfo, NetError, NetOps, NetResult};
use crate::model::Proto;

/// The fake kernel.
#[derive(Clone, Debug)]
pub struct FakeNet {
    /// The links.
    pub links: Vec<LinkInfo>,
    /// The addresses.
    pub addrs: Vec<KAddr>,
    /// The routes.
    pub routes: Vec<KRoute>,
    /// Sysctl values by key.
    pub sysctls: BTreeMap<String, String>,
    /// The port rules.
    pub rules: Vec<(Proto, u16, u16)>,
    /// The host name.
    pub hostname: String,
    /// Every operation, as `op args`.
    pub log: Vec<String>,
    /// Operations that fail (`add_address`, `add_route`, ...).
    pub failing: BTreeSet<String>,
}

impl Default for FakeNet {
    /// A kernel with `lo` up and its addresses.
    fn default() -> Self {
        let mut sysctls = BTreeMap::new();
        sysctls.insert("ipv4/tcp_rmem".into(), "4096\t131072\t6291456".into());
        sysctls.insert("ipv4/tcp_wmem".into(), "4096\t16384\t4194304".into());
        Self {
            links: vec![LinkInfo {
                index: 1,
                name: "lo".into(),
                up: true,
                carrier: Some(true),
                mtu: 65536,
            }],
            addrs: vec![
                KAddr {
                    link: 1,
                    addr: "127.0.0.1".parse().expect("v4"),
                    prefix: 8,
                },
                KAddr {
                    link: 1,
                    addr: "::1".parse().expect("v6"),
                    prefix: 128,
                },
            ],
            routes: Vec::new(),
            sysctls,
            rules: Vec::new(),
            hostname: String::new(),
            log: Vec::new(),
            failing: BTreeSet::new(),
        }
    }
}

impl FakeNet {
    /// Add an Ethernet link, down, with the given carrier.
    #[must_use]
    pub fn with_link(mut self, name: &str, carrier: bool) -> Self {
        let index = self.next_index();
        self.links.push(LinkInfo {
            index,
            name: name.into(),
            up: false,
            carrier: Some(carrier),
            mtu: 1500,
        });
        self
    }

    /// Set a link's carrier.
    pub fn set_carrier(&mut self, name: &str, carrier: bool) {
        if let Some(l) = self.links.iter_mut().find(|l| l.name == name) {
            l.carrier = Some(carrier);
        }
    }

    /// The addresses on a link, by name.
    pub fn addrs_on(&self, name: &str) -> Vec<String> {
        let Some(l) = self.links.iter().find(|l| l.name == name) else {
            return Vec::new();
        };
        self.addrs
            .iter()
            .filter(|a| a.link == l.index)
            .map(|a| format!("{}/{}", a.addr, a.prefix))
            .collect()
    }

    fn next_index(&self) -> u32 {
        self.links.iter().map(|l| l.index).max().unwrap_or(0) + 1
    }

    fn op(&mut self, name: &str, args: &str) -> NetResult<()> {
        self.log.push(format!("{name} {args}").trim_end().to_string());
        if self.failing.contains(name) {
            return Err(NetError::new(format!("{name} refused")));
        }
        Ok(())
    }

    fn link_mut(&mut self, index: u32) -> NetResult<&mut LinkInfo> {
        self.links.iter_mut().find(|l| l.index == index).ok_or(NetError {
            text: format!("no link {index}"),
            exists: false,
            not_found: true,
        })
    }
}

fn exists(what: &str) -> NetError {
    NetError {
        text: format!("{what} exists"),
        exists: true,
        not_found: false,
    }
}

fn missing(what: &str) -> NetError {
    NetError {
        text: format!("{what} not found"),
        exists: false,
        not_found: true,
    }
}

impl NetOps for FakeNet {
    fn links(&mut self) -> NetResult<Vec<LinkInfo>> {
        Ok(self.links.clone())
    }

    fn set_up(&mut self, index: u32, up: bool) -> NetResult<()> {
        self.op("set_up", &format!("{index} {up}"))?;
        self.link_mut(index)?.up = up;
        Ok(())
    }

    fn set_mtu(&mut self, index: u32, mtu: u32) -> NetResult<()> {
        self.op("set_mtu", &format!("{index} {mtu}"))?;
        self.link_mut(index)?.mtu = mtu;
        Ok(())
    }

    fn create_dummy(&mut self, name: &str) -> NetResult<LinkInfo> {
        self.op("create_dummy", name)?;
        if self.links.iter().any(|l| l.name == name) {
            return Err(exists(name));
        }
        let l = LinkInfo {
            index: self.next_index(),
            name: name.into(),
            up: false,
            carrier: Some(true),
            mtu: 1500,
        };
        self.links.push(l.clone());
        Ok(l)
    }

    fn create_vlan(&mut self, name: &str, parent: u32, id: u16) -> NetResult<LinkInfo> {
        self.op("create_vlan", &format!("{name} {parent} {id}"))?;
        if self.links.iter().any(|l| l.name == name) {
            return Err(exists(name));
        }
        let carrier = self.links.iter().find(|l| l.index == parent).and_then(|l| l.carrier);
        let l = LinkInfo {
            index: self.next_index(),
            name: name.into(),
            up: false,
            carrier,
            mtu: 1500,
        };
        self.links.push(l.clone());
        Ok(l)
    }

    fn delete_link(&mut self, index: u32) -> NetResult<()> {
        self.op("delete_link", &index.to_string())?;
        let before = self.links.len();
        self.links.retain(|l| l.index != index);
        if before == self.links.len() {
            return Err(missing("link"));
        }
        self.addrs.retain(|a| a.link != index);
        self.routes.retain(|r| r.link != Some(index));
        Ok(())
    }

    fn addresses(&mut self) -> NetResult<Vec<KAddr>> {
        Ok(self.addrs.clone())
    }

    fn add_address(&mut self, a: &KAddr) -> NetResult<()> {
        self.op("add_address", &format!("{} {}/{}", a.link, a.addr, a.prefix))?;
        if self.addrs.iter().any(|x| x.link == a.link && x.addr == a.addr) {
            return Err(exists("address"));
        }
        self.addrs.push(a.clone());
        Ok(())
    }

    fn del_address(&mut self, a: &KAddr) -> NetResult<()> {
        self.op("del_address", &format!("{} {}/{}", a.link, a.addr, a.prefix))?;
        let before = self.addrs.len();
        self.addrs.retain(|x| !(x.link == a.link && x.addr == a.addr));
        if before == self.addrs.len() {
            return Err(missing("address"));
        }
        Ok(())
    }

    fn routes(&mut self) -> NetResult<Vec<KRoute>> {
        Ok(self.routes.clone())
    }

    fn add_route(&mut self, r: &KRoute) -> NetResult<()> {
        let dst = r.dst.map_or("default".into(), |d| d.to_string());
        let via = r.gateway.map_or(String::new(), |g| format!(" via {g}"));
        self.op("add_route", &format!("{dst}/{}{via}", r.prefix))?;
        if self
            .routes
            .iter()
            .any(|x| x.dst == r.dst && x.prefix == r.prefix && x.gateway == r.gateway && x.tos == r.tos)
        {
            return Err(exists("route"));
        }
        if let Some(gw) = r.gateway {
            let reachable = self.addrs.iter().any(|a| crate::addr::contains(a.addr, a.prefix, gw));
            if !reachable {
                return Err(NetError::new(format!("{gw} is not on a connected network")));
            }
        }
        self.routes.push(r.clone());
        Ok(())
    }

    fn del_route(&mut self, r: &KRoute) -> NetResult<()> {
        let dst = r.dst.map_or("default".into(), |d| d.to_string());
        self.op("del_route", &format!("{dst}/{}", r.prefix))?;
        let before = self.routes.len();
        self.routes
            .retain(|x| !(x.dst == r.dst && x.prefix == r.prefix && x.gateway == r.gateway && x.tos == r.tos));
        if before == self.routes.len() {
            return Err(missing("route"));
        }
        Ok(())
    }

    fn sysctl_read(&mut self, key: &str) -> NetResult<String> {
        self.sysctls.get(key).cloned().ok_or_else(|| missing(key))
    }

    fn sysctl_write(&mut self, key: &str, value: &str) -> NetResult<()> {
        self.op("sysctl", &format!("{key}={value}"))?;
        self.sysctls.insert(key.into(), value.into());
        Ok(())
    }

    fn set_port_rules(&mut self, rules: &[(Proto, u16, u16)]) -> NetResult<()> {
        self.op("port_rules", &format!("{}", rules.len()))?;
        self.rules = rules.to_vec();
        Ok(())
    }

    fn set_hostname(&mut self, name: &str) -> NetResult<()> {
        self.op("hostname", name)?;
        self.hostname = name.into();
        Ok(())
    }
}
