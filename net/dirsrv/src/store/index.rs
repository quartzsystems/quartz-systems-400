// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The index tree: definitions (entry 0), keys kept with every change of
//! an entry, and lookups (presence, equality, substring candidates).

use std::collections::BTreeSet;
use std::ops::Bound;

use store::btree::BTree;
use store::txn::Txn;

use super::db::DirDb;
use super::error::{DirError, Result};
use super::format::{IndexKey, IndexKind, SLOT_ENTRIES, SLOT_INDEX};
use super::normalize::{Normalizer, Rule};
use super::record::{self, Entry, IndexDef, MAX_INDEX_NAME};

type Index = BTree<IndexKey, Vec<u8>>;

/// Gram sentinels: before a value and after it.
const START: u8 = 0x02;
const END: u8 = 0x03;

/// Entries re-indexed per batch when an index is added.
const BATCH: usize = 256;

/// The grams of a value normalized under its substrings rule.
fn grams(v: &[u8]) -> Vec<Vec<u8>> {
    let mut b = Vec::with_capacity(v.len() + 2);
    b.push(START);
    b.extend_from_slice(v);
    b.push(END);
    b.windows(3).map(<[u8]>::to_vec).collect()
}

/// The grams a filter piece of at least three bytes gives.
fn piece_grams(p: &[u8]) -> Vec<Vec<u8>> {
    p.windows(3).map(<[u8]>::to_vec).collect()
}

/// Every index key of `entry` (ID `id`) under `defs`.
fn keys_of(
    norm: &dyn Normalizer,
    defs: &[IndexDef],
    id: u64,
    entry: &Entry,
) -> Result<BTreeSet<IndexKey>> {
    let mut out = BTreeSet::new();
    for a in &entry.attrs {
        let Some(attr) = norm.attr_id(&a.name) else {
            continue;
        };
        for d in defs.iter().filter(|d| d.attr == attr) {
            if d.has(IndexKind::Presence) {
                out.insert(IndexKey::new(d.number, IndexKind::Presence, &[], id));
            }
            for v in &a.values {
                if d.has(IndexKind::Equality)
                    && let Some(k) = norm.value_key(&attr, v, Rule::Equality)?
                {
                    out.insert(IndexKey::new(d.number, IndexKind::Equality, &k, id));
                }
                if d.has(IndexKind::Substring)
                    && let Some(k) = norm.value_key(&attr, v, Rule::Substring)?
                {
                    for g in grams(&k) {
                        out.insert(IndexKey::new(d.number, IndexKind::Substring, &g, id));
                    }
                }
            }
        }
    }
    Ok(out)
}

/// Sorted intersection.
fn intersect(a: &[u64], b: &[u64]) -> Vec<u64> {
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                out.push(a[i]);
                i += 1;
                j += 1;
            }
        }
    }
    out
}

impl DirDb {
    /// The index definitions, in number order.
    ///
    /// # Errors
    ///
    /// The object store's, or a damaged entry 0.
    pub fn indexes(&self, txn: &mut Txn<'_>) -> Result<Vec<IndexDef>> {
        let b = self
            .read_record(txn, SLOT_ENTRIES, 0)?
            .ok_or_else(|| DirError::Corrupt("no index definitions".into()))?;
        record::decode_defs(&b)
    }

    /// The definition indexing attribute `attr` (an `attr_id`) for
    /// `kind`, if any.
    ///
    /// # Errors
    ///
    /// The object store's.
    pub fn index_for(
        &self,
        txn: &mut Txn<'_>,
        attr: &str,
        kind: IndexKind,
    ) -> Result<Option<IndexDef>> {
        Ok(self
            .indexes(txn)?
            .into_iter()
            .find(|d| d.attr == attr && d.has(kind)))
    }

    /// Bring the index keys of entry `id` from those of `old` to those of
    /// `new` (`None` for none).
    pub(crate) fn reindex(
        &self,
        txn: &mut Txn<'_>,
        norm: &dyn Normalizer,
        id: u64,
        old: Option<&Entry>,
        new: Option<&Entry>,
    ) -> Result<()> {
        let defs = self.indexes(txn)?;
        let before = match old {
            Some(e) => keys_of(norm, &defs, id, e)?,
            None => BTreeSet::new(),
        };
        let after = match new {
            Some(e) => keys_of(norm, &defs, id, e)?,
            None => BTreeSet::new(),
        };
        self.apply(txn, &before, &after)
    }

    fn apply(
        &self,
        txn: &mut Txn<'_>,
        before: &BTreeSet<IndexKey>,
        after: &BTreeSet<IndexKey>,
    ) -> Result<()> {
        let mut t: Index = self.tree(txn, SLOT_INDEX)?;
        let mut delta: i64 = 0;
        for k in before.difference(after) {
            if t.delete(txn, k)?.is_some() {
                delta -= 1;
            }
        }
        for k in after.difference(before) {
            if t.insert(txn, *k, &Vec::new())?.is_none() {
                delta += 1;
            }
        }
        if delta != 0 {
            self.set_root(txn, SLOT_INDEX, t.root())?;
            let mut attr = self.attr(txn)?;
            attr.index_keys = attr.index_keys.saturating_add_signed(delta);
            self.save_attr(txn, &attr)?;
        }
        Ok(())
    }

    /// Index attribute `attr` (an `attr_id`) for `kinds`
    /// (`IndexKind::bit`s), building its keys from every entry. Returns
    /// its number.
    ///
    /// # Errors
    ///
    /// The attribute already indexed, a name past the limit, no kinds.
    pub fn add_index(
        &self,
        txn: &mut Txn<'_>,
        norm: &dyn Normalizer,
        attr: &str,
        kinds: u8,
    ) -> Result<u16> {
        let mut defs = self.indexes(txn)?;
        if defs.iter().any(|d| d.attr == attr) {
            return Err(DirError::AlreadyExists);
        }
        let kinds = kinds & 0x07;
        if attr.is_empty() || attr.len() > MAX_INDEX_NAME || kinds == 0 {
            return Err(DirError::Unwilling(format!("index on {attr:?}")));
        }
        let mut a = self.attr(txn)?;
        let number = a.next_index;
        a.next_index = number
            .checked_add(1)
            .ok_or_else(|| DirError::Limit("index numbers used up".into()))?;
        a.index_count += 1;
        self.save_attr(txn, &a)?;
        let def = IndexDef {
            number,
            attr: attr.into(),
            kinds,
            always: false,
        };
        defs.push(def.clone());
        self.write_record(txn, SLOT_ENTRIES, 0, &record::encode_defs(&defs)?)?;
        let one = [def];
        let mut from = 1;
        loop {
            let ids = self.ids(txn, from, BATCH)?;
            let Some(&last) = ids.last() else { break };
            let mut keys = BTreeSet::new();
            for id in ids {
                let e = self.must_get(txn, id)?;
                keys.extend(keys_of(norm, &one, id, &e)?);
            }
            self.apply(txn, &BTreeSet::new(), &keys)?;
            from = last + 1;
        }
        Ok(number)
    }

    /// Drop index `number` and its keys. The always-on indexes stay.
    ///
    /// # Errors
    ///
    /// No such index, or an always-on one.
    pub fn remove_index(&self, txn: &mut Txn<'_>, number: u16) -> Result<()> {
        let mut defs = self.indexes(txn)?;
        let Some(at) = defs.iter().position(|d| d.number == number) else {
            return Err(DirError::Unwilling(format!("no index {number}")));
        };
        if defs[at].always {
            return Err(DirError::Unwilling(format!(
                "index {} is always kept",
                defs[at].attr
            )));
        }
        defs.remove(at);
        self.write_record(txn, SLOT_ENTRIES, 0, &record::encode_defs(&defs)?)?;
        let mut a = self.attr(txn)?;
        a.index_count -= 1;
        self.save_attr(txn, &a)?;
        loop {
            let t: Index = self.tree(txn, SLOT_INDEX)?;
            let keys: BTreeSet<IndexKey> = t
                .range(
                    txn,
                    IndexKey::index_low(number)..=IndexKey::index_high(number),
                )?
                .take(4096)
                .map(|r| r.map(|(k, _)| k))
                .collect::<store::Result<_>>()?;
            if keys.is_empty() {
                return Ok(());
            }
            self.apply(txn, &keys, &BTreeSet::new())?;
        }
    }

    /// Entry IDs under one key, ascending, after `after`, at most `max`.
    fn postings(
        &self,
        txn: &mut Txn<'_>,
        key: IndexKey,
        after: Option<u64>,
        max: usize,
    ) -> Result<Vec<u64>> {
        let t: Index = self.tree(txn, SLOT_INDEX)?;
        let lo = match after {
            Some(a) if a == u64::MAX => return Ok(Vec::new()),
            Some(a) => Bound::Excluded(key.with_entry(a)),
            None => Bound::Included(key.with_entry(0)),
        };
        let hi = Bound::Included(key.with_entry(u64::MAX));
        t.range(txn, (lo, hi))?
            .take(max)
            .map(|r| r.map(|(k, _)| k.entry()).map_err(DirError::from))
            .collect()
    }

    /// Entries holding attribute `attr` (an `attr_id`), ascending, after
    /// `after`, at most `max`; `None` when no index keeps its presence.
    ///
    /// # Errors
    ///
    /// The object store's.
    pub fn present(
        &self,
        txn: &mut Txn<'_>,
        attr: &str,
        after: Option<u64>,
        max: usize,
    ) -> Result<Option<Vec<u64>>> {
        let Some(d) = self.index_for(txn, attr, IndexKind::Presence)? else {
            return Ok(None);
        };
        let key = IndexKey::new(d.number, IndexKind::Presence, &[], 0);
        self.postings(txn, key, after, max).map(Some)
    }

    /// Entries with a value of `attr` equal to `value` (as the client
    /// wrote it), ascending, after `after`, at most `max`; `None` when no
    /// index keeps its equality.
    ///
    /// # Errors
    ///
    /// A value the normalizer refuses, the object store's.
    pub fn equal(
        &self,
        txn: &mut Txn<'_>,
        norm: &dyn Normalizer,
        attr: &str,
        value: &[u8],
        after: Option<u64>,
        max: usize,
    ) -> Result<Option<Vec<u64>>> {
        let Some(d) = self.index_for(txn, attr, IndexKind::Equality)? else {
            return Ok(None);
        };
        let Some(k) = norm.value_key(attr, value, Rule::Equality)? else {
            return Ok(None);
        };
        let key = IndexKey::new(d.number, IndexKind::Equality, &k, 0);
        self.postings(txn, key, after, max).map(Some)
    }

    /// The candidates of a substring filter on `attr`: every entry whose
    /// values hold all the pieces' grams, ascending. `None` when no index
    /// keeps its substrings or the pieces are too short to give a gram;
    /// the caller then filters some other way. Candidates must still be
    /// matched.
    ///
    /// # Errors
    ///
    /// A piece the normalizer refuses, the object store's.
    pub fn substring_candidates(
        &self,
        txn: &mut Txn<'_>,
        norm: &dyn Normalizer,
        attr: &str,
        initial: Option<&[u8]>,
        any: &[&[u8]],
        last: Option<&[u8]>,
    ) -> Result<Option<Vec<u64>>> {
        let Some(d) = self.index_for(txn, attr, IndexKind::Substring)? else {
            return Ok(None);
        };
        let mut gs = BTreeSet::new();
        let norm_piece =
            |p: &[u8]| -> Result<Option<Vec<u8>>> { Ok(norm.value_key(attr, p, Rule::Substring)?) };
        if let Some(p) = initial
            && let Some(p) = norm_piece(p)?
        {
            let mut b = vec![START];
            b.extend_from_slice(&p);
            gs.extend(piece_grams(&b));
        }
        for p in any {
            if let Some(p) = norm_piece(p)? {
                gs.extend(piece_grams(&p));
            }
        }
        if let Some(p) = last
            && let Some(mut p) = norm_piece(p)?
        {
            p.push(END);
            gs.extend(piece_grams(&p));
        }
        let mut found: Option<Vec<u64>> = None;
        for g in gs {
            let key = IndexKey::new(d.number, IndexKind::Substring, &g, 0);
            let ids = self.postings(txn, key, None, usize::MAX)?;
            let next = match found {
                None => ids,
                Some(f) => intersect(&f, &ids),
            };
            if next.is_empty() {
                return Ok(Some(next));
            }
            found = Some(next);
        }
        Ok(found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grams_and_intersections() {
        assert_eq!(grams(b"ab"), [b"\x02ab".to_vec(), b"ab\x03".to_vec()]);
        assert_eq!(grams(b""), Vec::<Vec<u8>>::new());
        assert!(piece_grams(b"ab").is_empty());
        assert_eq!(intersect(&[1, 3, 5, 7], &[3, 4, 7]), [3, 7]);
    }
}
