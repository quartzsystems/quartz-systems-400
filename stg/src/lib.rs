// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The storage service protocol.
//!
//! The storage service (`qsstg`) owns the object store, the database
//! and every lock; job processes never touch a page. [`proto`] declares
//! the requests; [`Link`] carries them; [`remote`] implements the
//! runtime's source traits (`ObjectSource`, `IoSource`, `DataSource`,
//! `UserSpaceSource`, `MsgLookup`, the APIs' `Provider`) by sending
//! them. The service side lives in `slic`, which hosts the service.
//! `docs/STORAGE_SERVICE.md` describes the design.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod link;
pub mod proto;
pub mod remote;

pub use link::{Link, Shared};
pub use proto::{Client, ExecReply, Request, Service, Start, dispatch};
pub use wm::table::JobInfo;

/// Where the service listens for job connections.
pub const SOCKET: &str = "/run/qs400/stg";
