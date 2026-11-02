// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! LOB values: the `LobRef` a LOB field holds in the record and the
//! member's LOB tree that keeps values longer than the reference. See
//! `docs/DB_FORMAT.md` §4.

use store::btree::BTree;
use store::obj::ObjectId;
use store::txn::Txn;

use super::Core;
use crate::error::{DbError, Result};
use crate::spec::{Layout, field};

/// Header tree root slot of the LOB tree.
pub const LOB_ROOT: usize = 2;
/// Bytes of a `LobRef`.
pub const REF_LEN: usize = 32;
/// The longest value kept in the reference itself.
pub const INLINE_MAX: usize = 24;
/// Bytes per tree value.
pub const CHUNK: usize = 1024;

/// The spec layout.
pub static LAYOUT: Layout = Layout {
    name: "LobRef",
    size: Some(REF_LEN),
    fields: &[
        field("length", 0, 4),
        field("kind", 4, 1),
        field("reserved", 5, 3),
        field("data", 8, 24),
    ],
};

/// Keys `(lob_id << 24) | chunk`, values the chunks.
type LobTree = BTree<u64, Vec<u8>>;

fn corrupt(what: impl Into<String>) -> DbError {
    DbError::Corrupt { what: what.into() }
}

/// A decoded reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LobRef {
    /// The value itself.
    Inline(Vec<u8>),
    /// A value in the tree: its ID and length.
    Tree {
        /// LOB ID.
        id: u64,
        /// Bytes.
        length: u32,
    },
}

impl LobRef {
    /// From the field's bytes.
    pub fn decode(b: &[u8]) -> Result<Self> {
        if b.len() != REF_LEN {
            return Err(corrupt(format!("a LOB reference of {} bytes", b.len())));
        }
        let length = u32::from_be_bytes(b[..4].try_into().expect("4 bytes"));
        match b[4] {
            0 => {
                let n = usize::try_from(length).expect("fits");
                if n > INLINE_MAX {
                    return Err(corrupt("an inline LOB longer than its reference"));
                }
                Ok(Self::Inline(b[8..8 + n].to_vec()))
            }
            1 => Ok(Self::Tree { id: u64::from_be_bytes(b[8..16].try_into().expect("8 bytes")), length }),
            k => Err(corrupt(format!("LOB reference kind {k}"))),
        }
    }

    /// The field's bytes.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut b = vec![0u8; REF_LEN];
        match self {
            Self::Inline(data) => {
                b[..4].copy_from_slice(&u32::try_from(data.len()).expect("short").to_be_bytes());
                b[8..8 + data.len()].copy_from_slice(data);
            }
            Self::Tree { id, length } => {
                b[..4].copy_from_slice(&length.to_be_bytes());
                b[4] = 1;
                b[8..16].copy_from_slice(&id.to_be_bytes());
            }
        }
        b
    }

    /// The value's length in bytes.
    #[must_use]
    pub fn len(&self) -> u64 {
        match self {
            Self::Inline(d) => d.len() as u64,
            Self::Tree { length, .. } => u64::from(*length),
        }
    }

    /// Whether the value is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The member whose tree holds a record's LOBs: a physical member
/// itself, a logical member's first based-on member.
pub fn owner(txn: &mut Txn<'_>, member: ObjectId) -> Result<ObjectId> {
    let core = Core::load(txn, member)?;
    if core.attr.base_count == 0 {
        return Ok(member);
    }
    core.base_members(txn)?.first().copied().ok_or_else(|| corrupt("a logical member with no based-on member"))
}

/// Store a value in `member`, giving the reference the field holds.
pub fn put(txn: &mut Txn<'_>, member: ObjectId, data: &[u8]) -> Result<Vec<u8>> {
    if data.len() <= INLINE_MAX {
        return Ok(LobRef::Inline(data.to_vec()).encode());
    }
    let length = u32::try_from(data.len()).map_err(|_| DbError::DataMapping {
        field: "LOB".into(),
        what: format!("{} bytes is longer than a LOB can be", data.len()),
    })?;
    let mut core = Core::load(txn, member)?;
    let mut tree: LobTree = BTree::open(core.roots[LOB_ROOT], member);
    let id = tree.floor(txn, &u64::MAX)?.map_or(1, |(k, _)| (k >> 24) + 1);
    if id >= 1 << 40 {
        return Err(corrupt("the member has used every LOB ID"));
    }
    for (i, chunk) in data.chunks(CHUNK).enumerate() {
        tree.insert(txn, (id << 24) | i as u64, &chunk.to_vec())?;
    }
    core.set_root(txn, LOB_ROOT, tree.root())?;
    Ok(LobRef::Tree { id, length }.encode())
}

/// The bytes `start..start + len` of a value (cut at its end), read
/// from `member`'s tree.
pub fn read_part(txn: &mut Txn<'_>, member: ObjectId, r: &LobRef, start: u64, len: u64) -> Result<Vec<u8>> {
    let end = r.len().min(start.saturating_add(len));
    if start >= end {
        return Ok(Vec::new());
    }
    let (s, e) = (usize::try_from(start).expect("fits"), usize::try_from(end).expect("fits"));
    let (id, length) = match r {
        LobRef::Inline(d) => return Ok(d[s..e].to_vec()),
        LobRef::Tree { id, length } => (*id, *length),
    };
    let core = Core::load(txn, member)?;
    let tree: LobTree = BTree::open(core.roots[LOB_ROOT], member);
    let mut out = Vec::with_capacity(e - s);
    for i in s / CHUNK..e.div_ceil(CHUNK) {
        let chunk = tree
            .get(txn, &((id << 24) | i as u64))?
            .ok_or_else(|| corrupt(format!("LOB {id} of {length} bytes has no chunk {i}")))?;
        let base = i * CHUNK;
        let lo = s.saturating_sub(base);
        let hi = (e - base).min(chunk.len());
        if lo < hi {
            out.extend_from_slice(&chunk[lo..hi]);
        }
    }
    if out.len() != e - s {
        return Err(corrupt(format!("LOB {id} is shorter than its {length} bytes")));
    }
    Ok(out)
}

/// The whole value.
pub fn read(txn: &mut Txn<'_>, member: ObjectId, r: &LobRef) -> Result<Vec<u8>> {
    read_part(txn, member, r, 0, r.len())
}

/// Free a value's chunks: an update or delete that no rollback or
/// journal entry can put back. An inline value has none.
pub fn free(txn: &mut Txn<'_>, member: ObjectId, r: &LobRef) -> Result<()> {
    let LobRef::Tree { id, length } = r else {
        return Ok(());
    };
    let mut core = Core::load(txn, member)?;
    let mut tree: LobTree = BTree::open(core.roots[LOB_ROOT], member);
    for i in 0..usize::try_from(*length).expect("fits").div_ceil(CHUNK) {
        tree.delete(txn, &((id << 24) | i as u64))?;
    }
    core.set_root(txn, LOB_ROOT, tree.root())?;
    Ok(())
}

/// Free the chunks of every value not in `keep` (`RGZPFM`); the number
/// of values freed.
pub fn sweep<S: std::hash::BuildHasher>(txn: &mut Txn<'_>, member: ObjectId, keep: &std::collections::HashSet<u64, S>) -> Result<u64> {
    let mut core = Core::load(txn, member)?;
    let mut tree: LobTree = BTree::open(core.roots[LOB_ROOT], member);
    let gone: Vec<u64> = tree.entries(txn)?.map(|e| e.map(|(k, _)| k)).collect::<store::Result<Vec<_>>>()?;
    let gone: Vec<u64> = gone.into_iter().filter(|k| !keep.contains(&(k >> 24))).collect();
    let mut freed = std::collections::HashSet::new();
    for k in gone {
        tree.delete(txn, &k)?;
        freed.insert(k >> 24);
    }
    core.set_root(txn, LOB_ROOT, tree.root())?;
    Ok(freed.len() as u64)
}

/// The IDs of the values in the member's tree, ascending.
pub fn ids(txn: &mut Txn<'_>, member: ObjectId) -> Result<Vec<u64>> {
    let core = Core::load(txn, member)?;
    let tree: LobTree = BTree::open(core.roots[LOB_ROOT], member);
    let mut out: Vec<u64> = Vec::new();
    for e in tree.entries(txn)? {
        let id = e?.0 >> 24;
        if out.last() != Some(&id) {
            out.push(id);
        }
    }
    Ok(out)
}

/// Free the member's tree (`CLRPFM`).
pub fn clear(txn: &mut Txn<'_>, member: ObjectId) -> Result<()> {
    let mut core = Core::load(txn, member)?;
    if let Some(root) = core.roots[LOB_ROOT] {
        store::btree::free_tree(txn, root)?;
        core.set_root(txn, LOB_ROOT, None)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references() {
        let r = LobRef::Inline(b"abc".to_vec());
        assert_eq!(LobRef::decode(&r.encode()).unwrap(), r);
        let t = LobRef::Tree { id: 7, length: 5000 };
        let b = t.encode();
        assert_eq!(&b[..8], &[0, 0, 0x13, 0x88, 1, 0, 0, 0]);
        assert_eq!(LobRef::decode(&b).unwrap(), t);
        assert_eq!(t.len(), 5000);
        assert!(LobRef::decode(&[0; 31]).is_err());
        let mut bad = r.encode();
        bad[4] = 9;
        assert!(LobRef::decode(&bad).is_err());
        assert!(LobRef::decode(&[0; 32]).unwrap().is_empty());
    }
}
