// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! How the view reaches the store: one transaction per call, committed
//! when the call succeeds, with the job the request runs for and the
//! system values it may need.

use std::collections::BTreeMap;
use std::sync::Mutex;

use ifs::Errno;
use store::Store;
use store::obj::{JobHandle, ObjectId};
use store::txn::Txn;

/// The store as the view needs it.
pub trait StoreAccess {
    /// Run `f` in a transaction on the main store with `QSYS`,
    /// committed when it succeeds and aborted when it fails.
    fn with_main<T>(
        &self,
        f: &mut dyn FnMut(&mut Txn<'_>, ObjectId) -> Result<T, Errno>,
    ) -> Result<T, Errno>;
    /// The lock handle of the job whose request is being answered.
    fn job(&self) -> JobHandle;
    /// A system value's text (`QCRTOBJAUD`, `QCCSID`).
    fn sysval(&self, name: &str) -> Option<String>;
    /// The time, Unix seconds.
    fn now(&self) -> u64;
}

impl<S: StoreAccess> StoreAccess for std::sync::Arc<S> {
    fn with_main<T>(
        &self,
        f: &mut dyn FnMut(&mut Txn<'_>, ObjectId) -> Result<T, Errno>,
    ) -> Result<T, Errno> {
        (**self).with_main(f)
    }
    fn job(&self) -> JobHandle {
        (**self).job()
    }
    fn sysval(&self, name: &str) -> Option<String> {
        (**self).sysval(name)
    }
    fn now(&self) -> u64 {
        (**self).now()
    }
}

/// A store in memory, for tests and tools.
pub struct MemAccess {
    /// The store.
    pub store: Mutex<Store>,
    /// `QSYS`.
    pub qsys: ObjectId,
    /// The job.
    pub job: JobHandle,
    /// System values.
    pub sysvals: Mutex<BTreeMap<String, String>>,
}

impl MemAccess {
    /// Over `store` with its `QSYS`.
    #[must_use]
    pub fn new(store: Store, qsys: ObjectId) -> Self {
        Self {
            store: Mutex::new(store),
            qsys,
            job: JobHandle(1),
            sysvals: Mutex::new(BTreeMap::new()),
        }
    }

    /// Set a system value.
    pub fn set_sysval(&self, name: &str, value: &str) {
        self.sysvals
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(name.into(), value.into());
    }
}

impl StoreAccess for MemAccess {
    fn with_main<T>(
        &self,
        f: &mut dyn FnMut(&mut Txn<'_>, ObjectId) -> Result<T, Errno>,
    ) -> Result<T, Errno> {
        let mut s = self
            .store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut txn = s.begin().map_err(|_| Errno::EIO)?;
        match f(&mut txn, self.qsys) {
            Ok(v) => {
                txn.commit().map_err(|_| Errno::EIO)?;
                Ok(v)
            }
            Err(e) => {
                txn.abort();
                Err(e)
            }
        }
    }

    fn job(&self) -> JobHandle {
        self.job
    }

    fn sysval(&self, name: &str) -> Option<String> {
        self.sysvals
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(name)
            .cloned()
    }

    fn now(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    }
}

/// A store error as an `errno`.
#[must_use]
pub fn store_errno(e: &store::StoreError) -> Errno {
    use store::StoreError as E;
    match e {
        E::ObjectNotFound { .. } | E::LibraryNotFound { .. } | E::StaleReference { .. } => {
            Errno::ENOENT
        }
        E::AlreadyExists { .. } => Errno::EEXIST,
        E::NotAuthorized { .. } => Errno::EACCES,
        E::Locked { .. } | E::Busy { .. } => Errno::EBUSY,
        E::InvalidName { .. } => Errno::EBADNAME,
        E::NoSpace { .. } => Errno::ENOSPC,
        E::Damaged { .. } => Errno::EDAMAGE,
        _ => Errno::EIO,
    }
}

/// A database error as an `errno`.
#[must_use]
pub fn db_errno(e: &db::DbError) -> Errno {
    use db::DbError as E;
    match e {
        E::Store(s) => store_errno(s),
        E::MemberNotFound { .. } | E::NotAFile { .. } => Errno::ENOENT,
        E::MemberExists { .. } => Errno::EEXIST,
        E::TooManyMembers { .. } | E::MemberFull { .. } => Errno::ENOSPC,
        _ => Errno::EIO,
    }
}
