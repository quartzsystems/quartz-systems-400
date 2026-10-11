// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Access paths: the encoded key of a record and the member's B+tree of
//! `PathKey`s. See `docs/DB_FORMAT.md` §6.

pub mod encode;

use std::ops::Bound;

use store::PageNo;
use store::btree::{BTree, DynKey};
use store::obj::ObjectId;
use store::page::pager::Pager;

use crate::error::{DbError, Result};
use crate::format::FormatDef;
use crate::record::{self, Value};
use crate::spec::{Field, Layout};

/// The spec layout.
pub static LAYOUT: Layout = Layout {
    name: "PathKey",
    size: None,
    fields: &[
        Field {
            name: "key",
            offset: Some(0),
            size: None,
        },
        Field {
            name: "format",
            offset: None,
            size: Some(2),
        },
        Field {
            name: "rrn",
            offset: None,
            size: Some(8),
        },
    ],
};

/// `MbrAttr.format` of a multi-format path.
pub const MULTI_FORMAT: u16 = 0xFFFF;

/// One entry of an access path.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PathKey {
    /// The encoded key, padded to the path's key length.
    pub key: Vec<u8>,
    /// Format number; 0 in a single-format path.
    pub format: u16,
    /// The record in the based-on member.
    pub rrn: u64,
}

/// A member's access path tree.
#[derive(Clone, Debug)]
pub struct Path {
    /// The member that owns the tree pages.
    pub owner: ObjectId,
    /// Encoded key bytes.
    pub key_len: usize,
    /// Whether entries carry a format number.
    pub multi: bool,
    /// The root, `None` for empty.
    pub root: Option<PageNo>,
}

type Tree = BTree<DynKey, ()>;

impl Path {
    /// Bytes per tree key.
    #[must_use]
    pub fn tree_key_len(&self) -> usize {
        self.key_len + if self.multi { 2 } else { 0 } + 8
    }

    fn tree(&self) -> Tree {
        BTree::with_key_len(self.root, self.owner, self.tree_key_len())
    }

    fn encode(&self, k: &PathKey) -> DynKey {
        let mut out = Vec::with_capacity(self.tree_key_len());
        out.extend_from_slice(&k.key[..k.key.len().min(self.key_len)]);
        out.resize(self.key_len, 0);
        if self.multi {
            out.extend_from_slice(&k.format.to_be_bytes());
        }
        out.extend_from_slice(&k.rrn.to_be_bytes());
        DynKey(out)
    }

    fn decode(&self, k: &DynKey) -> PathKey {
        let b = &k.0;
        let mut at = self.key_len;
        let format = if self.multi {
            at += 2;
            u16::from_be_bytes([b[self.key_len], b[self.key_len + 1]])
        } else {
            0
        };
        PathKey {
            key: b[..self.key_len].to_vec(),
            format,
            rrn: u64::from_be_bytes(b[at..at + 8].try_into().expect("8")),
        }
    }

    /// The lowest tree key with key bytes `prefix` (padded with zeros).
    fn low(&self, prefix: &[u8]) -> DynKey {
        self.encode(&PathKey {
            key: prefix.to_vec(),
            format: 0,
            rrn: 0,
        })
    }

    /// The highest tree key with key bytes `prefix` (padded with 0xFF).
    fn high(&self, prefix: &[u8]) -> DynKey {
        let mut key = prefix[..prefix.len().min(self.key_len)].to_vec();
        key.resize(self.key_len, 0xFF);
        self.encode(&PathKey {
            key,
            format: u16::MAX,
            rrn: u64::MAX,
        })
    }

    /// Add an entry; false when it was already there.
    pub fn insert(&mut self, p: &mut dyn Pager, k: &PathKey) -> Result<bool> {
        let mut tree = self.tree();
        let fresh = tree.insert(p, self.encode(k), &())?.is_none();
        self.root = tree.root();
        Ok(fresh)
    }

    /// Remove an entry; false when it was not there.
    pub fn remove(&mut self, p: &mut dyn Pager, k: &PathKey) -> Result<bool> {
        let mut tree = self.tree();
        let had = tree.delete(p, &self.encode(k))?.is_some();
        self.root = tree.root();
        Ok(had)
    }

    /// Whether any entry has exactly these key bytes.
    pub fn has_key(&self, p: &mut dyn Pager, key: &[u8]) -> Result<bool> {
        let tree = self.tree();
        let mut it = tree.range(p, self.low(key)..=self.high(key))?;
        Ok(it.next().transpose()?.is_some())
    }

    /// Entries whose key bytes lie within the bounds (a shorter bound is
    /// a prefix), ascending, at most `max`.
    pub fn scan(
        &self,
        p: &mut dyn Pager,
        lo: Bound<&[u8]>,
        hi: Bound<&[u8]>,
        max: usize,
    ) -> Result<Vec<PathKey>> {
        let lo = match lo {
            Bound::Unbounded => Bound::Unbounded,
            Bound::Included(k) => Bound::Included(self.low(k)),
            Bound::Excluded(k) => Bound::Excluded(self.high(k)),
        };
        let hi = match hi {
            Bound::Unbounded => Bound::Unbounded,
            Bound::Included(k) => Bound::Included(self.high(k)),
            Bound::Excluded(k) => Bound::Excluded(self.low(k)),
        };
        let tree = self.tree();
        let mut out = Vec::new();
        for r in tree.range(p, (lo, hi))?.take(max) {
            let (k, ()) = r?;
            out.push(self.decode(&k));
        }
        Ok(out)
    }

    /// Entries at or below `hi` (a prefix), descending, at most `max`.
    pub fn scan_back(
        &self,
        p: &mut dyn Pager,
        hi: Bound<&[u8]>,
        max: usize,
    ) -> Result<Vec<PathKey>> {
        let tree = self.tree();
        let mut probe = match hi {
            Bound::Unbounded => Some(DynKey(vec![0xFF; self.tree_key_len()])),
            Bound::Included(k) => Some(self.high(k)),
            Bound::Excluded(k) => pred(&self.low(k)),
        };
        let mut out = Vec::new();
        while out.len() < max {
            let Some(pr) = probe else { break };
            let Some((k, ())) = tree.floor(p, &pr)? else {
                break;
            };
            probe = pred(&k);
            out.push(self.decode(&k));
        }
        Ok(out)
    }

    /// Every entry, ascending.
    pub fn all(&self, p: &mut dyn Pager) -> Result<Vec<PathKey>> {
        self.scan(p, Bound::Unbounded, Bound::Unbounded, usize::MAX)
    }

    /// Replace the tree with `entries`, bulk loaded.
    pub fn load(&mut self, p: &mut dyn Pager, mut entries: Vec<PathKey>) -> Result<()> {
        self.free(p)?;
        let mut keys: Vec<DynKey> = entries.iter().map(|k| self.encode(k)).collect();
        entries.clear();
        keys.sort_unstable();
        keys.dedup();
        let tree = Tree::bulk_load_with_key_len(
            p,
            self.owner,
            self.tree_key_len(),
            keys.into_iter().map(|k| (k, ())),
        )?;
        self.root = tree.root();
        Ok(())
    }

    /// Free every page.
    pub fn free(&mut self, p: &mut dyn Pager) -> Result<()> {
        if let Some(root) = self.root.take() {
            store::btree::free_tree(p, root)?;
        }
        Ok(())
    }

    /// Entries, by walking the leaves.
    pub fn count(&self, p: &mut dyn Pager) -> Result<usize> {
        Ok(self.tree().len(p)?)
    }
}

/// The key just below `k`, as a big integer minus one.
fn pred(k: &DynKey) -> Option<DynKey> {
    let mut out = k.clone();
    for b in out.0.iter_mut().rev() {
        if *b > 0 {
            *b -= 1;
            return Some(out);
        }
        *b = 0xFF;
    }
    None
}

/// The encoded key of a record of `format`.
pub fn record_key(format: &FormatDef, record: &[u8], null_map: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(usize::from(format.key_len));
    for (i, k) in format.format.keys.iter().enumerate() {
        let f = &format.format.fields[k.field];
        let v = record::read_field(f, record, null_map, format.null_index(k.field))?;
        encode::key_part(f, k, &v, &mut out)?;
        if out.len() > usize::from(format.key_len) {
            return Err(DbError::Corrupt {
                what: format!("key part {i} overruns the key of {}", format.format.name),
            });
        }
    }
    Ok(out)
}

/// Whether any key field of the record is null.
#[must_use]
pub fn key_has_null(format: &FormatDef, null_map: &[u8]) -> bool {
    format
        .format
        .keys
        .iter()
        .any(|k| record::is_null(null_map, format.null_index(k.field)))
}

/// The encoded key of the first `values.len()` key fields, as a search
/// prefix (`%KDS` style partial keys).
pub fn values_key(format: &FormatDef, values: &[Value]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    for (k, v) in format.format.keys.iter().zip(values) {
        encode::key_part(&format.format.fields[k.field], k, v, &mut out)?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use store::MemPager;

    fn path(multi: bool) -> Path {
        Path {
            owner: ObjectId::NONE,
            key_len: 3,
            multi,
            root: None,
        }
    }

    fn pk(key: &[u8], format: u16, rrn: u64) -> PathKey {
        PathKey {
            key: key.to_vec(),
            format,
            rrn,
        }
    }

    #[test]
    fn insert_scan_remove() {
        let mut p = MemPager::new(4096);
        let mut path = path(false);
        assert_eq!(path.tree_key_len(), 11);
        for (k, rrn) in [
            (b"ABC", 1),
            (b"ABC", 5),
            (b"ABD", 2),
            (b"B\x00\x00", 3),
            (b"AAA", 4),
        ] {
            assert!(path.insert(&mut p, &pk(k, 0, rrn)).unwrap());
        }
        assert!(!path.insert(&mut p, &pk(b"ABC", 0, 1)).unwrap());
        assert_eq!(path.count(&mut p).unwrap(), 5);
        let all = path.all(&mut p).unwrap();
        let rrns: Vec<u64> = all.iter().map(|k| k.rrn).collect();
        assert_eq!(rrns, [4, 1, 5, 2, 3]);
        assert!(path.has_key(&mut p, b"ABC").unwrap());
        assert!(!path.has_key(&mut p, b"ABE").unwrap());
        // Prefix scans.
        let ab = path
            .scan(&mut p, Bound::Included(b"AB"), Bound::Included(b"AB"), 10)
            .unwrap();
        assert_eq!(ab.iter().map(|k| k.rrn).collect::<Vec<_>>(), [1, 5, 2]);
        let after = path
            .scan(&mut p, Bound::Excluded(b"ABC"), Bound::Unbounded, 10)
            .unwrap();
        assert_eq!(after.iter().map(|k| k.rrn).collect::<Vec<_>>(), [2, 3]);
        let before = path
            .scan(&mut p, Bound::Unbounded, Bound::Excluded(b"ABC"), 10)
            .unwrap();
        assert_eq!(before.iter().map(|k| k.rrn).collect::<Vec<_>>(), [4]);
        let back = path.scan_back(&mut p, Bound::Included(b"ABC"), 10).unwrap();
        assert_eq!(back.iter().map(|k| k.rrn).collect::<Vec<_>>(), [5, 1, 4]);
        let back = path.scan_back(&mut p, Bound::Excluded(b"ABC"), 10).unwrap();
        assert_eq!(back.iter().map(|k| k.rrn).collect::<Vec<_>>(), [4]);
        let back = path.scan_back(&mut p, Bound::Unbounded, 2).unwrap();
        assert_eq!(back.iter().map(|k| k.rrn).collect::<Vec<_>>(), [3, 2]);
        assert!(path.remove(&mut p, &pk(b"ABC", 0, 1)).unwrap());
        assert!(!path.remove(&mut p, &pk(b"ABC", 0, 1)).unwrap());
        assert_eq!(path.count(&mut p).unwrap(), 4);
        // Bulk load replaces.
        path.load(&mut p, vec![pk(b"Z", 0, 9), pk(b"A", 0, 8), pk(b"A", 0, 8)])
            .unwrap();
        let all = path.all(&mut p).unwrap();
        assert_eq!(all.iter().map(|k| k.rrn).collect::<Vec<_>>(), [8, 9]);
        assert_eq!(all[0].key, b"A\x00\x00");
        path.free(&mut p).unwrap();
        assert_eq!(path.count(&mut p).unwrap(), 0);
    }

    #[test]
    fn multi_format_tags() {
        let mut p = MemPager::new(4096);
        let mut path = path(true);
        assert_eq!(path.tree_key_len(), 13);
        path.insert(&mut p, &pk(b"K", 1, 7)).unwrap();
        path.insert(&mut p, &pk(b"K", 0, 7)).unwrap();
        path.insert(&mut p, &pk(b"J", 1, 1)).unwrap();
        let all = path.all(&mut p).unwrap();
        assert_eq!(
            all,
            vec![
                pk(b"J\x00\x00", 1, 1),
                pk(b"K\x00\x00", 0, 7),
                pk(b"K\x00\x00", 1, 7)
            ]
        );
    }
}
