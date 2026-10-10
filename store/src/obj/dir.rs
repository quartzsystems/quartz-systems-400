// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The object directory (ID to header page) and the per-library
//! directories (type and name to ID).

use qsname::Name;

use crate::btree::BTree;
use crate::btree::codec::FixedKey;
use crate::obj::ObjectId;
use crate::page::pager::PageNo;
use crate::spec::{Layout, field};
use crate::txn::Txn;
use crate::{Result, StoreError};

/// The object directory tree.
pub type ObjDir = BTree<ObjectId, u64>;
/// A library directory tree.
pub type LibDir = BTree<DirKey, ObjectId>;

/// The spec layout of a directory key.
pub static DIR_KEY_LAYOUT: Layout = Layout {
    name: "DirKey",
    size: Some(12),
    fields: &[field("type_code", 0, 2), field("name", 2, 10)],
};

/// Type and name, the key of a library directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DirKey {
    /// Type code.
    pub type_code: u16,
    /// Object name.
    pub name: Name,
}

impl DirKey {
    /// Build a key.
    #[must_use]
    pub fn new(type_code: u16, name: Name) -> Self {
        Self { type_code, name }
    }
}

impl FixedKey for DirKey {
    const LEN: usize = 12;
    fn write(&self, out: &mut [u8]) {
        out[..2].copy_from_slice(&self.type_code.to_be_bytes());
        out[2..12].copy_from_slice(self.name.as_bytes());
    }
    fn read(bytes: &[u8]) -> Self {
        Self {
            type_code: u16::from_be_bytes([bytes[0], bytes[1]]),
            name: Name::from_bytes(bytes[2..12].try_into().expect("10")),
        }
    }
}

/// The object directory as this transaction sees it.
#[must_use]
pub fn objdir(txn: &Txn<'_>) -> ObjDir {
    ObjDir::open(txn.objdir_root(), ObjectId::NONE)
}

/// Header page of `id`. A serial below the ASP's counter that is gone
/// is a stale reference; one at or above it never existed.
pub fn header_page(txn: &mut Txn<'_>, id: ObjectId) -> Result<PageNo> {
    let dir = objdir(txn);
    if let Some(p) = dir.get(txn, &id)? {
        return Ok(p);
    }
    // Same serial, any generation?
    let probe = ObjectId {
        generation: u32::MAX,
        ..id
    };
    if let Some((found, _)) = dir.floor(txn, &probe)?
        && found.asp == id.asp
        && found.type_code == id.type_code
        && found.serial == id.serial
    {
        return Err(StoreError::StaleReference { id });
    }
    if id.serial < txn.peek_serial() {
        return Err(StoreError::StaleReference { id });
    }
    Err(StoreError::ObjectNotFound {
        name: id.to_string(),
        library: String::new(),
    })
}

/// Record `id` at `page` in the object directory.
pub fn objdir_insert(txn: &mut Txn<'_>, id: ObjectId, page: PageNo) -> Result<()> {
    let mut dir = objdir(txn);
    if dir.insert(txn, id, &page)?.is_some() {
        return Err(StoreError::Corrupt {
            page,
            what: format!("object {id} already in the directory"),
        });
    }
    txn.set_root(ObjectId::NONE, dir.root());
    Ok(())
}

/// Remove `id` from the object directory.
pub fn objdir_remove(txn: &mut Txn<'_>, id: ObjectId) -> Result<()> {
    let mut dir = objdir(txn);
    if dir.delete(txn, &id)?.is_none() {
        return Err(StoreError::StaleReference { id });
    }
    txn.set_root(ObjectId::NONE, dir.root());
    Ok(())
}

/// Every (ID, header page) in the directory.
pub fn objdir_entries(txn: &mut Txn<'_>) -> Result<Vec<(ObjectId, PageNo)>> {
    let dir = objdir(txn);
    dir.entries(txn)?.collect()
}
