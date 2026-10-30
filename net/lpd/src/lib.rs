// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The line printer daemon of `*LPD` (docs/SERVERS.md "`*LPD`"), without
//! the store or the job:
//!
//! - [`proto`]: RFC 1179's daemon commands and receive-job
//!   subcommands, read and written.
//! - [`session`]: one connection, served against a [`Spooler`] (the
//!   output queues).
//! - [`convert`]: a received job (control file and data files) as
//!   spooled files: SCS for text, the bytes as they came otherwise,
//!   the sender's attributes when a QS/400 sent it.
//! - [`status`]: the queue-state answer.
//!
//! The client side is `work/print`'s LPR sink.

#![deny(missing_docs)]

pub mod convert;
pub mod proto;
pub mod session;
pub mod status;

pub use convert::Spooled;
pub use proto::{Command, Sub};
pub use session::{Job, Spooler, serve};

/// The LPD port.
pub const PORT: u16 = 515;
