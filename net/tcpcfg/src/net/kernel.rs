// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! [`NetOps`] over the kernel: rtnetlink for links, addresses and
//! routes, `/proc/sys` for sysctls and the host name, nftables for the
//! port rules. A thin adapter; the sockets open per call. Off Linux
//! every call fails (`netlink` cannot open a socket there).

use std::net::IpAddr;

use netlink::route::{Address, Rtnl};
use netlink::sysctl::Sysctl;
use netlink::{Family, nft};

use super::{KAddr, KRoute, LinkInfo, NetError, NetOps, NetResult};
use crate::model::Proto;

/// The kernel.
#[derive(Debug, Default)]
pub struct Kernel {
    sysctl: Sysctl,
}

impl Kernel {
    /// The kernel, sysctls under `/proc/sys/net`.
    pub fn new() -> Self {
        Self::default()
    }

    fn rtnl() -> NetResult<Rtnl> {
        Rtnl::open().map_err(err)
    }
}

#[allow(clippy::needless_pass_by_value)] // map_err hands it over
fn err(e: netlink::Error) -> NetError {
    NetError {
        exists: e.is_exists(),
        not_found: e.is_not_found(),
        text: e.to_string(),
    }
}

fn info(l: &netlink::route::Link) -> LinkInfo {
    LinkInfo {
        index: l.index,
        name: l.name.clone(),
        up: l.is_up(),
        carrier: l.carrier,
        mtu: l.mtu,
    }
}

fn route_of(r: &KRoute) -> netlink::route::Route {
    let family = match (r.dst, r.gateway) {
        (Some(d), _) => Family::of(d),
        (None, Some(g)) => Family::of(g),
        (None, None) => Family::V4,
    };
    let mut x = netlink::route::Route::new(family, r.dst, r.prefix);
    x.gateway = r.gateway;
    x.oif = r.link;
    x.mtu = r.mtu;
    x.tos = r.tos;
    x.metric = r.metric;
    if r.gateway.is_none() && r.link.is_some() {
        x.scope = netlink::route::consts::RT_SCOPE_LINK;
    }
    x
}

impl NetOps for Kernel {
    fn links(&mut self) -> NetResult<Vec<LinkInfo>> {
        Ok(Self::rtnl()?.links().map_err(err)?.iter().map(info).collect())
    }

    fn set_up(&mut self, index: u32, up: bool) -> NetResult<()> {
        Self::rtnl()?.set_up(index, up).map_err(err)
    }

    fn set_mtu(&mut self, index: u32, mtu: u32) -> NetResult<()> {
        Self::rtnl()?.set_mtu(index, mtu).map_err(err)
    }

    fn create_dummy(&mut self, name: &str) -> NetResult<LinkInfo> {
        Ok(info(&Self::rtnl()?.create_simple(name, "dummy").map_err(err)?))
    }

    fn create_vlan(&mut self, name: &str, parent: u32, id: u16) -> NetResult<LinkInfo> {
        Ok(info(&Self::rtnl()?.create_vlan(name, parent, id).map_err(err)?))
    }

    fn delete_link(&mut self, index: u32) -> NetResult<()> {
        Self::rtnl()?.delete_link(index).map_err(err)
    }

    fn addresses(&mut self) -> NetResult<Vec<KAddr>> {
        Ok(Self::rtnl()?
            .addresses(None)
            .map_err(err)?
            .into_iter()
            .map(|a| KAddr {
                link: a.index,
                addr: a.addr,
                prefix: a.prefix,
            })
            .collect())
    }

    fn add_address(&mut self, a: &KAddr) -> NetResult<()> {
        let mut x = Address::new(a.link, a.addr, a.prefix);
        if let IpAddr::V4(v4) = a.addr
            && a.prefix < 31
        {
            x.broadcast = Some(Address::default_broadcast(v4, a.prefix));
        }
        Self::rtnl()?.add_address(&x).map_err(err)
    }

    fn del_address(&mut self, a: &KAddr) -> NetResult<()> {
        Self::rtnl()?
            .delete_address(&Address::new(a.link, a.addr, a.prefix))
            .map_err(err)
    }

    fn routes(&mut self) -> NetResult<Vec<KRoute>> {
        Ok(Self::rtnl()?
            .routes(None)
            .map_err(err)?
            .into_iter()
            .filter(|r| r.table == netlink::route::consts::RT_TABLE_MAIN)
            .map(|r| KRoute {
                dst: r.dst,
                prefix: r.dst_len,
                gateway: r.gateway,
                link: r.oif,
                mtu: r.mtu,
                tos: r.tos,
                metric: r.metric,
                ours: r.protocol == netlink::route::consts::RTPROT_QS400,
            })
            .collect())
    }

    fn add_route(&mut self, r: &KRoute) -> NetResult<()> {
        Self::rtnl()?.add_route(&route_of(r)).map_err(err)
    }

    fn del_route(&mut self, r: &KRoute) -> NetResult<()> {
        Self::rtnl()?.delete_route(&route_of(r)).map_err(err)
    }

    fn sysctl_read(&mut self, key: &str) -> NetResult<String> {
        self.sysctl.read(key).map_err(err)
    }

    fn sysctl_write(&mut self, key: &str, value: &str) -> NetResult<()> {
        self.sysctl.write(key, value).map_err(err)
    }

    fn set_port_rules(&mut self, rules: &[(Proto, u16, u16)]) -> NetResult<()> {
        let mut n = nft::Nft::open().map_err(err)?;
        n.remove_all().map_err(err)?;
        for (p, first, last) in rules {
            let l4 = match p {
                Proto::Tcp => nft::L4::Tcp,
                Proto::Udp => nft::L4::Udp,
            };
            let rule = nft::PortRule::new(l4, *first, *last).map_err(err)?;
            n.add_port_drop(&rule).map_err(err)?;
        }
        Ok(())
    }

    fn set_hostname(&mut self, name: &str) -> NetResult<()> {
        std::fs::write("/proc/sys/kernel/hostname", format!("{name}\n"))
            .map_err(|e| NetError::new(format!("host name: {e}")))
    }
}
