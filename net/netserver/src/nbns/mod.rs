// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `NetBIOS` name service on UDP 137 (RFC 1001/1002), as a B node,
//! for clients that find the server by its `NetBIOS` name: run by
//! `QZLSSERVER` when the "allow `NetBIOS`" attribute is on.
//!
//! - [`packet`]: the packets, names encoded as the session service's
//!   ([`crate::nbss::name`]).
//! - [`service`]: the names held (the server name `<00>` and `<20>`,
//!   unique; the workgroup `<00>`, group), registration, answers to name
//!   queries and node status requests, defence, refresh and release.
//! - [`udp`]: the loop over a [`udp::Datagrams`] and UDP 137 itself.
//!
//! The datagram service on UDP 138 is not built: file serving does not use
//! it, only browsing (the browse list, master browser elections) does,
//! and browsing is refused (`docs/NETSERVER.md` section 10). There is no
//! WINS client: names are registered by broadcast only.

pub mod packet;
pub mod service;
pub mod udp;

pub use packet::Packet;
pub use service::{NameConfig, NameService};
pub use udp::{Datagrams, Udp, serve};

/// Why a packet was not read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum NbnsError {
    /// The packet ends inside a field.
    #[error("truncated name service packet")]
    Truncated,
    /// A name is malformed.
    #[error("malformed NetBIOS name")]
    BadName,
    /// A label pointer does not point back at a full name.
    #[error("bad label pointer")]
    BadPointer,
    /// A section claims more records than a name service packet carries.
    #[error("too many records")]
    TooMany,
    /// Longer than a name service packet.
    #[error("packet too long")]
    TooLong,
}
