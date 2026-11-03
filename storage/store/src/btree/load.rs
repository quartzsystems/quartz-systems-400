// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A bulk load in steps. Entries go in ascending; full leaves and the
//! internal nodes above them are written as they fill. Between steps
//! [`Loader::snapshot`] makes the entries so far a whole tree (the open
//! leaf written, the right edge's nodes written to pages of their own),
//! so the caller can publish its root and commit part of a long load:
//! a load cut short leaves a tree whose pages can all be found and
//! freed. Without snapshots the tree is the one `bulk_load` makes.

use super::node::{self, MAX_VALUE_LEN, Node};
use super::{BTree, FixedKey, Value};
use crate::obj::ObjectId;
use crate::page::pager::{PageNo, Pager};
use crate::{Result, StoreError};

/// A tree being loaded.
pub struct Loader<K, V> {
    tree: BTree<K, V>,
    /// The open leaf and its bytes used.
    leaf: Vec<(K, Vec<u8>)>,
    used: usize,
    prev: Option<K>,
    /// Per level, the nodes written that have no parent yet (fewer than
    /// a node holds).
    levels: Vec<Vec<(K, PageNo)>>,
    /// Children per internal node.
    per_node: usize,
    /// The right edge the last snapshot wrote, freed at the next.
    edge: Vec<PageNo>,
    /// Leaves written since the last snapshot.
    leaves: usize,
}

impl<K: FixedKey, V: Value> Loader<K, V> {
    /// An empty load of a tree owned by `owner` with `key_len`-byte keys.
    #[must_use]
    pub fn new(owner: ObjectId, key_len: usize) -> Self {
        let per_node =
            ((node::BULK_FILL - node::HEADER_LEN - 8) / (key_len + node::ENTRY_EXTRA)).max(1) + 1;
        Self {
            tree: BTree::with_key_len(None, owner, key_len),
            leaf: Vec::new(),
            used: node::HEADER_LEN,
            prev: None,
            levels: Vec::new(),
            per_node,
            edge: Vec::new(),
            leaves: 0,
        }
    }

    /// Leaves written since the last snapshot.
    #[must_use]
    pub fn leaves(&self) -> usize {
        self.leaves
    }

    /// Add the next entry; keys strictly ascending.
    pub fn push(&mut self, p: &mut dyn Pager, k: K, v: &V) -> Result<()> {
        if self.prev.as_ref().is_some_and(|q| q >= &k) {
            return Err(StoreError::Unsorted);
        }
        self.prev = Some(k.clone());
        let bytes = v.to_bytes();
        if bytes.len() > MAX_VALUE_LEN {
            return Err(StoreError::ValueTooLong { len: bytes.len() });
        }
        let cost = self.tree.key_len + node::SLOT_EXTRA + bytes.len();
        if !self.leaf.is_empty() && self.used + cost > node::BULK_FILL {
            self.close_leaf(p)?;
        }
        self.used += cost;
        self.leaf.push((k, bytes));
        Ok(())
    }

    fn close_leaf(&mut self, p: &mut dyn Pager) -> Result<()> {
        let node = Node::Leaf(std::mem::take(&mut self.leaf));
        let first = node.first_key().expect("non-empty").clone();
        let pno = self.tree.store(p, &node)?;
        self.used = node::HEADER_LEN;
        self.leaves += 1;
        self.add(p, 0, (first, pno))
    }

    /// A node written at `level`; a full group gets its parent.
    fn add(&mut self, p: &mut dyn Pager, level: usize, entry: (K, PageNo)) -> Result<()> {
        if self.levels.len() <= level {
            self.levels.push(Vec::new());
        }
        self.levels[level].push(entry);
        if self.levels[level].len() < self.per_node {
            return Ok(());
        }
        let group = std::mem::take(&mut self.levels[level]);
        let pno = self.tree.store(p, &internal(&group))?;
        self.add(p, level + 1, (group[0].0.clone(), pno))
    }

    /// The right edge over the nodes without parents: the root, and the
    /// pages written for it.
    fn edge(&self, p: &mut dyn Pager) -> Result<(Option<PageNo>, Vec<PageNo>)> {
        let mut written = Vec::new();
        let mut carry: Option<(K, PageNo)> = None;
        for i in 0..self.levels.len() {
            let mut group = self.levels[i].clone();
            group.extend(carry.take());
            if group.is_empty() {
                continue;
            }
            let higher = self.levels[i + 1..].iter().any(|l| !l.is_empty());
            if group.len() == 1 && !higher {
                return Ok((Some(group[0].1), written));
            }
            let pno = self.tree.store(p, &internal(&group))?;
            written.push(pno);
            carry = Some((group[0].0.clone(), pno));
        }
        Ok((carry.map(|(_, pno)| pno), written))
    }

    /// Make the entries so far a whole tree: its root, to publish before
    /// a part is committed. The previous snapshot's edge is freed.
    pub fn snapshot(&mut self, p: &mut dyn Pager) -> Result<Option<PageNo>> {
        for pno in std::mem::take(&mut self.edge) {
            p.free(pno)?;
        }
        if !self.leaf.is_empty() {
            self.close_leaf(p)?;
        }
        let (root, written) = self.edge(p)?;
        self.edge = written;
        self.leaves = 0;
        Ok(root)
    }

    /// The finished tree.
    pub fn finish(mut self, p: &mut dyn Pager) -> Result<BTree<K, V>> {
        for pno in std::mem::take(&mut self.edge) {
            p.free(pno)?;
        }
        if !self.leaf.is_empty() {
            self.close_leaf(p)?;
        }
        let (root, _) = self.edge(p)?;
        self.tree.root = root;
        Ok(self.tree)
    }
}

/// An internal node over `group`: its first child, then the rest keyed.
fn internal<K: FixedKey>(group: &[(K, PageNo)]) -> Node<K> {
    Node::Internal {
        first: group[0].1,
        entries: group[1..].to_vec(),
    }
}
