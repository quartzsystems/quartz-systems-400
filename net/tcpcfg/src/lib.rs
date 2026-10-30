// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! TCP/IP configuration and operation, without the store or the kernel:
//!
//! - [`model`]: interfaces, routes, the host table, the domain, the
//!   `CHGTCPA` attributes, port restrictions, servers and the related
//!   tables, with the commands' rules (`ADD`, `CHG`, `RMV`, `MRG`, `RNM`).
//! - [`files`]: the `QUSRSYS` physical files that hold them, in IBM's
//!   record formats where IBM's are known, and the record codecs.
//! - [`Config`]: everything loaded at once.
//! - [`state`]: what is running (`STRTCP`, interface and route status),
//!   held by the storage service like the job table.
//! - [`net`]: the kernel operations the engine calls ([`net::NetOps`]),
//!   with a fake for tests and the netlink adapter on Linux.
//! - [`apply`]: `STRTCP`, `ENDTCP`, `STRTCPIFC`, `ENDTCPIFC`, routes,
//!   sysctls and port rules applied through [`net::NetOps`].
//! - [`line`], [`server`]: what other parts of the system answer (line
//!   descriptions, the servers' start and end).
//! - [`resolver`]: [`resolver::HostConfig`] and the services table for
//!   the sockets layer.

#![deny(missing_docs)]

pub mod addr;
pub mod apply;
mod config;
mod error;
pub mod files;
pub mod line;
pub mod model;
pub mod msg;
pub mod net;
pub mod panel;
pub mod resolver;
pub mod server;
pub mod state;
mod wire;

pub use config::Config;
pub use error::{Error, Result};
pub use state::State;
