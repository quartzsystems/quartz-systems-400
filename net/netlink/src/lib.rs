// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The kernel-facing network layer: a netlink client written from
//! scratch, plus the `/proc` and `/proc/sys/net` readers.
//!
//! - [`codec`]: `nlmsghdr` and attribute building and parsing. Pure,
//!   host-testable.
//! - [`Socket`]: a netlink socket on one bus. Linux only; elsewhere
//!   [`Socket::open`] fails with [`Error::Unsupported`].
//! - [`route`]: `NETLINK_ROUTE` links, bonds, VLANs, addresses, routes,
//!   neighbors and link events.
//! - [`diag`]: `NETLINK_SOCK_DIAG` TCP and UDP sockets, `SOCK_DESTROY`.
//! - [`nft`]: the `qs400` nftables table that backs port restrictions.
//! - [`sysctl`]: `/proc/sys/net` and the `CHGTCPA` mappings.
//! - [`procnet`]: `/proc/net/{tcp,udp}{,6}` and socket inode owners.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod codec;
pub mod diag;
mod error;
pub mod nft;
pub mod procnet;
pub mod route;
mod sock;
pub mod sysctl;

pub use error::{Error, Result};
pub use sock::{Bus, Socket};

/// `AF_INET`.
pub const AF_INET: u8 = 2;
/// `AF_INET6`.
pub const AF_INET6: u8 = 10;
/// `AF_UNSPEC`.
pub const AF_UNSPEC: u8 = 0;

/// An address family for requests that take one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Family {
    /// IPv4.
    V4,
    /// IPv6.
    V6,
}

impl Family {
    /// The `AF_*` number.
    pub fn af(self) -> u8 {
        match self {
            Family::V4 => AF_INET,
            Family::V6 => AF_INET6,
        }
    }

    /// From an `AF_*` number.
    pub fn from_af(af: u8) -> Option<Self> {
        match af {
            AF_INET => Some(Family::V4),
            AF_INET6 => Some(Family::V6),
            _ => None,
        }
    }

    /// The family of an address.
    pub fn of(ip: std::net::IpAddr) -> Self {
        if ip.is_ipv4() { Family::V4 } else { Family::V6 }
    }
}
