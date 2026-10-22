// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A profile's indexes of the objects it is tied to, so `DSPUSRPRF
//! *OBJOWN`/`*OBJAUT` and `DLTUSRPRF` need no scan of the system. Each
//! is a B+tree in one of the profile object's tree root slots, keyed by
//! object ID: owned objects, private authorities (value: the bits), and
//! objects whose primary group the profile is.

use store::btree::BTree;
use store::obj::ops::{self, read_header};
use store::obj::{ObjectId, auth, types};
use store::txn::Txn;

use crate::Result;
use crate::profile::objects;

/// Which index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Objects the profile owns.
    Owned = 0,
    /// Objects the profile has a private authority to.
    Private = 1,
    /// Objects the profile is the primary group of.
    Group = 2,
}

fn tree(root: Option<u64>, id: ObjectId) -> BTree<ObjectId, u64> {
    BTree::open(root, id)
}

/// Add or update `obj` in `profile`'s `kind` index; a profile that does
/// not exist (yet) has no index and is skipped.
pub fn put(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    profile: &str,
    kind: Kind,
    obj: ObjectId,
    value: u64,
) -> Result<()> {
    let Some(pid) = lookup(txn, qsys, profile)? else {
        return Ok(());
    };
    let (page, h) = read_header(txn, pid)?;
    let slot = kind as usize;
    let mut t = tree(h.tree_roots[slot], pid);
    let old = t.insert(txn, obj, &value)?;
    if t.root() != h.tree_roots[slot] {
        ops::set_tree_root(txn, page, slot, t.root())?;
    }
    if kind == Kind::Owned && old.is_none() {
        bump_owned(txn, pid, 1)?;
    }
    Ok(())
}

/// Remove `obj` from `profile`'s `kind` index.
pub fn remove(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    profile: &str,
    kind: Kind,
    obj: ObjectId,
) -> Result<()> {
    let Some(pid) = lookup(txn, qsys, profile)? else {
        return Ok(());
    };
    let (page, h) = read_header(txn, pid)?;
    let slot = kind as usize;
    if h.tree_roots[slot].is_none() {
        return Ok(());
    }
    let mut t = tree(h.tree_roots[slot], pid);
    let old = t.delete(txn, &obj)?;
    if t.root() != h.tree_roots[slot] {
        ops::set_tree_root(txn, page, slot, t.root())?;
    }
    if kind == Kind::Owned && old.is_some() {
        bump_owned(txn, pid, -1)?;
    }
    Ok(())
}

/// Every entry of `profile`'s `kind` index.
pub fn list(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    profile: &str,
    kind: Kind,
) -> Result<Vec<(ObjectId, u64)>> {
    let Some(pid) = lookup(txn, qsys, profile)? else {
        return Ok(Vec::new());
    };
    let (_, h) = read_header(txn, pid)?;
    let t = tree(h.tree_roots[kind as usize], pid);
    if t.is_empty() {
        return Ok(Vec::new());
    }
    Ok(t.entries(txn)?.collect::<store::Result<_>>()?)
}

/// Empty every index of profile object `pid` (its object is going).
pub fn clear(txn: &mut Txn<'_>, pid: ObjectId) -> Result<()> {
    let (page, h) = read_header(txn, pid)?;
    for slot in 0..3 {
        if let Some(root) = h.tree_roots[slot] {
            store::btree::free_tree(txn, root)?;
            ops::set_tree_root(txn, page, slot, None)?;
        }
    }
    Ok(())
}

fn lookup(txn: &mut Txn<'_>, qsys: ObjectId, profile: &str) -> Result<Option<ObjectId>> {
    if profile.is_empty() || profile.starts_with('*') {
        return Ok(None);
    }
    objects::find(txn, qsys, profile)
}

fn bump_owned(txn: &mut Txn<'_>, pid: ObjectId, by: i64) -> Result<()> {
    let mut p = objects::read_id(txn, pid)?;
    p.owned_count = p.owned_count.saturating_add_signed(by);
    objects::write(txn, pid, &p)
}

/// Rebuild every profile's indexes from the objects themselves: what
/// `RCLSTG` and the first IPL with security do. Returns the entries made.
pub fn rebuild(txn: &mut Txn<'_>, qsys: ObjectId) -> Result<usize> {
    let profiles = objects::list(txn, qsys)?;
    for name in &profiles {
        if let Some(pid) = objects::find(txn, qsys, name)? {
            clear(txn, pid)?;
            let mut p = objects::read_id(txn, pid)?;
            p.owned_count = 0;
            objects::write(txn, pid, &p)?;
        }
    }
    let mut made = 0;
    for id in all_objects(txn, qsys)? {
        let (_, h) = read_header(txn, id)?;
        let owner = crate::rec::text_of(&h.owner);
        put(txn, qsys, &owner, Kind::Owned, id, 0)?;
        made += 1;
        let pgp = crate::rec::text_of(&h.primary_group);
        if !pgp.is_empty() {
            put(
                txn,
                qsys,
                &pgp,
                Kind::Group,
                id,
                u64::from(h.group_authority.0),
            )?;
            made += 1;
        }
        for pa in auth::private_list(txn, &h)? {
            let who = crate::rec::text_of(&pa.profile);
            put(txn, qsys, &who, Kind::Private, id, u64::from(pa.bits.0))?;
            made += 1;
        }
    }
    Ok(made)
}

/// Every object in a library, every library, and QSYS itself.
pub fn all_objects(txn: &mut Txn<'_>, qsys: ObjectId) -> Result<Vec<ObjectId>> {
    let mut out = vec![qsys];
    let lib = store::obj::library::Library::load(txn, qsys)?;
    let top = lib.list(txn, None)?;
    for (_, id) in top {
        if id == qsys {
            continue;
        }
        out.push(id);
        if id.type_code == types::LIB {
            let l = store::obj::library::Library::load(txn, id)?;
            out.extend(l.list(txn, None)?.into_iter().map(|(_, o)| o));
        }
    }
    Ok(out)
}

/// Where the indexes and the objects disagree, as text; empty when they
/// agree. `CHKOBJITG`-style use and the deletion tests.
pub fn verify(txn: &mut Txn<'_>, qsys: ObjectId) -> Result<Vec<String>> {
    use std::collections::BTreeSet;
    let mut want: BTreeSet<(String, u8, ObjectId, u64)> = BTreeSet::new();
    for id in all_objects(txn, qsys)? {
        let (_, h) = read_header(txn, id)?;
        let owner = crate::rec::text_of(&h.owner);
        if objects::exists(txn, qsys, &owner) {
            want.insert((owner, Kind::Owned as u8, id, 0));
        }
        let pgp = crate::rec::text_of(&h.primary_group);
        if !pgp.is_empty() {
            want.insert((pgp, Kind::Group as u8, id, u64::from(h.group_authority.0)));
        }
        for pa in auth::private_list(txn, &h)? {
            let who = crate::rec::text_of(&pa.profile);
            want.insert((who, Kind::Private as u8, id, u64::from(pa.bits.0)));
        }
    }
    let mut have = BTreeSet::new();
    for name in objects::list(txn, qsys)? {
        for kind in [Kind::Owned, Kind::Private, Kind::Group] {
            for (id, v) in list(txn, qsys, &name, kind)? {
                have.insert((name.clone(), kind as u8, id, v));
            }
        }
    }
    let mut out: Vec<String> = want
        .difference(&have)
        .map(|e| format!("missing from the index: {e:?}"))
        .collect();
    out.extend(have.difference(&want).map(|e| format!("indexed but not on the object: {e:?}")));
    Ok(out)
}
