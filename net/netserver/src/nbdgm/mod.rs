// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `NetBIOS` datagram service on UDP 138 (RFC 1001 17, RFC 1002 4.4),
//! as a B node, for the one thing a server sends there: host announcements
//! to the workgroup's master browser ([MS-BRWS]), so the server shows in
//! network lists that are still built by browsing. Run by `QZLSSERVER`
//! beside the name service ([`crate::nbns`]) when the "allow `NetBIOS`"
//! attribute is on, every browse interval.
//!
//! - [`packet`]: datagrams, the SMB mailslot write inside them, the
//!   `HostAnnouncement` and `AnnouncementRequest` frames.
//! - [`service`]: when to announce ([`Announcer`]).
//! - [`udp`]: the loop over the name service's [`crate::nbns::Datagrams`].
//!
//! The server is not a browser: no elections, no browse list (clients
//! fetch one over SMB1, which is refused), no backup lists.

pub mod packet;
pub mod service;
pub mod udp;

pub use packet::Datagram;
pub use service::{AnnounceConfig, Announcer};
pub use udp::serve;

/// Why a datagram was not read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum NbdgmError {
    /// It ends inside a field.
    #[error("truncated datagram")]
    Truncated,
    /// A name is malformed.
    #[error("malformed NetBIOS name")]
    BadName,
    /// Not a whole direct or broadcast datagram (an error, a query, a
    /// fragment).
    #[error("not a datagram the server reads")]
    NotData,
    /// Not a mailslot write.
    #[error("not a mailslot write")]
    NotMailslot,
    /// Longer than a datagram.
    #[error("datagram too long")]
    TooLong,
}
