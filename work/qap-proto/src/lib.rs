// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Quartz Access Protocol, versions 1 to 9 (docs/QAP.md; versions 7
//! to 9 are drafts until they ship): how QS/400
//! Access talks to QS/400. Every frame is one Wire-encoded [`Envelope`]
//! in an `ipc` length-prefixed frame of at most [`MAX_FRAME`] bytes.
//!
//! - [`envelope`]: the envelope, its kinds, services, statuses, flags,
//!   the server's messages, framing and replies split with `More`.
//! - [`ops`]: the operations of each service and their bodies.
//! - [`types`]: types several services share.
//! - [`secret`]: passwords and tokens, never printed and wiped on drop.
//!
//! Every value is defined from one protocol version (docs/QAP.md,
//! "Compatibility of new values"): [`Op::since`], [`Service::since`],
//! [`Status::since`], [`ops::event::since`]. A peer is sent only what
//! its negotiated version defines.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod envelope;
pub mod ops;
pub mod secret;
pub mod types;

mod error;

pub use envelope::{Envelope, Kind, Reassembly, ServerMessage, Service, Status};
pub use error::ProtoError;
pub use ops::Op;
pub use secret::{Secret, Token};

use std::time::Duration;

/// The newest protocol version this crate speaks.
pub const VERSION: u16 = 9;
/// The oldest.
pub const MIN_VERSION: u16 = 1;
/// The server's port unless configured otherwise.
pub const DEFAULT_PORT: u16 = 2320;
/// The longest encoded envelope: 1 MiB, not counting the length prefix.
pub const MAX_FRAME: usize = 1 << 20;
/// Requests a connection may have outstanding; beyond, `Busy`.
pub const MAX_OUTSTANDING: usize = 64;
/// The client pings after this long without traffic.
pub const PING_AFTER: Duration = Duration::from_secs(60);
/// The server closes a connection after this long without frames.
pub const IDLE_CLOSE: Duration = Duration::from_secs(300);
/// The request ID `Hello` must carry.
pub const HELLO_ID: u32 = 1;
