// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `NETLINK_ROUTE`: links, bonds, VLANs, addresses, routes, neighbors
//! and link events. [`Rtnl`] is the client; the submodules hold the
//! request builders and reply parsers it uses.

pub mod addr;
pub mod consts;
pub mod link;
pub mod neigh;
pub mod rt;

use std::time::Duration;

pub use addr::Address;
pub use link::{BondMode, BondSpec, Link, LinkChange, LinkStats, OperState};
pub use neigh::Neighbor;
pub use rt::Route;

use crate::codec::Msg;
use crate::{Bus, Error, Family, Result, Socket};
use consts::{RTM_DELLINK, RTM_NEWADDR, RTM_NEWLINK, RTM_NEWNEIGH, RTM_NEWROUTE, RTMGRP_LINK};

/// A route-netlink client.
pub struct Rtnl {
    sock: Socket,
}

impl Rtnl {
    /// Open a request socket.
    pub fn open() -> Result<Self> {
        Ok(Self {
            sock: Socket::open(Bus::Route)?,
        })
    }

    /// Every link.
    pub fn links(&mut self) -> Result<Vec<Link>> {
        self.sock.dump(&link::dump_request(), |m| {
            (m.kind == RTM_NEWLINK).then(|| link::parse(m.payload)).transpose()
        })
    }

    /// One link by name.
    pub fn link_by_name(&mut self, name: &str) -> Result<Link> {
        self.get_link(&link::get_by_name(name))
            .map_err(|e| not_found(e, name))
    }

    /// One link by index.
    pub fn link_by_index(&mut self, index: u32) -> Result<Link> {
        self.get_link(&link::get_by_index(index))
            .map_err(|e| not_found(e, &format!("link index {index}")))
    }

    fn get_link(&mut self, req: &crate::codec::Builder) -> Result<Link> {
        let mut out = None;
        self.sock.request(req, |m| {
            if m.kind == RTM_NEWLINK {
                out = Some(link::parse(m.payload)?);
            }
            Ok(())
        })?;
        out.ok_or(Error::Malformed("no link in reply"))
    }

    /// Apply a [`LinkChange`].
    pub fn change(&mut self, index: u32, ch: &LinkChange) -> Result<()> {
        self.sock.ack(&link::set_request(index, ch))
    }

    /// Bring a link up or down.
    pub fn set_up(&mut self, index: u32, up: bool) -> Result<()> {
        self.change(
            index,
            &LinkChange {
                up: Some(up),
                ..LinkChange::default()
            },
        )
    }

    /// Set the MTU.
    pub fn set_mtu(&mut self, index: u32, mtu: u32) -> Result<()> {
        self.change(
            index,
            &LinkChange {
                mtu: Some(mtu),
                ..LinkChange::default()
            },
        )
    }

    /// Set the hardware address.
    pub fn set_mac(&mut self, index: u32, mac: &[u8]) -> Result<()> {
        self.change(
            index,
            &LinkChange {
                mac: Some(mac.to_vec()),
                ..LinkChange::default()
            },
        )
    }

    /// Create a bond and return it.
    pub fn create_bond(&mut self, name: &str, spec: &BondSpec) -> Result<Link> {
        self.sock.ack(&link::new_bond_request(name, spec))?;
        self.link_by_name(name)
    }

    /// Enslave `slave` to `bond`. The slave is taken down first, as the
    /// bonding driver requires, and brought back up if it was up.
    pub fn enslave(&mut self, slave: u32, bond: u32) -> Result<()> {
        let was_up = self.link_by_index(slave)?.is_up();
        if was_up {
            self.set_up(slave, false)?;
        }
        self.change(
            slave,
            &LinkChange {
                master: Some(bond),
                ..LinkChange::default()
            },
        )?;
        if was_up {
            self.set_up(slave, true)?;
        }
        Ok(())
    }

    /// Release `slave` from its bond.
    pub fn release(&mut self, slave: u32) -> Result<()> {
        self.change(
            slave,
            &LinkChange {
                master: Some(0),
                ..LinkChange::default()
            },
        )
    }

    /// Create an 802.1Q sub-interface and return it.
    pub fn create_vlan(&mut self, name: &str, parent: u32, vlan_id: u16) -> Result<Link> {
        self.sock.ack(&link::new_vlan_request(name, parent, vlan_id)?)?;
        self.link_by_name(name)
    }

    /// Create a link of a kind that takes no data (`dummy`).
    pub fn create_simple(&mut self, name: &str, kind: &str) -> Result<Link> {
        self.sock.ack(&link::new_simple_request(name, kind))?;
        self.link_by_name(name)
    }

    /// Delete a link (bonds, VLANs, virtual links).
    pub fn delete_link(&mut self, index: u32) -> Result<()> {
        self.sock.ack(&link::del_request(index))
    }

    /// Addresses of one family, or all.
    pub fn addresses(&mut self, family: Option<Family>) -> Result<Vec<Address>> {
        self.sock.dump(&addr::dump_request(family), |m| {
            if m.kind == RTM_NEWADDR {
                addr::parse(m.payload)
            } else {
                Ok(None)
            }
        })
    }

    /// Add an address.
    pub fn add_address(&mut self, a: &Address) -> Result<()> {
        self.sock.ack(&addr::request(a, true)?)
    }

    /// Delete an address.
    pub fn delete_address(&mut self, a: &Address) -> Result<()> {
        self.sock.ack(&addr::request(a, false)?)
    }

    /// Routes of one family, or all, in every table.
    pub fn routes(&mut self, family: Option<Family>) -> Result<Vec<Route>> {
        self.sock.dump(&rt::dump_request(family), |m| {
            if m.kind == RTM_NEWROUTE {
                rt::parse(m.payload)
            } else {
                Ok(None)
            }
        })
    }

    /// Add a route.
    pub fn add_route(&mut self, r: &Route) -> Result<()> {
        self.sock.ack(&rt::request(r, true)?)
    }

    /// Delete a route.
    pub fn delete_route(&mut self, r: &Route) -> Result<()> {
        self.sock.ack(&rt::request(r, false)?)
    }

    /// The neighbor cache of one family, or all.
    pub fn neighbors(&mut self, family: Option<Family>) -> Result<Vec<Neighbor>> {
        self.sock.dump(&neigh::dump_request(family), |m| {
            if m.kind == RTM_NEWNEIGH {
                neigh::parse(m.payload)
            } else {
                Ok(None)
            }
        })
    }
}

fn not_found(e: Error, what: &str) -> Error {
    if e.is_not_found() {
        Error::NotFound(what.to_string())
    } else {
        e
    }
}

/// A link change the kernel announced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkEvent {
    /// The link was removed (`RTM_DELLINK`).
    pub removed: bool,
    /// The link as it is now.
    pub link: Link,
}

/// A subscription to `RTNLGRP_LINK`: link added, removed, up/down and
/// carrier changes.
pub struct LinkEvents {
    sock: Socket,
}

impl LinkEvents {
    /// Subscribe.
    pub fn open() -> Result<Self> {
        Ok(Self {
            sock: Socket::subscribe(Bus::Route, RTMGRP_LINK)?,
        })
    }

    /// Wait at most `timeout` in [`LinkEvents::wait`] (`None` forever).
    pub fn set_timeout(&self, timeout: Option<Duration>) -> Result<()> {
        self.sock.set_timeout(timeout)
    }

    /// The events of the next datagram.
    pub fn wait(&mut self) -> Result<Vec<LinkEvent>> {
        let buf = self.sock.recv()?;
        parse_events(buf)
    }
}

/// The link events in a datagram.
pub fn parse_events(buf: &[u8]) -> Result<Vec<LinkEvent>> {
    let mut out = Vec::new();
    for m in crate::codec::messages(buf) {
        let m: Msg<'_> = m?;
        if m.kind == RTM_NEWLINK || m.kind == RTM_DELLINK {
            out.push(LinkEvent {
                removed: m.kind == RTM_DELLINK,
                link: link::parse(m.payload)?,
            });
        }
    }
    Ok(out)
}
