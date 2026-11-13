// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The server side every protocol shares (docs/NFS.md section 5), with
//! no job, store or service code:
//!
//! - [`vfs`]: the [`Vfs`] trait the protocols call, handle-based and
//!   with the caller explicit in each [`Req`]; [`error`] its errors.
//! - [`ifsvfs`]: [`IfsVfs`], the [`Vfs`] over an `IfsClient`.
//! - [`handle`]: the 32-byte file handle.
//! - [`export`]: what the server needs of an export ([`ExportView`],
//!   [`ExportTable`]) and an in-memory table.
//! - [`gate`]: the check every call passes; [`cred`] the caller.
//! - [`text`]: data and path name conversion per host.
//! - [`instance`]: a start's write verifier, clocks.
//! - [`news`]: the poll for granted locks and lease breaks.
//! - [`locate`]: where another host's programs listen.
//! - [`run`]: the daemons' loops: `nfsd`, `mountd`, `nlmd`, `nsmd`.
//!
//! A daemon thread owns its [`Vfs`] (the IFS client is not `Send`);
//! what threads share is behind `Arc`s: the export table, the
//! duplicate request cache, the [`instance::Instance`], the lock
//! manager's and status monitor's tables.

pub mod cred;
pub mod error;
pub mod export;
pub mod gate;
pub mod handle;
pub mod ifsvfs;
pub mod instance;
pub mod locate;
pub mod news;
// pub mod run;
pub mod text;
pub mod vfs;

pub use error::{VResult, VfsError};
pub use export::{Anon, ExportTable, ExportView, HostOpts, HostPat, MemExport, MemExports};
pub use gate::Gate;
pub use handle::FileHandle;
pub use ifsvfs::IfsVfs;
pub use instance::{Clock, Instance, ManualClock, SystemClock};
pub use vfs::*;
