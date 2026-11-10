// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `NetServer` (`*NETSVR`): QS/400's glue around the vendored SMB2/3 server
//! (`net/smb-server`, rust-smb-server 0.10.0 with QS/400's patches). No
//! job, store or service code: the job implements the traits.
//!
//! - [`ifs`]: file shares over the job's [`::ifs::client::IfsClient`].
//! - [`signon`]: profile sign-on ([`signon::SignOn`]) and the library's
//!   verifier over it.
//! - [`print`]: print shares over the job's spooler ([`print::Printer`]).
//! - [`server`]: [`NetServer`], one job's server.
//! - [`nbss`]: the `NetBIOS` session request on port 139.
//! - [`route`]: the `ClientGuid` of a new connection, for routing
//!   reconnects.
//!
//! One `QZLSFILE` job serves one client (and its reconnects), so the
//! library's share modes, locks and leases see that client's opens only;
//! the IFS backend takes share modes and byte-range locks in the IFS too,
//! so other jobs see them, and puts its leases in the IFS's lease table
//! ([`ifs::lease`], [`NetServer::start_leases`]), so other jobs' changes
//! break them. `docs/NETSERVER.md` is the reference.

#![deny(missing_docs)]

pub mod ifs;
pub mod nbss;
pub mod print;
pub mod route;
pub mod server;
pub mod signon;

/// The library's tree admission and its sessions' view.
pub use smb_server::{SessionView, TreeAdmission, TreeRequest, TreeView};
pub use server::{BuildError, FileShare, NetServer, NetServerConfig, PrintShareCfg};

/// The library's parsers, for `tools/fuzz`.
#[cfg(feature = "fuzzing")]
pub use smb_server::fuzzing;
