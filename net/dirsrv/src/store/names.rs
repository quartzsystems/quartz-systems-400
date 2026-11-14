// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The names tree: suffixes, DNs to entry IDs, adding, renaming, moving
//! and deleting entries, children and subtrees in pages.

use std::ops::Bound;

use ldap::{Dn, Rdn};
use store::btree::BTree;
use store::txn::Txn;

use super::db::DirDb;
use super::error::{DirError, Result};
use super::format::{MAX_SUFFIXES, NameKey, SLOT_NAMES};
use super::normalize::Normalizer;
use super::record::Entry;

type Names = BTree<NameKey, u64>;

/// What a DN resolved to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolved {
    /// The entry, when every RDN was found.
    pub entry: Option<u64>,
    /// The deepest entry found (the matched DN of a `noSuchObject`) and
    /// how many of the DN's RDNs, counted from its end, name it; `None`
    /// when the DN is under no suffix.
    pub matched: Option<(u64, usize)>,
}

/// Where a children listing stopped: the last key answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChildCursor(NameKey);

/// Where a subtree walk stopped: for each level of the path from the
/// base, the parent and the last child answered under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkCursor {
    stack: Vec<(u64, Option<NameKey>)>,
}

/// A page of entry IDs and where to go on, `None` when the listing is
/// done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<C> {
    /// The IDs, in order.
    pub ids: Vec<u64>,
    /// The cursor for the next page.
    pub next: Option<C>,
}

/// A suffix's key: each RDN's key, most specific first, length-prefixed.
fn suffix_key(keys: &[Vec<u8>]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    for k in keys {
        let n = u16::try_from(k.len())
            .map_err(|_| DirError::Limit(format!("normalized RDN of {} bytes", k.len())))?;
        out.extend_from_slice(&n.to_be_bytes());
        out.extend_from_slice(k);
    }
    Ok(out)
}

fn rdn_keys(norm: &dyn Normalizer, rdns: &[Rdn]) -> Result<Vec<Vec<u8>>> {
    rdns.iter().map(|r| Ok(norm.rdn_key(r)?)).collect()
}

impl DirDb {
    fn names(&self, txn: &mut Txn<'_>) -> Result<Names> {
        self.tree(txn, SLOT_NAMES)
    }

    fn name_get(&self, txn: &mut Txn<'_>, key: &NameKey) -> Result<Option<u64>> {
        let t = self.names(txn)?;
        Ok(t.get(txn, key)?)
    }

    fn name_put(&self, txn: &mut Txn<'_>, key: NameKey, id: u64) -> Result<()> {
        let mut t = self.names(txn)?;
        t.insert(txn, key, &id)?;
        self.set_root(txn, SLOT_NAMES, t.root())
    }

    fn name_drop(&self, txn: &mut Txn<'_>, key: &NameKey) -> Result<()> {
        let mut t = self.names(txn)?;
        t.delete(txn, key)?;
        self.set_root(txn, SLOT_NAMES, t.root())
    }

    /// The suffixes' root entries, in the order they were added.
    ///
    /// # Errors
    ///
    /// The object store's.
    pub fn suffixes(&self, txn: &mut Txn<'_>) -> Result<Vec<u64>> {
        Ok(self.attr(txn)?.suffixes)
    }

    /// Add suffix `dn` with `entry` as its root entry (its `rdn` the
    /// suffix's DN as written). Returns the root's ID.
    ///
    /// # Errors
    ///
    /// The suffix exists, 16 suffixes already, the empty DN, a name the
    /// normalizer refuses, a limit.
    pub fn add_suffix(
        &self,
        txn: &mut Txn<'_>,
        norm: &dyn Normalizer,
        dn: &Dn,
        entry: &Entry,
    ) -> Result<u64> {
        if dn.is_root() {
            return Err(DirError::Unwilling("the empty DN is not a suffix".into()));
        }
        if self.attr(txn)?.suffixes.len() >= MAX_SUFFIXES {
            return Err(DirError::Limit(format!(
                "more than {MAX_SUFFIXES} suffixes"
            )));
        }
        let skey = suffix_key(&rdn_keys(norm, &dn.0)?)?;
        let key = NameKey::new(0, &skey);
        if self.name_get(txn, &key)?.is_some() {
            return Err(DirError::AlreadyExists);
        }
        let e = Entry {
            parent: 0,
            suffix: true,
            rdn_key: skey,
            ..entry.clone()
        };
        let id = self.insert_new(txn, norm, &e)?;
        self.name_put(txn, key, id)?;
        let mut attr = self.attr(txn)?;
        attr.suffixes.push(id);
        self.save_attr(txn, &attr)?;
        Ok(id)
    }

    /// Resolve `dn`: the suffix that ends it (the longest, when suffixes
    /// nest), then one RDN at a time down the names tree.
    ///
    /// # Errors
    ///
    /// An RDN the normalizer refuses, the object store's.
    pub fn resolve(&self, txn: &mut Txn<'_>, norm: &dyn Normalizer, dn: &Dn) -> Result<Resolved> {
        let n = dn.0.len();
        let none = Resolved {
            entry: None,
            matched: None,
        };
        if n == 0 {
            return Ok(none);
        }
        let keys = rdn_keys(norm, &dn.0)?;
        for k in (1..=n).rev() {
            let skey = suffix_key(&keys[n - k..])?;
            let Some(mut cur) = self.name_get(txn, &NameKey::new(0, &skey))? else {
                continue;
            };
            for i in (0..n - k).rev() {
                match self.name_get(txn, &NameKey::new(cur, &keys[i]))? {
                    Some(c) => cur = c,
                    None => {
                        return Ok(Resolved {
                            entry: None,
                            matched: Some((cur, n - i - 1)),
                        });
                    }
                }
            }
            return Ok(Resolved {
                entry: Some(cur),
                matched: Some((cur, n)),
            });
        }
        Ok(none)
    }

    /// Add `entry` under `parent` as `rdn` (`entry.rdn` its text). Returns
    /// the new entry's ID.
    ///
    /// # Errors
    ///
    /// No such parent, the name taken, a name or value the normalizer
    /// refuses, a limit.
    pub fn add(
        &self,
        txn: &mut Txn<'_>,
        norm: &dyn Normalizer,
        parent: u64,
        rdn: &Rdn,
        entry: &Entry,
    ) -> Result<u64> {
        if !self.exists(txn, parent)? {
            return Err(DirError::NoSuchEntry(parent));
        }
        let rkey = norm.rdn_key(rdn)?;
        let key = NameKey::new(parent, &rkey);
        if self.name_get(txn, &key)?.is_some() {
            return Err(DirError::AlreadyExists);
        }
        let e = Entry {
            parent,
            suffix: false,
            rdn_key: rkey,
            ..entry.clone()
        };
        let id = self.insert_new(txn, norm, &e)?;
        self.name_put(txn, key, id)?;
        Ok(id)
    }

    fn key_of(e: &Entry) -> NameKey {
        NameKey::new(e.parent, &e.rdn_key)
    }

    /// Rename entry `id` to `rdn` (written `rdn_text`) and, with
    /// `new_parent`, move it there: one names key changes, whatever the
    /// subtree's size. The attributes stay; the caller changes the RDN's
    /// values with [`put`](Self::put).
    ///
    /// # Errors
    ///
    /// No such entry or parent, the new name taken, a suffix's root, a
    /// move under the entry itself.
    pub fn rename(
        &self,
        txn: &mut Txn<'_>,
        norm: &dyn Normalizer,
        id: u64,
        rdn: &Rdn,
        rdn_text: &str,
        new_parent: Option<u64>,
    ) -> Result<()> {
        let old = self.must_get(txn, id)?;
        if old.suffix {
            return Err(DirError::Unwilling("a suffix's root is not renamed".into()));
        }
        let parent = new_parent.unwrap_or(old.parent);
        if parent != old.parent {
            if !self.exists(txn, parent)? {
                return Err(DirError::NoSuchEntry(parent));
            }
            let mut up = parent;
            while up != 0 {
                if up == id {
                    return Err(DirError::Unwilling(
                        "an entry is not moved under itself".into(),
                    ));
                }
                up = self.must_get(txn, up)?.parent;
            }
        }
        let rkey = norm.rdn_key(rdn)?;
        let key = NameKey::new(parent, &rkey);
        let old_key = Self::key_of(&old);
        if key != old_key && self.name_get(txn, &key)?.is_some() {
            return Err(DirError::AlreadyExists);
        }
        let new = Entry {
            parent,
            rdn: rdn_text.into(),
            rdn_key: rkey,
            ..old
        };
        self.write_record(txn, super::format::SLOT_ENTRIES, id, &new.encode()?)?;
        if key != old_key {
            self.name_drop(txn, &old_key)?;
            self.name_put(txn, key, id)?;
        }
        Ok(())
    }

    /// Delete entry `id`, a leaf; a suffix's root goes from the suffixes.
    ///
    /// # Errors
    ///
    /// No such entry, or it has children.
    pub fn delete(&self, txn: &mut Txn<'_>, norm: &dyn Normalizer, id: u64) -> Result<()> {
        let old = self.must_get(txn, id)?;
        if self.has_children(txn, id)? {
            return Err(DirError::NotLeaf(id));
        }
        self.name_drop(txn, &Self::key_of(&old))?;
        self.remove_record(txn, norm, id, &old)?;
        if old.suffix {
            let mut attr = self.attr(txn)?;
            attr.suffixes.retain(|s| *s != id);
            self.save_attr(txn, &attr)?;
        }
        Ok(())
    }

    /// Whether entry `id` has children.
    ///
    /// # Errors
    ///
    /// The object store's.
    pub fn has_children(&self, txn: &mut Txn<'_>, id: u64) -> Result<bool> {
        Ok(self.next_child(txn, id, None)?.is_some())
    }

    fn next_child(
        &self,
        txn: &mut Txn<'_>,
        parent: u64,
        after: Option<&NameKey>,
    ) -> Result<Option<(NameKey, u64)>> {
        let t = self.names(txn)?;
        let lo = after.map_or(Bound::Included(NameKey::low(parent)), |k| {
            Bound::Excluded(*k)
        });
        let mut it = t.range(txn, (lo, Bound::Included(NameKey::high(parent))))?;
        Ok(it.next().transpose()?)
    }

    /// Up to `max` children of `parent` in key order, from where `from`
    /// stopped.
    ///
    /// # Errors
    ///
    /// The object store's.
    pub fn children(
        &self,
        txn: &mut Txn<'_>,
        parent: u64,
        from: Option<&ChildCursor>,
        max: usize,
    ) -> Result<Page<ChildCursor>> {
        let t = self.names(txn)?;
        let lo = from.map_or(Bound::Included(NameKey::low(parent)), |c| {
            Bound::Excluded(c.0)
        });
        let mut ids = Vec::new();
        let mut last = None;
        let mut more = false;
        for r in t.range(txn, (lo, Bound::Included(NameKey::high(parent))))? {
            let (k, id) = r?;
            if ids.len() == max {
                more = true;
                break;
            }
            ids.push(id);
            last = Some(k);
        }
        Ok(Page {
            ids,
            next: if more { last.map(ChildCursor) } else { None },
        })
    }

    /// Up to `max` entries of `base`'s subtree, `base` first, each
    /// parent before its children, from where `from` stopped.
    ///
    /// # Errors
    ///
    /// The object store's.
    pub fn subtree(
        &self,
        txn: &mut Txn<'_>,
        base: u64,
        from: Option<&WalkCursor>,
        max: usize,
    ) -> Result<Page<WalkCursor>> {
        let mut ids = Vec::new();
        // A cursor with an empty stack has answered nothing yet.
        let mut stack = if let Some(c) = from.filter(|c| !c.stack.is_empty()) {
            c.stack.clone()
        } else {
            if max == 0 {
                return Ok(Page {
                    ids,
                    next: Some(WalkCursor { stack: Vec::new() }),
                });
            }
            ids.push(base);
            vec![(base, None)]
        };
        while let Some(&(parent, after)) = stack.last() {
            if ids.len() == max {
                return Ok(Page {
                    ids,
                    next: Some(WalkCursor { stack }),
                });
            }
            match self.next_child(txn, parent, after.as_ref())? {
                Some((k, child)) => {
                    stack.last_mut().expect("non-empty").1 = Some(k);
                    ids.push(child);
                    stack.push((child, None));
                }
                None => {
                    stack.pop();
                }
            }
        }
        Ok(Page { ids, next: None })
    }

    /// Entry `id`'s DN as written, its RDNs joined from the entry up.
    ///
    /// # Errors
    ///
    /// No such entry, or a broken parent chain.
    pub fn dn_of(&self, txn: &mut Txn<'_>, id: u64) -> Result<String> {
        let mut parts = Vec::new();
        let mut cur = id;
        while cur != 0 {
            let e = self.must_get(txn, cur)?;
            parts.push(e.rdn);
            cur = e.parent;
            if parts.len() > 10_000 {
                return Err(DirError::Corrupt(format!("entry {id}: parent loop")));
            }
        }
        Ok(parts.join(","))
    }
}
