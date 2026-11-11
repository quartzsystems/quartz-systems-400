// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Who hears of changes to libraries, files and members. The store does
//! not know who caches them; the storage service installs a [`Watcher`]
//! (SMB leases and change notify on `/QSYS.LIB`, `docs/NETSERVER.md`
//! section 6). The store reports libraries and files created, deleted,
//! renamed and moved; the database reports members opened for change,
//! their records changed, and members added, removed and renamed. A store
//! without a watcher, or with one that hears nothing now
//! ([`Watcher::active`]), costs one check.

use std::sync::Arc;

use qsname::Name;

use crate::Result;
use crate::obj::{JobHandle, ObjectId};
use crate::txn::Txn;

/// What happened to an object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// A member is about to be opened for change. The only change that
    /// may be refused: `StoreError::Deferred` when the open has to wait.
    Opening,
    /// A member's records changed.
    Data,
    /// The object was made.
    Added,
    /// The object is about to go.
    Removing,
    /// The object was renamed from `from`.
    Renamed {
        /// Its old name.
        from: Name,
    },
    /// The object was moved from the library `from`.
    Moved {
        /// Its old library.
        from: ObjectId,
    },
}

/// What the store tells about libraries, files and members.
pub trait Watcher: Send + Sync {
    /// Whether anything is to be told now (a cheap check).
    fn active(&self) -> bool;

    /// `job` did `what` to `id`.
    ///
    /// # Errors
    ///
    /// `StoreError::Deferred` for an [`Change::Opening`] that waits.
    fn changed(&self, txn: &mut Txn<'_>, job: JobHandle, id: ObjectId, what: Change) -> Result<()>;
}

/// The watcher to tell now, if any.
fn watcher(txn: &mut Txn<'_>) -> Option<Arc<dyn Watcher>> {
    txn.store().watcher().filter(|w| w.active())
}

/// Whether a change would be told: callers skip work for none.
pub fn active(txn: &mut Txn<'_>) -> bool {
    watcher(txn).is_some()
}

/// Tell the watcher that `job` did `what` to `id`.
///
/// # Errors
///
/// The watcher's refusal of an [`Change::Opening`].
pub fn changed(txn: &mut Txn<'_>, job: JobHandle, id: ObjectId, what: Change) -> Result<()> {
    match watcher(txn) {
        Some(w) => w.changed(txn, job, id, what),
        None => Ok(()),
    }
}
