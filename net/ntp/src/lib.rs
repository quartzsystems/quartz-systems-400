// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SNTP for `*NTP` (docs/SERVERS.md "`*NTP`"): RFC 4330 over the RFC
//! 5905 packet, without the store or the job.
//!
//! - [`packet`]: the 48-byte header and its timestamps.
//! - [`client`]: one exchange with a time server and its offset and
//!   delay; trying `CHGNTPA`'s servers in order.
//! - [`adjust`]: what the client does with an offset under `MINADJ`,
//!   `ADJTHLD` and `MAXADJ`.
//! - [`server`]: the answer to a client's request, and the UDP loop.
//! - [`poll`]: one poll of the client over its servers.
//! - [`clock`]: the clock the client reads and adjusts.

#![deny(missing_docs)]

pub mod adjust;
pub mod client;
pub mod clock;
pub mod packet;
pub mod poll;
pub mod server;

pub use adjust::{Decision, Policy};
pub use client::{Sample, SntpError};
pub use clock::Clock;
pub use packet::{Leap, Mode, Packet, Timestamp};

/// The NTP port.
pub const PORT: u16 = 123;
