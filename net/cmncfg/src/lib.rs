// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Communications configuration objects: the `*LIND` payload of an
//! Ethernet line ([`lind`]), the names of the network controller and
//! device a line gets ([`names`]), what a line means to the kernel
//! ([`link`]), the message identifiers the lines send ([`msgs`]) and
//! the runtime status of lines, controllers and devices ([`status`]),
//! driven by vary requests and the kernel's link events. Nothing here
//! touches the store or netlink: the storage service does, with what
//! [`status::State`] asks of it. `docs/TCPIP.md` has the design.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod lind;
pub mod link;
pub mod msgs;
pub mod names;
pub mod status;

pub use lind::{Lind, RcyLimit};
pub use status::{Effect, Kind, Row, State, Status};

/// Errors of the payload codecs.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The payload is not a line description, or is truncated.
    #[error("corrupt line description: {0}")]
    Corrupt(String),
}

/// A result of this crate.
pub type Result<T> = std::result::Result<T, Error>;

impl From<wm::Error> for Error {
    fn from(e: wm::Error) -> Self {
        Error::Corrupt(e.to_string())
    }
}
