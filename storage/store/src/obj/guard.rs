// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The authority guard. The store does not know who is asking or what a
//! user may do; the security layer does, and installs a [`Guard`] on the
//! store. Every operation that touches an object on someone's behalf
//! calls [`check`] with an operation name from `sec/required.toml`; a
//! store without a guard (formatting, tests, the IPL before security is
//! up) allows everything.

use std::sync::Arc;

use crate::Result;
use crate::obj::header::ObjectHeader;
use crate::txn::Txn;

/// What the security layer does for the store.
pub trait Guard: Send + Sync {
    /// Authorize `op` on the object described by `h`, or fail with
    /// `StoreError::NotAuthorized`.
    fn check(&self, txn: &mut Txn<'_>, op: &'static str, h: &ObjectHeader) -> Result<()>;

    /// A new object's header before it is written: owner, primary
    /// group, public authority and auditing as the creator and the
    /// library say. `library` is the containing library's header.
    fn creating(
        &self,
        txn: &mut Txn<'_>,
        h: &mut ObjectHeader,
        library: &ObjectHeader,
    ) -> Result<()>;

    /// The object now exists: index it under its owner.
    fn created(&self, txn: &mut Txn<'_>, h: &ObjectHeader) -> Result<()>;

    /// The object is about to go: drop it from every profile's index.
    fn deleting(&self, txn: &mut Txn<'_>, h: &ObjectHeader) -> Result<()>;

    /// The object's storage changes by `pages` (negative: freed): the
    /// owner's tally, and its storage limit when it grows.
    fn sizing(&self, txn: &mut Txn<'_>, h: &ObjectHeader, pages: i64) -> Result<()>;
}

/// The guard installed on the transaction's store, if any.
fn guard(txn: &mut Txn<'_>) -> Option<Arc<dyn Guard>> {
    txn.store().guard()
}

/// Authorize `op` on `h`.
pub fn check(txn: &mut Txn<'_>, op: &'static str, h: &ObjectHeader) -> Result<()> {
    match guard(txn) {
        Some(g) => g.check(txn, op, h),
        None => Ok(()),
    }
}

/// Authorize `op` on the object `id`.
pub fn check_id(txn: &mut Txn<'_>, op: &'static str, id: crate::obj::ObjectId) -> Result<()> {
    match guard(txn) {
        Some(g) => {
            let (_, h) = crate::obj::ops::read_header(txn, id)?;
            g.check(txn, op, &h)
        }
        None => Ok(()),
    }
}

/// See [`Guard::creating`].
pub fn creating(txn: &mut Txn<'_>, h: &mut ObjectHeader, library: &ObjectHeader) -> Result<()> {
    match guard(txn) {
        Some(g) => g.creating(txn, h, library),
        None => Ok(()),
    }
}

/// See [`Guard::created`].
pub fn created(txn: &mut Txn<'_>, h: &ObjectHeader) -> Result<()> {
    match guard(txn) {
        Some(g) => g.created(txn, h),
        None => Ok(()),
    }
}

/// See [`Guard::deleting`].
pub fn deleting(txn: &mut Txn<'_>, h: &ObjectHeader) -> Result<()> {
    match guard(txn) {
        Some(g) => g.deleting(txn, h),
        None => Ok(()),
    }
}

/// See [`Guard::sizing`].
pub fn sizing(txn: &mut Txn<'_>, h: &ObjectHeader, pages: i64) -> Result<()> {
    match guard(txn) {
        Some(g) => g.sizing(txn, h, pages),
        None => Ok(()),
    }
}
