// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Journaling: `*JRN` and `*JRNRCV` objects in the store, the entry
//! layout, appending in the caller's transaction so the journal and the
//! data commit together, and reading entries back along the receiver
//! chain. What is journaled, and when, is the database's business (the
//! `db` crate); this crate only keeps the entries.

#![deny(missing_docs)]

mod error;

pub mod dsp;
pub mod entry;
pub mod journal;
pub mod rcv;
pub mod spec;

pub use dsp::{ChainCursor, Filter, dspjrn, dspjrnrcva, render};
pub use entry::{Draft, Entry, JobInfo, ObjectRef, RecordImage, code};
pub use error::{JrnError, Result};
pub use journal::{CrtJrnOpts, Journal, JrnAttr, JrnObjEntry, NewReceiver, RcvDirEntry, RcvStatus};
pub use rcv::{Cursor, RcvAttr, Receiver};

/// Type code of `*JRN`.
pub const JRN: u16 = 0x0018;
/// Type code of `*JRNRCV`.
pub const JRNRCV: u16 = 0x0019;
