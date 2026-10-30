// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The network tools: `PING`/`VFYTCPCNN`, `TRACEROUTE`/`TRCTCPRTE`,
//! the `NETSTAT` views and the communications trace.
//!
//! - [`icmp`]: ICMP echo packets, v4 and v6, and the checksum.
//! - [`cmsg`]: ancillary data the kernel hands back (`IP_TTL`,
//!   `IP_RECVERR`'s `sock_extended_err`), parsed from bytes.
//! - [`ping`]: the echo loop and its messages, over an [`ping::Echoer`].
//! - [`route`]: the hop-by-hop probe loop, over a [`route::Prober`].
//! - [`netstat`]: the `NETSTAT` lists and displays from what the kernel
//!   reports, as panel variables.
//! - [`cmntrc`]: the trace store, the frame decoder and the
//!   `PRTCMNTRC` report.
//! - [`sys`]: the Linux sockets (ICMP datagram, UDP with `IP_RECVERR`,
//!   `AF_PACKET`); elsewhere every call fails.
//!
//! The logic is pure and host-tested; only [`sys`] touches the kernel.

#![deny(missing_docs)]
#![deny(unsafe_code)]

pub mod cmntrc;
pub mod cmsg;
pub mod icmp;
pub mod msg;
pub mod netstat;
pub mod ping;
pub mod route;
pub mod sys;

pub use msg::Msg;
