// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Linux socket, through rustix.

use std::io;
use std::time::Duration;

use rustix::fd::OwnedFd;
use rustix::io::Errno;
use rustix::net::netlink::{self, SocketAddrNetlink};
use rustix::net::sockopt::{Timeout, set_socket_timeout};
use rustix::net::{
    AddressFamily, RecvFlags, SendFlags, SocketFlags, SocketType, bind, recv, send, socket_with,
};

use super::Bus;

/// A bound netlink socket.
pub struct Raw(OwnedFd);

impl Raw {
    /// Open and bind, joining `groups`.
    pub fn open(bus: Bus, groups: u32) -> io::Result<Self> {
        // NETLINK_ROUTE is protocol 0, which rustix spells `None`.
        let proto = match bus {
            Bus::Route => None,
            Bus::SockDiag => Some(netlink::SOCK_DIAG),
            Bus::Netfilter => Some(netlink::NETFILTER),
        };
        let fd = socket_with(
            AddressFamily::NETLINK,
            SocketType::RAW,
            SocketFlags::CLOEXEC,
            proto,
        )?;
        bind(&fd, &SocketAddrNetlink::new(0, groups))?;
        Ok(Self(fd))
    }

    /// Receive timeout.
    pub fn set_timeout(&self, t: Option<Duration>) -> io::Result<()> {
        Ok(set_socket_timeout(&self.0, Timeout::Recv, t)?)
    }

    /// Send to the kernel.
    pub fn send(&self, b: &[u8]) -> io::Result<()> {
        loop {
            match send(&self.0, b, SendFlags::empty()) {
                Ok(_) => return Ok(()),
                Err(Errno::INTR) => {}
                Err(e) => return Err(e.into()),
            }
        }
    }

    /// One datagram.
    pub fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            match recv(&self.0, &mut *buf, RecvFlags::empty()) {
                Ok((n, _)) => return Ok(n),
                Err(Errno::INTR) => {}
                Err(e) => return Err(e.into()),
            }
        }
    }
}
