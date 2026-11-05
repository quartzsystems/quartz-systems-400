// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Triggers of physical files (`ADDPFTRG`, SQL's `CREATE TRIGGER`):
//! kept as `TRIGGER` entries of the file's `*SQLINF`
//! (`docs/DB_FORMAT.md` §8.1) and fired by the open data path around
//! every change and read of a record, whoever makes it (`fire`).
//!
//! A trigger program runs in the job that changes the record. The
//! database never calls one itself: an open data path either hands the
//! calls to a [`fire::Caller`] the job's side gave it, or returns them
//! for the job to make and resume ([`fire::Mode`]; `docs/STORAGE_SERVICE.md`
//! "Trigger programs").

pub mod buffer;
pub mod fire;
pub mod model;
pub mod ops;

pub use fire::{Call, Caller, Failure, Mode};
pub use model::{Event, MtAction, Threadsafe, Time, Trigger, UpdateCondition};

use std::sync::Arc;

use store::obj::ObjectId;
use store::txn::Txn;

use crate::error::Result;
use crate::file::FileDesc;
use crate::sqlinf::{self, tag};

/// Most triggers a file may have, as IBM allows.
pub const MAX_TRIGGERS: usize = 300;

/// The triggers of `file`, in the order kept (the order they were added).
pub fn list(txn: &mut Txn<'_>, file: ObjectId) -> Result<Vec<Trigger>> {
    let Some(info) = sqlinf::load(txn, file)? else {
        return Ok(Vec::new());
    };
    info.all(tag::TRIGGER).map(Trigger::decode).collect()
}

/// Replace the triggers of `file`.
pub fn store(txn: &mut Txn<'_>, file: ObjectId, trgs: &[Trigger], now: u64) -> Result<()> {
    let mut info = sqlinf::load(txn, file)?.unwrap_or_else(|| sqlinf::SqlInfo::new(sqlinf::Kind::Table));
    info.remove(tag::TRIGGER);
    for t in trgs {
        info.push(tag::TRIGGER, t.encode());
    }
    sqlinf::save(txn, file, &info, now)
}

/// A physical file's enabled triggers, as its members fire them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Active {
    /// Enabled triggers in the order they fire (the order added).
    pub triggers: Vec<Trigger>,
}

impl Active {
    /// The enabled triggers of a physical file, `None` when it has none.
    pub fn load(txn: &mut Txn<'_>, file: &FileDesc) -> Result<Option<Arc<Self>>> {
        if file.header.sql.is_none() {
            return Ok(None);
        }
        let mut triggers: Vec<Trigger> = list(txn, file.id)?.into_iter().filter(|t| t.enabled).collect();
        if triggers.is_empty() {
            return Ok(None);
        }
        triggers.sort_by_key(|t| t.number);
        Ok(Some(Arc::new(Self { triggers })))
    }

    /// Those that fire for `event` at `time`.
    pub fn matching(&self, time: Time, event: Event) -> impl Iterator<Item = &Trigger> {
        self.triggers.iter().filter(move |t| t.time == time && t.event == event)
    }

    /// Whether any fires for `event`.
    #[must_use]
    pub fn any(&self, event: Event) -> bool {
        self.triggers.iter().any(|t| t.event == event)
    }
}
