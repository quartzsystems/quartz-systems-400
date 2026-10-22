// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*USRIDX`: keyed entries in a B+tree owned by the index. See the
//! format spec §10.

use std::ops::Bound;

use crate::btree::BTree;
use crate::btree::codec::FixedKey;
use crate::bytes::{get_u16, get_u64, put_u16, put_u64};
use crate::obj::ObjectId;
use crate::obj::ops::{self, CreateParams};
use crate::obj::types;
use crate::spec::{Layout, field};
use crate::txn::Txn;
use crate::{Result, StoreError};

pub use crate::obj::dtaq::KeyOp;

/// Bytes of the attribute area used.
pub const ATTR_LEN: usize = 32;
/// Longest key.
pub const MAX_KEY: u16 = 120;
/// Longest entry beyond the key.
pub const MAX_DATA: usize = 1024;

const OFF_KIND: usize = 0;
const OFF_KEY_LEN: usize = 2;
const OFF_ENTRY_LEN: usize = 4;
const OFF_ROOT: usize = 8;
const OFF_COUNT: usize = 16;
const OFF_NEXT_SEQ: usize = 24;

/// The spec layout of the attribute area.
pub static LAYOUT: Layout = Layout {
    name: "UsridxAttr",
    size: Some(ATTR_LEN),
    fields: &[
        field("entry_kind", OFF_KIND, 1),
        field("reserved", 1, 1),
        field("key_len", OFF_KEY_LEN, 2),
        field("entry_len", OFF_ENTRY_LEN, 2),
        field("reserved2", 6, 2),
        field("root", OFF_ROOT, 8),
        field("entry_count", OFF_COUNT, 8),
        field("next_seq", OFF_NEXT_SEQ, 8),
    ],
};

/// The spec layout of a tree key.
pub static KEY_LAYOUT: Layout = Layout {
    name: "IdxKey",
    size: Some(128),
    fields: &[field("key", 0, 120), field("seq", 120, 8)],
};

/// Tree key: the entry key zero padded, then the insertion sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct IdxKey {
    /// Zero-padded key.
    pub key: [u8; 120],
    /// Insertion sequence.
    pub seq: u64,
}

impl IdxKey {
    fn new(key: &[u8], seq: u64) -> Self {
        let mut k = [0u8; 120];
        k[..key.len()].copy_from_slice(key);
        Self { key: k, seq }
    }
}

impl FixedKey for IdxKey {
    const LEN: usize = 128;
    fn write(&self, out: &mut [u8]) {
        out[..120].copy_from_slice(&self.key);
        out[120..128].copy_from_slice(&self.seq.to_be_bytes());
    }
    fn read(bytes: &[u8]) -> Self {
        Self {
            key: bytes[..120].try_into().expect("120"),
            seq: u64::from_be_bytes(bytes[120..128].try_into().expect("8")),
        }
    }
}

type Tree = BTree<IdxKey, Vec<u8>>;

/// The attribute area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexAttr {
    /// Whether entries are all `entry_len` bytes.
    pub fixed: bool,
    /// Key length.
    pub key_len: u16,
    /// Entry length, fixed or maximum.
    pub entry_len: u16,
    /// Tree root.
    pub root: Option<u64>,
    /// Entries.
    pub entry_count: u64,
    /// Next sequence.
    pub next_seq: u64,
}

impl IndexAttr {
    fn encode(&self) -> [u8; ATTR_LEN] {
        let mut a = [0u8; ATTR_LEN];
        a[OFF_KIND] = u8::from(!self.fixed);
        put_u16(&mut a, OFF_KEY_LEN, self.key_len);
        put_u16(&mut a, OFF_ENTRY_LEN, self.entry_len);
        put_u64(&mut a, OFF_ROOT, self.root.unwrap_or(0));
        put_u64(&mut a, OFF_COUNT, self.entry_count);
        put_u64(&mut a, OFF_NEXT_SEQ, self.next_seq);
        a
    }

    fn decode(a: &[u8]) -> Self {
        let root = get_u64(a, OFF_ROOT);
        Self {
            fixed: a[OFF_KIND] == 0,
            key_len: get_u16(a, OFF_KEY_LEN),
            entry_len: get_u16(a, OFF_ENTRY_LEN),
            root: (root != 0).then_some(root),
            entry_count: get_u64(a, OFF_COUNT),
            next_seq: get_u64(a, OFF_NEXT_SEQ),
        }
    }
}

/// Write the attribute area back.
pub(crate) fn write_attr(
    txn: &mut Txn<'_>,
    page: crate::page::pager::PageNo,
    attr: &IndexAttr,
) -> Result<()> {
    ops::write_attr(txn, page, 0, &attr.encode())
}

/// Create an index.
pub fn create(
    txn: &mut Txn<'_>,
    params: &CreateParams,
    fixed: bool,
    key_len: u16,
    entry_len: u16,
) -> Result<ObjectId> {
    if !(1..=MAX_KEY).contains(&key_len)
        || entry_len < key_len
        || usize::from(entry_len - key_len) > MAX_DATA
    {
        return Err(StoreError::BadValue {
            what: format!("index with key {key_len} and entry {entry_len}"),
        });
    }
    let params = CreateParams {
        type_code: types::USRIDX,
        ..params.clone()
    };
    let id = ops::create(txn, &params)?;
    let attr = IndexAttr {
        fixed,
        key_len,
        entry_len,
        root: None,
        entry_count: 0,
        next_seq: 1,
    };
    save(txn, id, &attr)?;
    Ok(id)
}

/// The attribute area.
pub fn describe(txn: &mut Txn<'_>, id: ObjectId) -> Result<IndexAttr> {
    let (page, h) = ops::read_header(txn, id)?;
    if h.id.type_code != types::USRIDX {
        return Err(StoreError::ObjectNotFound {
            name: h.name.to_string(),
            library: String::new(),
        });
    }
    let a = ops::read_attr(txn, page)?;
    Ok(IndexAttr::decode(&a[..ATTR_LEN]))
}

fn save(txn: &mut Txn<'_>, id: ObjectId, attr: &IndexAttr) -> Result<()> {
    let (page, _) = ops::read_header(txn, id)?;
    ops::write_attr(txn, page, 0, &attr.encode())
}

fn check_entry(attr: &IndexAttr, entry: &[u8]) -> Result<()> {
    let ok = if attr.fixed {
        entry.len() == usize::from(attr.entry_len)
    } else {
        entry.len() >= usize::from(attr.key_len) && entry.len() <= usize::from(attr.entry_len)
    };
    if ok {
        Ok(())
    } else {
        Err(StoreError::BadValue {
            what: format!(
                "entry of {} bytes for an index of {} (key {})",
                entry.len(),
                attr.entry_len,
                attr.key_len
            ),
        })
    }
}

fn bad_key(attr: &IndexAttr, key: &[u8]) -> Result<()> {
    if key.len() == usize::from(attr.key_len) {
        Ok(())
    } else {
        Err(StoreError::BadValue {
            what: format!(
                "key of {} bytes for an index with {}-byte keys",
                key.len(),
                attr.key_len
            ),
        })
    }
}

/// Insert an entry (its first `key_len` bytes are the key). Equal
/// keys may repeat; an identical whole entry is refused.
pub fn insert(txn: &mut Txn<'_>, id: ObjectId, entry: &[u8]) -> Result<()> {
    crate::obj::guard::check_id(txn, "usridx.write", id)?;
    let mut attr = describe(txn, id)?;
    check_entry(&attr, entry)?;
    let (key, data) = entry.split_at(usize::from(attr.key_len));
    let mut tree = Tree::open(attr.root, id);
    for found in tree.range(txn, IdxKey::new(key, 0)..=IdxKey::new(key, u64::MAX))? {
        if found?.1 == data {
            return Err(StoreError::AlreadyExists {
                name: format!("index entry {}", hex(key)),
                library: String::new(),
            });
        }
    }
    tree.insert(txn, IdxKey::new(key, attr.next_seq), &data.to_vec())?;
    attr.root = tree.root();
    attr.next_seq += 1;
    attr.entry_count += 1;
    save(txn, id, &attr)
}

fn hex(b: &[u8]) -> String {
    use std::fmt::Write as _;
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}

fn entry_of(key_len: usize, k: &IdxKey, data: &[u8]) -> Vec<u8> {
    let mut e = k.key[..key_len].to_vec();
    e.extend_from_slice(data);
    e
}

/// Up to `max` entries whose key compares to `key` as `op` says, in
/// key order (`Lt` and `Le` are nearest first).
pub fn find(
    txn: &mut Txn<'_>,
    id: ObjectId,
    op: KeyOp,
    key: &[u8],
    max: usize,
) -> Result<Vec<Vec<u8>>> {
    crate::obj::guard::check_id(txn, "usridx.read", id)?;
    let attr = describe(txn, id)?;
    bad_key(&attr, key)?;
    let found = search(txn, id, &attr, op, key, max)?;
    Ok(found
        .into_iter()
        .map(|(k, d)| entry_of(usize::from(attr.key_len), &k, &d))
        .collect())
}

fn search(
    txn: &mut Txn<'_>,
    id: ObjectId,
    attr: &IndexAttr,
    op: KeyOp,
    key: &[u8],
    max: usize,
) -> Result<Vec<(IdxKey, Vec<u8>)>> {
    let tree = Tree::open(attr.root, id);
    let lo = IdxKey::new(key, 0);
    let hi = IdxKey::new(key, u64::MAX);
    let mut out = Vec::new();
    match op {
        KeyOp::Eq => {
            for r in tree.range(txn, lo..=hi)?.take(max) {
                out.push(r?);
            }
        }
        KeyOp::Ge => {
            for r in tree.range(txn, lo..)?.take(max) {
                out.push(r?);
            }
        }
        KeyOp::Gt => {
            for r in tree
                .range(txn, (Bound::Excluded(hi), Bound::Unbounded))?
                .take(max)
            {
                out.push(r?);
            }
        }
        KeyOp::Le | KeyOp::Lt => {
            let mut probe = if op == KeyOp::Le { Some(hi) } else { pred(&lo) };
            while out.len() < max {
                let Some(p) = probe else { break };
                let Some(found) = tree.floor(txn, &p)? else {
                    break;
                };
                probe = pred(&found.0);
                out.push(found);
            }
        }
        KeyOp::Ne => {
            for r in tree.entries(txn)? {
                let r = r?;
                if r.0.key[..key.len()] != *key {
                    out.push(r);
                    if out.len() >= max {
                        break;
                    }
                }
            }
        }
    }
    Ok(out)
}

/// The greatest key below `k`, as a 1024-bit integer minus one.
fn pred(k: &IdxKey) -> Option<IdxKey> {
    let mut out = *k;
    if out.seq > 0 {
        out.seq -= 1;
        return Some(out);
    }
    out.seq = u64::MAX;
    for b in out.key.iter_mut().rev() {
        if *b > 0 {
            *b -= 1;
            return Some(out);
        }
        *b = 0xFF;
    }
    None
}

/// The first `n` entries in key order.
pub fn first(txn: &mut Txn<'_>, id: ObjectId, n: usize) -> Result<Vec<Vec<u8>>> {
    crate::obj::guard::check_id(txn, "usridx.read", id)?;
    let attr = describe(txn, id)?;
    let tree = Tree::open(attr.root, id);
    let mut out = Vec::new();
    for r in tree.entries(txn)?.take(n) {
        let (k, d) = r?;
        out.push(entry_of(usize::from(attr.key_len), &k, &d));
    }
    Ok(out)
}

/// The last `n` entries, last first.
pub fn last(txn: &mut Txn<'_>, id: ObjectId, n: usize) -> Result<Vec<Vec<u8>>> {
    crate::obj::guard::check_id(txn, "usridx.read", id)?;
    let attr = describe(txn, id)?;
    let tree = Tree::open(attr.root, id);
    let mut out = Vec::new();
    let mut probe = Some(IdxKey {
        key: [0xFF; 120],
        seq: u64::MAX,
    });
    while out.len() < n {
        let Some(p) = probe else { break };
        let Some((k, d)) = tree.floor(txn, &p)? else {
            break;
        };
        probe = pred(&k);
        out.push(entry_of(usize::from(attr.key_len), &k, &d));
    }
    Ok(out)
}

/// Remove up to `max` entries matching `op key`. Returns them.
pub fn remove(
    txn: &mut Txn<'_>,
    id: ObjectId,
    op: KeyOp,
    key: &[u8],
    max: usize,
) -> Result<Vec<Vec<u8>>> {
    crate::obj::guard::check_id(txn, "usridx.write", id)?;
    let mut attr = describe(txn, id)?;
    bad_key(&attr, key)?;
    let found = search(txn, id, &attr, op, key, max)?;
    let mut tree = Tree::open(attr.root, id);
    let mut out = Vec::new();
    for (k, d) in found {
        tree.delete(txn, &k)?;
        attr.entry_count -= 1;
        out.push(entry_of(usize::from(attr.key_len), &k, &d));
    }
    attr.root = tree.root();
    save(txn, id, &attr)?;
    Ok(out)
}

/// Free the tree before the object is deleted.
pub fn drop_entries(txn: &mut Txn<'_>, id: ObjectId) -> Result<()> {
    crate::obj::guard::check_id(txn, "usridx.write", id)?;
    let mut attr = describe(txn, id)?;
    let mut tree = Tree::open(attr.root, id);
    tree.free_all(txn)?;
    attr.root = None;
    attr.entry_count = 0;
    save(txn, id, &attr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_predecessors() {
        let a = IdxKey::new(b"AB", 5);
        let b = IdxKey::new(b"AC", 0);
        assert!(a < b);
        let mut buf = [0u8; 128];
        a.write(&mut buf);
        assert_eq!(IdxKey::read(&buf), a);
        assert_eq!(pred(&a).unwrap().seq, 4);
        let p = pred(&b).unwrap();
        assert_eq!(&p.key[..2], b"AB");
        assert_eq!(p.key[2], 0xFF);
        assert_eq!(p.seq, u64::MAX);
        assert!(pred(&IdxKey::new(b"", 0)).is_none());
        let attr = IndexAttr {
            fixed: true,
            key_len: 4,
            entry_len: 10,
            root: Some(3),
            entry_count: 2,
            next_seq: 3,
        };
        assert_eq!(IndexAttr::decode(&attr.encode()), attr);
    }
}
