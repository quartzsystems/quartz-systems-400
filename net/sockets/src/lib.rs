// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! IBM's socket API semantics over the Linux kernel. Programs see IBM's
//! numbers ([`consts`]), structure layouts ([`layout`]) and `errno`
//! values ([`Errno`]); the kernel sees its own, translated in [`sys`]
//! and nowhere else. [`Sockets`] is one job's set of open sockets: it
//! checks port restrictions on every `bind` ([`PortRules`]), marks bound
//! sockets for the nftables backstop (`SO_MARK`), maps `AF_UNIX` paths
//! into the IFS ([`IfsPathMap`]) and passes descriptors between jobs
//! ([`DescriptorPassing`]). [`resolver`] is the stub resolver behind
//! `gethostbyname` and its kin.
//!
//! Off Linux every socket call is `ENOSYS`; the constants, layouts,
//! codecs and the resolver's host table still work there.

#![deny(missing_docs)]

pub mod consts;
pub mod layout;
pub mod pass;
pub mod paths;
pub mod resolver;
pub mod rules;
mod sock;
pub mod sys;

pub use ifs::Errno;
pub use layout::addr::{AddrForm, SockAddr};
pub use pass::{DescriptorPassing, InProcess, JobId};
pub use paths::{IfsPathMap, RootedPaths, UnixUse};
pub use rules::{AllowAll, PortRules, Restriction, Restrictions};
pub use sock::Sockets;
pub use sys::{Fd, Info, Received};

/// The IBM `strerror` text of an `errno` value (Quartz-written).
#[must_use]
pub fn strerror(code: i32) -> String {
    match Errno::from_code(code) {
        Some(e) => e.text().to_string(),
        None if code == 0 => "No error has been reported.".to_string(),
        None => format!("The error number {code} is not known."),
    }
}

/// `htonl`: IBM's is the identity, and so is QS/400's, because every
/// integer in job storage is big-endian (`ABI.md` §1): a value stored
/// into `sin_addr` is already in network order.
#[must_use]
pub fn htonl(v: u32) -> u32 {
    v
}

/// `htons`: the identity, as [`htonl`].
#[must_use]
pub fn htons(v: u16) -> u16 {
    v
}

/// `ntohl`: the identity, as [`htonl`].
#[must_use]
pub fn ntohl(v: u32) -> u32 {
    v
}

/// `ntohs`: the identity, as [`htonl`].
#[must_use]
pub fn ntohs(v: u16) -> u16 {
    v
}
