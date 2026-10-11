// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The QS/400 database on the object store: `*FILE` objects with their
//! record formats and members, record slots, field values and access
//! paths. The on-disk format is `docs/DB_FORMAT.md`; `tests/spec.rs`
//! checks the layouts here against it.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod chg;
pub mod dsp;
mod error;
pub mod file;
pub mod format;
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
