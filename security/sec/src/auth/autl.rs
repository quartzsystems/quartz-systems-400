// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Authorization lists. A `*AUTL` object in `QSYS` is the list: its
//! private authorities are the list's entries and its `*PUBLIC` is the
//! list's public authority, so the entries live in the header and its
//! overflow tree like any object's. Tree root slot 0 indexes the
//! objects the list secures.

use store::btree::BTree;
use store::obj::header::Authority as A;
use store::obj::ops::{self, CreateParams, read_header};
use store::obj::{ObjectId, auth, types};
use store::txn::Txn;

use crate::{Result, SecError};

/// `CPF2283`-style: no such list.
pub fn not_found(name: &str) -> SecError {
    SecError::msg(
        "CPF2283",
        format!("Authorization list {name} does not exist."),
    )
}

/// The list object `name`.
pub fn find(txn: &mut Txn<'_>, qsys: ObjectId, name: &str) -> Result<ObjectId> {
    let n = qsname::Name::parse(name).map_err(|_| not_found(name))?;
    let lib = store::obj::library::Library::load(txn, qsys)?;
    lib.lookup(txn, types::AUTL, n)?
        .ok_or_else(|| not_found(name))
}

/// Create list `name` owned by `owner` with `*PUBLIC` as `public`.
pub fn create(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    name: &str,
    text: &str,
    owner: &str,
    public: A,
    now: u64,
) -> Result<ObjectId> {
    let n = qsname::Name::parse(name)
        .map_err(|_| SecError::msg("CPF2203", format!("{name} is not a valid name.")))?;
    let lib = store::obj::library::Library::load(txn, qsys)?;
    if lib.lookup(txn, types::AUTL, n)?.is_some() {
        return Err(SecError::msg(
            "CPF2282",
            format!("Authorization list {name} already exists."),
        ));
    }
    let id = ops::create(
        txn,
        &CreateParams {
            library: qsys,
            name: n,
            type_code: types::AUTL,
            attribute: String::new(),
            text: text.chars().take(50).collect(),
            owner: owner.into(),
            system: "QS400".into(),
            now,
            by_system: true,
        },
    )?;
    let (page, mut h) = auth::load(txn, id)?;
    h.public_authority = public;
    auth::save_authority(txn, page, &mut h)?;
    Ok(id)
}

fn secured_tree(txn: &mut Txn<'_>, list: ObjectId) -> Result<(u64, BTree<ObjectId, u64>)> {
    let (page, h) = read_header(txn, list)?;
    Ok((page, BTree::open(h.tree_roots[0], list)))
}

/// Record that the list secures `obj`.
pub fn secure(txn: &mut Txn<'_>, qsys: ObjectId, name: &str, obj: ObjectId) -> Result<()> {
    let list = find(txn, qsys, name)?;
    let (page, mut t) = secured_tree(txn, list)?;
    let before = t.root();
    t.insert(txn, obj, &0)?;
    if t.root() != before {
        ops::set_tree_root(txn, page, 0, t.root())?;
    }
    Ok(())
}

/// Record that the list no longer secures `obj`.
pub fn unsecure(txn: &mut Txn<'_>, qsys: ObjectId, name: &str, obj: ObjectId) -> Result<()> {
    let Ok(list) = find(txn, qsys, name) else {
        return Ok(());
    };
    let (page, mut t) = secured_tree(txn, list)?;
    if t.is_empty() {
        return Ok(());
    }
    let before = t.root();
    t.delete(txn, &obj)?;
    if t.root() != before {
        ops::set_tree_root(txn, page, 0, t.root())?;
    }
    Ok(())
}

/// The objects the list secures.
pub fn secured(txn: &mut Txn<'_>, qsys: ObjectId, name: &str) -> Result<Vec<ObjectId>> {
    let list = find(txn, qsys, name)?;
    let (_, t) = secured_tree(txn, list)?;
    if t.is_empty() {
        return Ok(Vec::new());
    }
    Ok(t.entries(txn)?
        .map(|e| e.map(|(k, _)| k))
        .collect::<store::Result<_>>()?)
}

/// Every list name.
pub fn list(txn: &mut Txn<'_>, qsys: ObjectId) -> Result<Vec<String>> {
    let lib = store::obj::library::Library::load(txn, qsys)?;
    Ok(lib
        .list(txn, Some(types::AUTL))?
        .into_iter()
        .map(|(k, _)| k.name.to_string())
        .collect())
}
