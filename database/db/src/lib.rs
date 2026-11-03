// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The QS/400 database on the object store: `*FILE` objects with their
//! record formats and members, record slots, field values and access
//! paths. The on-disk format is `docs/DB_FORMAT.md`; `tests/spec.rs`
//! checks the layouts here against it.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod auth;
pub mod chg;
pub mod cmt;
pub mod dsp;
mod error;
pub mod file;
pub mod format;
pub mod journal;
pub mod lf;
pub mod member;
pub mod odp;
pub mod path;
pub mod record;
pub mod source;
pub mod spec;
pub mod util;

pub use error::{DbError, Result};
pub use file::{CreateFileOpts, FileDesc, FileKind, Maint, Recover, Size, Wait};
pub use format::{FormatDef, fmt_level_id};
pub use member::{MbrAttr, Member, Slot};
pub use odp::{
    IoFeedback, KeyArg, Odp, OdpRef, OpenOpts, Override, Position, Record, Session, SharedLocks,
    Usage,
};
pub use path::{Path, PathKey};
pub use record::Value;

/// Called between the steps of a long operation (a path built, records
/// converted, a rollback undone) with its transaction. It may commit
/// what was done so far and give the store to other jobs: `Ok(true)`
/// when it did, and what the caller read from the store is read again;
/// the caller holds the locks that keep the objects it works on as they
/// were. [`DbError::Ended`] ends the operation.
pub type Pause<'a> = &'a mut dyn FnMut(&mut store::txn::Txn<'_>) -> Result<bool>;

/// A [`Pause`] that never pauses.
///
/// # Errors
///
/// None.
pub fn no_pause(_: &mut store::txn::Txn<'_>) -> Result<bool> {
    Ok(false)
}

/// Steps between two calls of a [`Pause`].
pub const PAUSE_EVERY: u64 = 256;
