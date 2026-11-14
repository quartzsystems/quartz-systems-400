// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The entry store over the object store: `*DIRDB` (docs/DIRSRV.md §5).
//!
//! - [`format`]: the attribute area and the trees' keys.
//! - [`record`]: entries, index definitions, change records.
//! - [`DirDb`]: one instance's store; its calls, all inside the caller's
//!   transaction, are in `db` (entries by ID), `names` (suffixes, DNs,
//!   add, rename, move, delete, children, subtrees), `index` and
//!   `changelog`.
//! - [`normalize`]: what the store asks of the schema.

mod changelog;
mod db;
pub mod error;
pub mod format;
mod index;
mod names;
pub mod normalize;
pub mod record;

pub use db::{CreateOpts, DirDb};
pub use error::{DirError, Result};
pub use format::{DirdbAttr, IndexKind};
pub use names::{ChildCursor, Page, Resolved, WalkCursor};
pub use normalize::{AsciiFold, NormError, Normalizer, Rule};
pub use record::{Attr, ChangeKind, ChangeRec, Entry, IndexDef};
