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
//! them; [`ifs`] carries the integrated file system the same way. The
//! service side lives in `slic`, which hosts the service.
//! `docs/STORAGE_SERVICE.md` describes the design.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod ifs;
mod link;
pub mod proto;
pub mod qsh;
pub mod remote;
pub mod tcp;

pub use link::{Link, Shared};
pub use proto::{Client, ExecReply, HostRefused, HostSignedOn, QapRefused, QapSession, Request, Service, Start, dispatch};
pub use wm::table::JobInfo;

/// Where the service listens for job connections.
pub const SOCKET: &str = "/run/qs400/stg";

/// The file whose presence asks the service to end job `number`'s
/// running SQL statement (QAP's cancel): the engine looks for it
/// between rows, the job makes it and removes it.
#[must_use]
pub fn sql_cancel_path(number: u32) -> String {
    format!("/run/qs400/sqlcancel/{number:06}")
}
