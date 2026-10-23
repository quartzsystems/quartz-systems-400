// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Minimal rtnetlink: list links, set a link up, give a link an IPv4
//! address (Phase 8, for the TN5250E server).

pub mod msg;

use std::fmt::Write as _;

use rustix::io::Errno;
use rustix::net::netlink::SocketAddrNetlink;
use rustix::net::{AddressFamily, RecvFlags, SendFlags, SocketType, bind, recv, send, socket};

use msg::{
    IFF_UP, IFLA_IFNAME, NLM_F_ACK, NLM_F_CREATE, NLM_F_DUMP, NLM_F_EXCL, NLM_F_REQUEST,
    NLMSG_DONE, NLMSG_ERROR, RTM_GETLINK, RTM_NEWADDR, RTM_NEWLINK,
};

/// One network link.
#[derive(Debug, Clone)]
pub struct Link {
    /// Interface index.
    pub index: i32,
    /// Interface name.
    pub name: String,
    /// `IFF_*` flags.
    pub flags: u32,
}

impl Link {
    /// `IFF_UP` set.
    pub fn is_up(&self) -> bool {
        self.flags & IFF_UP != 0
    }
}

/// Every link, via an `RTM_GETLINK` dump.
pub fn links() -> Result<Vec<Link>, Errno> {
    let sock = open()?;
    let req = msg::link_request(RTM_GETLINK, NLM_F_REQUEST | NLM_F_DUMP, 1, 0, 0, 0);
    send(&sock, &req, SendFlags::empty())?;

    let mut links = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    'outer: loop {
        let (n, _) = recv(&sock, &mut buf[..], RecvFlags::empty())?;
        for m in msg::messages(&buf[..n]) {
            match m.kind {
                NLMSG_DONE => break 'outer,
                NLMSG_ERROR => return Err(m.error().unwrap_or(Errno::IO)),
                RTM_NEWLINK => {
                    if let Some((index, flags)) = m.ifinfo() {
                        let name = m
                            .attr(IFLA_IFNAME)
                            .map(|v| {
                                String::from_utf8_lossy(v)
                                    .trim_end_matches('\0')
                                    .to_string()
                            })
                            .unwrap_or_default();
                        links.push(Link { index, name, flags });
                    }
                }
                _ => {}
            }
        }
    }
    Ok(links)
}

/// Give a link an IPv4 address with a prefix length; an address
/// already there is not an error.
pub fn add_address(index: i32, ip: [u8; 4], prefix: u8) -> Result<(), Errno> {
    let sock = open()?;
    let req = msg::addr_request(
        RTM_NEWADDR,
        NLM_F_REQUEST | NLM_F_ACK | NLM_F_CREATE | NLM_F_EXCL,
        3,
        index,
        ip,
        prefix,
    );
    send(&sock, &req, SendFlags::empty())?;
    let mut buf = vec![0u8; 4096];
    let (n, _) = recv(&sock, &mut buf[..], RecvFlags::empty())?;
    for m in msg::messages(&buf[..n]) {
        if m.kind == NLMSG_ERROR {
            return match m.error() {
                None | Some(Errno::EXIST) => Ok(()),
                Some(e) => Err(e),
            };
        }
    }
    Ok(())
}

/// Set `IFF_UP` on a link.
pub fn set_up(index: i32) -> Result<(), Errno> {
    let sock = open()?;
    let req = msg::link_request(
        RTM_NEWLINK,
        NLM_F_REQUEST | NLM_F_ACK,
        2,
        index,
        IFF_UP,
        IFF_UP,
    );
    send(&sock, &req, SendFlags::empty())?;
    let mut buf = vec![0u8; 4096];
    let (n, _) = recv(&sock, &mut buf[..], RecvFlags::empty())?;
    for m in msg::messages(&buf[..n]) {
        if m.kind == NLMSG_ERROR {
            return match m.error() {
                Some(e) => Err(e),
                None => Ok(()),
            };
        }
    }
    Ok(())
}

/// Links as text.
pub fn dump(links: &[Link]) -> String {
    let mut s = String::from("Index  Link      State\n");
    for l in links {
        let _ = writeln!(
            s,
            "{:<6} {:<9} {}",
            l.index,
            l.name,
            if l.is_up() { "up" } else { "down" }
        );
    }
    s
}

fn open() -> Result<rustix::fd::OwnedFd, Errno> {
    // Protocol 0 is NETLINK_ROUTE; rustix has no constant for it.
    let sock = socket(AddressFamily::NETLINK, SocketType::RAW, None)?;
    bind(&sock, &SocketAddrNetlink::new(0, 0))?;
    Ok(sock)
}
