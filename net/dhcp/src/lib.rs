// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The DHCP client (IPv4) behind `ADDTCPIFC INTNETADR(*IP4DHCP)`, the IPL's
//! `qs400.ip=dhcp`, a network install and the response file's `ip = *IP4DHCP`:
//!
//! - [`packet`]: the BOOTP/DHCP message and its options (RFC 2131,
//!   RFC 2132).
//! - [`client`]: the client's states as a pure machine: `DISCOVER`,
//!   `OFFER`, `REQUEST`, `ACK`, renewal at T1, rebinding at T2, expiry,
//!   `INIT-REBOOT` with a kept lease, `RELEASE`.
//! - [`lease`]: a lease, its times and the text it is kept in.
//! - [`sys`]: the UDP socket on one link (Linux; a stub elsewhere).
//! - [`run`]: [`run::acquire`] (one lease, blocking) and [`run::Agent`]
//!   (a thread that keeps a lease and applies it through a
//!   [`run::Sink`]).

#![deny(missing_docs)]

pub mod client;
pub mod lease;
pub mod packet;
pub mod run;
pub mod sys;

pub use client::{Client, Config, Dest, Out};
pub use lease::Lease;
pub use packet::{Kind, Message};

/// A failure of the client.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A message that is not DHCP.
    #[error("not a DHCP message: {0}")]
    Malformed(&'static str),
    /// The socket.
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// No server answered in time.
    #[error("no DHCP server answered on {0}")]
    NoAnswer(String),
}
