// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A library's SQL name index: SQL name to system object, in the
//! library header's tree root slot 0. Schemas are indexed in QSYS's.
//! See `docs/DB_FORMAT.md` §8.2.

use qsname::Name;
use store::btree::{BTree, DynKey};
use store::obj::{ObjectId, ops};
use store::txn::Txn;

use crate::error::{DbError, Result};
use crate::spec::{Layout, field};

/// Tree root slot of the index in a library's header.
pub const SLOT: usize = 0;
/// Bytes of an SQL name in a key: 128 characters of UTF-8 at most.
pub const NAME_BYTES: usize = 512;
/// Bytes of a key: the namespace and the name, zero padded.
pub const KEY_LEN: usize = 1 + NAME_BYTES;

/// The key layout.
pub static KEY_LAYOUT: Layout = Layout {
    name: "SqlNameKey",
    size: Some(KEY_LEN),
    fields: &[field("space", 0, 1), field("name", 1, NAME_BYTES)],
};

/// SQL namespaces: names in different ones never clash.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Space {
    /// Tables, views, aliases and materialized query tables.
    Table = 1,
    /// Indexes.
    Index = 2,
    /// Routines by specific name.
    Specific = 3,
    /// Sequences.
    Sequence = 4,
    /// Global variables.
    Variable = 5,
    /// User-defined types.
    Type = 6,
    /// Triggers.
    Trigger = 7,
    /// Constraints.
    Constraint = 8,
    /// Schemas (in QSYS's index only).
    Schema = 9,
}

/// What an SQL name stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    /// The system object (for a constraint, its table).
    pub id: ObjectId,
    /// Its system name.
    pub system: Name,
}

fn key(space: Space, name: &str) -> Result<DynKey> {
    if name.is_empty() || name.len() > NAME_BYTES || name.chars().count() > 128 {
        return Err(DbError::Corrupt { what: format!("SQL name {name:?} too long") });
    }
    let mut k = vec![0u8; KEY_LEN];
    k[0] = space as u8;
    k[1..=name.len()].copy_from_slice(name.as_bytes());
    Ok(DynKey(k))
}

fn value(t: Target) -> Vec<u8> {
    let mut v = t.id.to_bytes().to_vec();
    v.extend_from_slice(t.system.as_bytes());
    v
}

fn target(v: &[u8]) -> Result<Target> {
    if v.len() < 26 {
        return Err(DbError::Corrupt { what: "SQL name index value".into() });
    }
    Ok(Target {
        id: ObjectId::from_bytes(&v[..16]),
        system: Name::from_bytes(v[16..26].try_into().expect("10")),
    })
}

fn open(txn: &mut Txn<'_>, lib: ObjectId) -> Result<(u64, BTree<DynKey, Vec<u8>>)> {
    let (page, h) = ops::read_header(txn, lib)?;
    Ok((page, BTree::with_key_len(h.tree_roots[SLOT], lib, KEY_LEN)))
}

/// The object `name` stands for in `lib`'s `space`.
pub fn find(txn: &mut Txn<'_>, lib: ObjectId, space: Space, name: &str) -> Result<Option<Target>> {
    let (_, t) = open(txn, lib)?;
    if t.is_empty() {
        return Ok(None);
    }
    t.get(txn, &key(space, name)?)?.map(|v| target(&v)).transpose()
}

/// Record `name` in `lib`'s `space`; returns what it replaced.
pub fn put(
    txn: &mut Txn<'_>,
    lib: ObjectId,
    space: Space,
    name: &str,
    to: Target,
) -> Result<Option<Target>> {
    let (page, mut t) = open(txn, lib)?;
    let before = t.root();
    let old = t.insert(txn, key(space, name)?, &value(to))?;
    if t.root() != before {
        ops::set_tree_root(txn, page, SLOT, t.root())?;
    }
    old.map(|v| target(&v)).transpose()
}

/// Forget `name` in `lib`'s `space`.
pub fn remove(txn: &mut Txn<'_>, lib: ObjectId, space: Space, name: &str) -> Result<()> {
    let (page, mut t) = open(txn, lib)?;
    if t.is_empty() {
        return Ok(());
    }
    let before = t.root();
    t.delete(txn, &key(space, name)?)?;
    if t.root() != before {
        ops::set_tree_root(txn, page, SLOT, t.root())?;
    }
    Ok(())
}

/// Every name in `lib`'s `space`, in byte order.
pub fn list(txn: &mut Txn<'_>, lib: ObjectId, space: Space) -> Result<Vec<(String, Target)>> {
    let (_, t) = open(txn, lib)?;
    if t.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for item in t.entries(txn)? {
        let (k, v) = item?;
        if k.0[0] != space as u8 {
            continue;
        }
        let name = &k.0[1..];
        let end = name.iter().position(|&b| b == 0).unwrap_or(NAME_BYTES);
        out.push((String::from_utf8_lossy(&name[..end]).into_owned(), target(&v)?));
    }
    Ok(out)
}
