// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A copy-on-write B+tree over a [`Pager`]. Nothing is modified in
//! place: a change rewrites the touched nodes to fresh pages and the
//! tree's new root is returned to the caller, who publishes it through
//! the transaction. Pages of the old version stay intact until the next
//! checkpoint, so a crash before commit leaves the old tree readable.

pub mod codec;
mod iter;
pub mod node;

use std::marker::PhantomData;
use std::ops::{Bound, RangeBounds};

use crate::obj::ObjectId;
use crate::page::pager::{PageNo, Pager};
use crate::page::{Page, PageType};
use crate::{Result, StoreError};

pub use codec::{DynKey, FixedKey, Value};
pub use iter::Iter;
use node::{MAX_KEY_LEN, MAX_VALUE_LEN, Node, NodeView};

/// A tree: its root page and owner. Cheap to copy; the pages are on the
/// device.
#[derive(Debug, Clone)]
pub struct BTree<K, V> {
    root: Option<PageNo>,
    owner: ObjectId,
    key_len: usize,
    _marker: PhantomData<(K, V)>,
}

/// Result of inserting into a subtree.
enum Put<K> {
    /// The subtree was rewritten to this page.
    One(PageNo),
    /// The subtree split: left page, separator, right page.
    Split(PageNo, K, PageNo),
}

/// Result of deleting from a subtree.
struct Took {
    /// The subtree's new page.
    page: PageNo,
    /// The value removed, if the key was present.
    old: Option<Vec<u8>>,
    /// Whether the new node is small enough to merge.
    underfull: bool,
}

impl<K: FixedKey, V: Value> BTree<K, V> {
    /// An empty tree owned by `owner` (`ObjectId::NONE` for metadata).
    #[must_use]
    pub fn new(owner: ObjectId) -> Self {
        Self::with_key_len(None, owner, K::LEN)
    }

    /// A tree whose root was published earlier.
    #[must_use]
    pub fn open(root: Option<PageNo>, owner: ObjectId) -> Self {
        Self::with_key_len(root, owner, K::LEN)
    }

    /// A tree whose keys are `key_len` bytes, decided at run time (for
    /// [`DynKey`]); `root` was published earlier or is `None`.
    #[must_use]
    pub fn with_key_len(root: Option<PageNo>, owner: ObjectId, key_len: usize) -> Self {
        assert!(
            key_len > 0 && key_len <= MAX_KEY_LEN,
            "key length {key_len}"
        );
        Self {
            root,
            owner,
            key_len,
            _marker: PhantomData,
        }
    }

    /// Bytes per key.
    #[must_use]
    pub fn key_len(&self) -> usize {
        self.key_len
    }

    /// The root page, to publish after a change. `None` is empty.
    #[must_use]
    pub fn root(&self) -> Option<PageNo> {
        self.root
    }

    /// Whether the tree has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    /// The value for `key`.
    pub fn get(&self, p: &mut dyn Pager, key: &K) -> Result<Option<V>> {
        let Some(mut pno) = self.root else {
            return Ok(None);
        };
        loop {
            let page = self.page(p, pno)?;
            let node = NodeView::open(page.body(), pno, self.key_len)?;
            if node.is_leaf() {
                let i = node.below(key);
                if i < node.count() && node.key::<K>(i) == *key {
                    return Ok(Some(V::from_bytes(node.value(i)?)?));
                }
                return Ok(None);
            }
            pno = node.child(node.at_most(key));
        }
    }

    /// The entry with the greatest key `<= key`.
    pub fn floor(&self, p: &mut dyn Pager, key: &K) -> Result<Option<(K, V)>> {
        let Some(mut pno) = self.root else {
            return Ok(None);
        };
        // Path of (page, chosen child index) down to the leaf.
        let mut path: Vec<(Page, PageNo, usize)> = Vec::new();
        loop {
            let page = self.page(p, pno)?;
            let node = NodeView::open(page.body(), pno, self.key_len)?;
            if node.is_leaf() {
                let i = node.at_most(key);
                if i > 0 {
                    return Ok(Some((node.key(i - 1), V::from_bytes(node.value(i - 1)?)?)));
                }
                break;
            }
            let i = node.at_most(key);
            let child = node.child(i);
            path.push((page, pno, i));
            pno = child;
        }
        // The leaf holds nothing <= key: the answer is the last entry of
        // the nearest subtree to the left, if there is one.
        while let Some((page, at, i)) = path.pop() {
            if i == 0 {
                continue;
            }
            let mut pno = NodeView::open(page.body(), at, self.key_len)?.child(i - 1);
            loop {
                let page = self.page(p, pno)?;
                let node = NodeView::open(page.body(), pno, self.key_len)?;
                if node.is_leaf() {
                    let Some(last) = node.count().checked_sub(1) else {
                        return Err(StoreError::Corrupt {
                            page: pno,
                            what: "empty leaf below an internal node".into(),
                        });
                    };
                    return Ok(Some((node.key(last), V::from_bytes(node.value(last)?)?)));
                }
                pno = node.child(node.count());
            }
        }
        Ok(None)
    }

    /// Insert or replace. Returns the previous value.
    pub fn insert(&mut self, p: &mut dyn Pager, key: K, value: &V) -> Result<Option<V>> {
        let bytes = value.to_bytes();
        if bytes.len() > MAX_VALUE_LEN {
            return Err(StoreError::ValueTooLong { len: bytes.len() });
        }
        let Some(root) = self.root else {
            let node = Node::Leaf(vec![(key, bytes)]);
            self.root = Some(self.store(p, &node)?);
            return Ok(None);
        };
        let (put, old) = self.insert_in(p, root, key, bytes)?;
        self.root = Some(match put {
            Put::One(pno) => pno,
            Put::Split(left, sep, right) => self.store(
                p,
                &Node::Internal {
                    first: left,
                    entries: vec![(sep, right)],
                },
            )?,
        });
        old.map(|b| V::from_bytes(&b)).transpose()
    }

    /// Remove `key`. Returns the value it had.
    pub fn delete(&mut self, p: &mut dyn Pager, key: &K) -> Result<Option<V>> {
        let Some(root) = self.root else {
            return Ok(None);
        };
        let took = self.delete_in(p, root, key)?;
        if took.old.is_none() {
            return Ok(None);
        }
        // Collapse the root: an internal root with one child, or an
        // empty leaf.
        self.root = Some(took.page);
        match self.load(p, took.page)? {
            Node::Internal { first, entries } if entries.is_empty() => {
                p.free(took.page)?;
                self.root = Some(first);
            }
            Node::Leaf(entries) if entries.is_empty() => {
                p.free(took.page)?;
                self.root = None;
            }
            _ => {}
        }
        took.old.map(|b| V::from_bytes(&b)).transpose()
    }

    /// Entries within `range`, ascending.
    pub fn range<'p, R: RangeBounds<K>>(
        &self,
        p: &'p mut dyn Pager,
        range: R,
    ) -> Result<Iter<'p, K, V>> {
        let start = clone_bound(range.start_bound());
        let end = clone_bound(range.end_bound());
        Iter::new(p, self.root, self.key_len, &start, end)
    }

    /// Every entry, ascending.
    pub fn entries<'p>(&self, p: &'p mut dyn Pager) -> Result<Iter<'p, K, V>> {
        self.range(p, ..)
    }

    /// Build a tree from strictly ascending entries, leaves filled to
    /// about 90%.
    pub fn bulk_load<I>(p: &mut dyn Pager, owner: ObjectId, items: I) -> Result<Self>
    where
        I: IntoIterator<Item = (K, V)>,
    {
        Self::bulk_load_with_key_len(p, owner, K::LEN, items)
    }

    /// [`bulk_load`](Self::bulk_load) with a run-time key length.
    pub fn bulk_load_with_key_len<I>(
        p: &mut dyn Pager,
        owner: ObjectId,
        key_len: usize,
        items: I,
    ) -> Result<Self>
    where
        I: IntoIterator<Item = (K, V)>,
    {
        let mut tree = Self::with_key_len(None, owner, key_len);
        // Leaves.
        let mut level: Vec<(K, PageNo)> = Vec::new();
        let mut leaf: Vec<(K, Vec<u8>)> = Vec::new();
        let mut used = node::HEADER_LEN;
        let mut prev: Option<K> = None;
        for (k, v) in items {
            if prev.as_ref().is_some_and(|q| q >= &k) {
                return Err(StoreError::Unsorted);
            }
            prev = Some(k.clone());
            let bytes = v.to_bytes();
            if bytes.len() > MAX_VALUE_LEN {
                return Err(StoreError::ValueTooLong { len: bytes.len() });
            }
            let cost = key_len + node::SLOT_EXTRA + bytes.len();
            if !leaf.is_empty() && used + cost > node::BULK_FILL {
                let node = Node::Leaf(std::mem::take(&mut leaf));
                level.push((
                    node.first_key().expect("non-empty").clone(),
                    tree.store(p, &node)?,
                ));
                used = node::HEADER_LEN;
            }
            used += cost;
            leaf.push((k, bytes));
        }
        if leaf.is_empty() {
            return Ok(tree);
        }
        let node = Node::Leaf(leaf);
        level.push((
            node.first_key().expect("non-empty").clone(),
            tree.store(p, &node)?,
        ));
        // Internal levels until one node remains.
        let per_node =
            ((node::BULK_FILL - node::HEADER_LEN - 8) / (key_len + node::ENTRY_EXTRA)).max(1) + 1;
        while level.len() > 1 {
            let mut next = Vec::new();
            for group in level.chunks(per_node) {
                let node = Node::Internal {
                    first: group[0].1,
                    entries: group[1..].to_vec(),
                };
                next.push((group[0].0.clone(), tree.store(p, &node)?));
            }
            level = next;
        }
        tree.root = Some(level[0].1);
        Ok(tree)
    }

    /// Free every page of the tree.
    pub fn free_all(&mut self, p: &mut dyn Pager) -> Result<()> {
        let Some(root) = self.root.take() else {
            return Ok(());
        };
        let mut stack = vec![root];
        while let Some(pno) = stack.pop() {
            if let Node::Internal { first, entries } = self.load(p, pno)? {
                stack.push(first);
                stack.extend(entries.iter().map(|(_, c)| *c));
            }
            p.free(pno)?;
        }
        Ok(())
    }
}

/// Every page of the tree rooted at `root`, whatever its key type:
/// only the node shape is read.
pub fn walk_pages(p: &mut dyn Pager, root: PageNo) -> Result<Vec<PageNo>> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(pno) = stack.pop() {
        out.push(pno);
        let page = p.read(pno)?;
        page.verify_type(pno, PageType::BTreeNode)?;
        stack.extend(node::children(page.body(), pno)?);
    }
    Ok(out)
}

/// The key length of the tree rooted at `root`, from its root node.
pub fn key_len_at(p: &mut dyn Pager, root: PageNo) -> Result<usize> {
    let page = p.read(root)?;
    page.verify_type(root, PageType::BTreeNode)?;
    let b = page.body();
    Ok(usize::from(u16::from_le_bytes([b[4], b[5]])))
}

/// Raw key and value bytes of a tree's entries.
pub type RawEntries = Vec<(Vec<u8>, Vec<u8>)>;

/// Every entry of the tree rooted at `root` as raw key and value bytes,
/// ascending, with the key length: what a save writes.
pub fn dump(p: &mut dyn Pager, root: PageNo) -> Result<(usize, RawEntries)> {
    let key_len = key_len_at(p, root)?;
    let tree: BTree<DynKey, Vec<u8>> = BTree::with_key_len(Some(root), ObjectId::NONE, key_len);
    let mut out = Vec::new();
    for e in tree.entries(p)? {
        let (k, v) = e?;
        out.push((k.0, v));
    }
    Ok((key_len, out))
}

/// Build a tree owned by `owner` from strictly ascending raw entries:
/// what a restore writes. `None` for no entries.
pub fn load(
    p: &mut dyn Pager,
    owner: ObjectId,
    key_len: usize,
    entries: RawEntries,
) -> Result<Option<PageNo>> {
    if entries.is_empty() {
        return Ok(None);
    }
    let tree = BTree::<DynKey, Vec<u8>>::bulk_load_with_key_len(
        p,
        owner,
        key_len,
        entries.into_iter().map(|(k, v)| (DynKey(k), v)),
    )?;
    Ok(tree.root())
}

/// Free every page of the tree rooted at `root`, whatever its key type.
pub fn free_tree(p: &mut dyn Pager, root: PageNo) -> Result<()> {
    for pno in walk_pages(p, root)? {
        p.free(pno)?;
    }
    Ok(())
}

impl<K: FixedKey, V: Value> BTree<K, V> {
    /// Every page of the tree.
    pub fn pages(&self, p: &mut dyn Pager) -> Result<Vec<PageNo>> {
        let Some(root) = self.root else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        let mut stack = vec![root];
        while let Some(pno) = stack.pop() {
            out.push(pno);
            if let Node::Internal { first, entries } = self.load(p, pno)? {
                stack.push(first);
                stack.extend(entries.iter().map(|(_, c)| *c));
            }
        }
        Ok(out)
    }

    /// Pages `bulk_load` writes for `n` entries whose values are all
    /// `value_len` bytes. Exact: it mirrors the packing in `bulk_load`.
    #[must_use]
    pub fn bulk_load_pages(n: usize, value_len: usize) -> u64 {
        Self::bulk_load_pages_with(K::LEN, n, value_len)
    }

    /// [`bulk_load_pages`](Self::bulk_load_pages) for `key_len`-byte keys.
    #[must_use]
    pub fn bulk_load_pages_with(key_len: usize, n: usize, value_len: usize) -> u64 {
        if n == 0 {
            return 0;
        }
        let per_leaf =
            (node::BULK_FILL - node::HEADER_LEN) / (key_len + node::SLOT_EXTRA + value_len);
        let mut level = n.div_ceil(per_leaf.max(1));
        let mut pages = level;
        let per_node =
            ((node::BULK_FILL - node::HEADER_LEN - 8) / (key_len + node::ENTRY_EXTRA)).max(1) + 1;
        while level > 1 {
            level = level.div_ceil(per_node);
            pages += level;
        }
        pages as u64
    }

    /// Number of entries, by walking every leaf.
    pub fn len(&self, p: &mut dyn Pager) -> Result<usize> {
        let Some(root) = self.root else {
            return Ok(0);
        };
        let mut n = 0;
        let mut stack = vec![root];
        while let Some(pno) = stack.pop() {
            match self.load(p, pno)? {
                Node::Leaf(entries) => n += entries.len(),
                Node::Internal { first, entries } => {
                    stack.push(first);
                    stack.extend(entries.iter().map(|(_, c)| *c));
                }
            }
        }
        Ok(n)
    }

    /// Read a node's page, checking page type and owner.
    fn page(&self, p: &mut dyn Pager, pno: PageNo) -> Result<Page> {
        let page = p.read(pno)?;
        let t = page.verify_type(pno, PageType::BTreeNode)?;
        if t.owner != self.owner {
            return Err(StoreError::Corrupt {
                page: pno,
                what: format!("node owned by {} in a tree of {}", t.owner, self.owner),
            });
        }
        Ok(page)
    }

    /// Read and decode a node, checking page type and owner.
    fn load(&self, p: &mut dyn Pager, pno: PageNo) -> Result<Node<K>> {
        let page = self.page(p, pno)?;
        Node::decode(page.body(), pno, self.key_len)
    }

    /// Write a node to a fresh page.
    fn store(&self, p: &mut dyn Pager, node: &Node<K>) -> Result<PageNo> {
        let pno = p.alloc()?;
        let mut page = Page::zeroed();
        node.encode(page.body_mut(), self.key_len);
        p.write(pno, &mut page, PageType::BTreeNode, self.owner)?;
        Ok(pno)
    }

    /// Write `node` over `old` when this transaction allocated `old`
    /// (nothing else can see it yet), else to a fresh page, freeing `old`.
    fn rewrite(&self, p: &mut dyn Pager, old: PageNo, node: &Node<K>) -> Result<PageNo> {
        if p.owns(old) {
            let mut page = Page::zeroed();
            node.encode(page.body_mut(), self.key_len);
            p.write(old, &mut page, PageType::BTreeNode, self.owner)?;
            return Ok(old);
        }
        p.free(old)?;
        self.store(p, node)
    }

    /// Write `node` in place of `old`, splitting first if it does not fit.
    fn replace(&self, p: &mut dyn Pager, old: PageNo, mut node: Node<K>) -> Result<Put<K>> {
        if node.fits(self.key_len) {
            return Ok(Put::One(self.rewrite(p, old, &node)?));
        }
        let (up, right) = node.split(self.key_len);
        let sep = match up {
            Some(k) => k,
            None => right.first_key().expect("right half non-empty").clone(),
        };
        let left = self.rewrite(p, old, &node)?;
        let right = self.store(p, &right)?;
        Ok(Put::Split(left, sep, right))
    }

    fn insert_in(
        &self,
        p: &mut dyn Pager,
        pno: PageNo,
        key: K,
        bytes: Vec<u8>,
    ) -> Result<(Put<K>, Option<Vec<u8>>)> {
        match self.load(p, pno)? {
            Node::Leaf(mut entries) => {
                let old = match entries.binary_search_by(|(k, _)| k.cmp(&key)) {
                    Ok(i) => Some(std::mem::replace(&mut entries[i].1, bytes)),
                    Err(i) => {
                        entries.insert(i, (key, bytes));
                        None
                    }
                };
                Ok((self.replace(p, pno, Node::Leaf(entries))?, old))
            }
            Node::Internal {
                mut first,
                mut entries,
            } => {
                let i = child_index(&entries, &key);
                let child = if i == 0 { first } else { entries[i - 1].1 };
                let (put, old) = self.insert_in(p, child, key, bytes)?;
                match put {
                    Put::One(c) => set_child(&mut first, &mut entries, i, c),
                    Put::Split(l, sep, r) => {
                        set_child(&mut first, &mut entries, i, l);
                        entries.insert(i, (sep, r));
                    }
                }
                Ok((
                    self.replace(p, pno, Node::Internal { first, entries })?,
                    old,
                ))
            }
        }
    }

    fn delete_in(&self, p: &mut dyn Pager, pno: PageNo, key: &K) -> Result<Took> {
        match self.load(p, pno)? {
            Node::Leaf(mut entries) => {
                let Ok(i) = entries.binary_search_by(|(k, _)| k.cmp(key)) else {
                    return Ok(Took {
                        page: pno,
                        old: None,
                        underfull: false,
                    });
                };
                let (_, old) = entries.remove(i);
                let node = Node::Leaf(entries);
                let underfull = node.underfull(self.key_len);
                Ok(Took {
                    page: self.rewrite(p, pno, &node)?,
                    old: Some(old),
                    underfull,
                })
            }
            Node::Internal {
                mut first,
                mut entries,
            } => {
                let i = child_index(&entries, key);
                let child = if i == 0 { first } else { entries[i - 1].1 };
                let took = self.delete_in(p, child, key)?;
                if took.old.is_none() {
                    return Ok(Took {
                        page: pno,
                        old: None,
                        underfull: false,
                    });
                }
                set_child(&mut first, &mut entries, i, took.page);
                if took.underfull && !entries.is_empty() {
                    self.rebalance(p, &mut first, &mut entries, i)?;
                }
                let node = Node::Internal { first, entries };
                let underfull = node.underfull(self.key_len);
                Ok(Took {
                    page: self.rewrite(p, pno, &node)?,
                    old: took.old,
                    underfull,
                })
            }
        }
    }

    /// Merge child `i` with a sibling, or rebalance the pair, rewriting
    /// both children and fixing the separator in `entries`.
    fn rebalance(
        &self,
        p: &mut dyn Pager,
        first: &mut PageNo,
        entries: &mut Vec<(K, PageNo)>,
        i: usize,
    ) -> Result<()> {
        // Pair (left child `pair`, right child `pair + 1`); the
        // separator between them is `entries[pair]`.
        let pair = if i == 0 { 0 } else { i - 1 };
        let left_pno = if pair == 0 {
            *first
        } else {
            entries[pair - 1].1
        };
        let right_pno = entries[pair].1;
        let left = self.load(p, left_pno)?;
        let right = self.load(p, right_pno)?;
        let merged = match (left, right) {
            (Node::Leaf(mut a), Node::Leaf(b)) => {
                a.extend(b);
                Node::Leaf(a)
            }
            (
                Node::Internal {
                    first: lf,
                    entries: mut le,
                },
                Node::Internal {
                    first: rf,
                    entries: re,
                },
            ) => {
                le.push((entries[pair].0.clone(), rf));
                le.extend(re);
                Node::Internal {
                    first: lf,
                    entries: le,
                }
            }
            _ => {
                return Err(StoreError::Corrupt {
                    page: right_pno,
                    what: "sibling nodes at different levels".into(),
                });
            }
        };
        if merged.fits(self.key_len) {
            p.free(right_pno)?;
            let pno = self.rewrite(p, left_pno, &merged)?;
            entries.remove(pair);
            set_child(first, entries, pair, pno);
        } else {
            let mut left = merged;
            let (up, right) = left.split(self.key_len);
            let sep = match up {
                Some(k) => k,
                None => right.first_key().expect("non-empty").clone(),
            };
            let lp = self.rewrite(p, left_pno, &left)?;
            let rp = self.rewrite(p, right_pno, &right)?;
            set_child(first, entries, pair, lp);
            entries[pair] = (sep, rp);
        }
        Ok(())
    }
}

/// Index of the child holding `key`: 0 for `first`, else 1 + the index
/// of the last separator `<= key`.
fn child_index<K: FixedKey>(entries: &[(K, PageNo)], key: &K) -> usize {
    entries.partition_point(|(k, _)| k <= key)
}

fn set_child<K>(first: &mut PageNo, entries: &mut [(K, PageNo)], i: usize, pno: PageNo) {
    if i == 0 {
        *first = pno;
    } else {
        entries[i - 1].1 = pno;
    }
}

fn clone_bound<K: Clone>(b: Bound<&K>) -> Bound<K> {
    match b {
        Bound::Included(k) => Bound::Included(k.clone()),
        Bound::Excluded(k) => Bound::Excluded(k.clone()),
        Bound::Unbounded => Bound::Unbounded,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemPager;

    #[test]
    fn insert_get_delete_small() {
        let mut p = MemPager::new(64);
        let mut t: BTree<u64, u64> = BTree::new(ObjectId::NONE);
        assert_eq!(t.get(&mut p, &1).unwrap(), None);
        assert_eq!(t.insert(&mut p, 1, &10).unwrap(), None);
        assert_eq!(t.insert(&mut p, 1, &11).unwrap(), Some(10));
        assert_eq!(t.get(&mut p, &1).unwrap(), Some(11));
        assert_eq!(t.delete(&mut p, &2).unwrap(), None);
        assert_eq!(t.delete(&mut p, &1).unwrap(), Some(11));
        assert!(t.is_empty());
        p.commit().unwrap();
        assert_eq!(p.in_use(), 0, "every page of an empty tree is freed");
    }

    #[test]
    fn grows_and_shrinks_through_splits() {
        let mut p = MemPager::new(4096);
        let mut t: BTree<u64, Vec<u8>> = BTree::new(ObjectId::NONE);
        for i in 0..3000u64 {
            t.insert(
                &mut p,
                i * 7 % 3001,
                &vec![u8::try_from(i % 251).unwrap(); 40],
            )
            .unwrap();
            p.commit().unwrap();
        }
        assert_eq!(t.len(&mut p).unwrap(), 3000);
        let all: Vec<_> = t.entries(&mut p).unwrap().map(Result::unwrap).collect();
        assert_eq!(all.len(), 3000);
        assert!(all.windows(2).all(|w| w[0].0 < w[1].0));
        let some: Vec<u64> = t
            .range(&mut p, 100..=110)
            .unwrap()
            .map(|r| r.unwrap().0)
            .collect();
        assert_eq!(some, (100..=110).collect::<Vec<_>>());
        for i in 0..3000u64 {
            assert!(t.delete(&mut p, &(i * 7 % 3001)).unwrap().is_some());
            p.commit().unwrap();
        }
        assert!(t.is_empty());
        assert_eq!(p.in_use(), 0);
    }

    #[test]
    fn bulk_load_matches_inserts() {
        let mut p = MemPager::new(2048);
        let items: Vec<(u64, u64)> = (0..5000).map(|i| (i * 3, i)).collect();
        let t: BTree<u64, u64> = BTree::bulk_load(&mut p, ObjectId::NONE, items.clone()).unwrap();
        let got: Vec<_> = t.entries(&mut p).unwrap().map(Result::unwrap).collect();
        assert_eq!(got, items);
        assert_eq!(t.get(&mut p, &(4999 * 3)).unwrap(), Some(4999));
        assert_eq!(t.get(&mut p, &1).unwrap(), None);
        assert!(matches!(
            BTree::<u64, u64>::bulk_load(&mut p, ObjectId::NONE, vec![(2, 0), (1, 0)]),
            Err(StoreError::Unsorted)
        ));
        let empty: BTree<u64, u64> = BTree::bulk_load(&mut p, ObjectId::NONE, Vec::new()).unwrap();
        assert!(empty.is_empty());
        let mut t = t;
        t.free_all(&mut p).unwrap();
        p.commit().unwrap();
        assert_eq!(p.in_use(), 0);
    }

    #[test]
    fn floor_finds_predecessors_across_leaves() {
        let mut p = MemPager::new(4096);
        let mut t: BTree<u64, u64> = BTree::new(ObjectId::NONE);
        assert_eq!(t.floor(&mut p, &5).unwrap(), None);
        for i in 0..2000u64 {
            t.insert(&mut p, i * 10, &i).unwrap();
        }
        assert_eq!(t.floor(&mut p, &5).unwrap(), Some((0, 0)));
        assert_eq!(t.floor(&mut p, &0).unwrap(), Some((0, 0)));
        assert_eq!(t.floor(&mut p, &19_995).unwrap(), Some((19_990, 1999)));
        assert_eq!(t.floor(&mut p, &u64::MAX).unwrap(), Some((19_990, 1999)));
        // Every key, exact and one past, including leaf boundaries.
        for i in 0..2000u64 {
            assert_eq!(t.floor(&mut p, &(i * 10)).unwrap(), Some((i * 10, i)));
            assert_eq!(t.floor(&mut p, &(i * 10 + 1)).unwrap(), Some((i * 10, i)));
        }
        t.delete(&mut p, &0).unwrap();
        assert_eq!(t.floor(&mut p, &5).unwrap(), None);
    }

    #[test]
    fn bulk_load_page_estimate_is_exact() {
        for n in [0usize, 1, 100, 200, 201, 5000, 50_000] {
            let mut p = MemPager::new(1 << 16);
            let items = (0..n as u64).map(|i| (i, i));
            let t: BTree<u64, u64> = BTree::bulk_load(&mut p, ObjectId::NONE, items).unwrap();
            let got = t.pages(&mut p).unwrap().len() as u64;
            assert_eq!(got, BTree::<u64, u64>::bulk_load_pages(n, 8), "{n} entries");
        }
    }

    #[test]
    fn value_too_long() {
        let mut p = MemPager::new(16);
        let mut t: BTree<u64, Vec<u8>> = BTree::new(ObjectId::NONE);
        assert!(matches!(
            t.insert(&mut p, 1, &vec![0; MAX_VALUE_LEN + 1]),
            Err(StoreError::ValueTooLong { .. })
        ));
    }
}
