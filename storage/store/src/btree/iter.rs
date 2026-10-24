// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Ordered range scan. The tree has no sibling pointers, so the
//! iterator keeps the path from the root as a stack of node pages and
//! the next position in each, reading keys in place.

use std::marker::PhantomData;
use std::ops::Bound;

use crate::Result;
use crate::page::Page;
use crate::page::pager::{PageNo, Pager};

use super::codec::{FixedKey, Value};
use super::node::NodeView;

/// Entries of a tree in key order, within bounds.
pub struct Iter<'p, K, V> {
    pager: &'p mut dyn Pager,
    key_len: usize,
    /// Path from the root; the index is the next child or slot to visit.
    stack: Vec<(Page, PageNo, usize)>,
    end: Bound<K>,
    done: bool,
    _marker: PhantomData<V>,
}

impl<'p, K: FixedKey, V: Value> Iter<'p, K, V> {
    /// Position at the first entry within `start`.
    pub(super) fn new(
        pager: &'p mut dyn Pager,
        root: Option<PageNo>,
        key_len: usize,
        start: &Bound<K>,
        end: Bound<K>,
    ) -> Result<Self> {
        let mut it = Self {
            pager,
            key_len,
            stack: Vec::new(),
            end,
            done: root.is_none(),
            _marker: PhantomData,
        };
        let Some(mut pno) = root else {
            return Ok(it);
        };
        loop {
            let page = it.load(pno)?;
            let node = NodeView::open(page.body(), pno, key_len)?;
            if node.is_leaf() {
                let at = match start {
                    Bound::Unbounded => 0,
                    Bound::Included(k) => node.below(k),
                    Bound::Excluded(k) => node.at_most(k),
                };
                it.stack.push((page, pno, at));
                return Ok(it);
            }
            let i = match start {
                Bound::Unbounded => 0,
                Bound::Included(k) | Bound::Excluded(k) => node.at_most(k),
            };
            let child = node.child(i);
            it.stack.push((page, pno, i + 1));
            pno = child;
        }
    }

    fn load(&mut self, pno: PageNo) -> Result<Page> {
        let page = self.pager.read(pno)?;
        page.verify_type(pno, crate::page::PageType::BTreeNode)?;
        Ok(page)
    }

    fn step(&mut self) -> Result<Option<(K, V)>> {
        loop {
            let Some((page, pno, at)) = self.stack.last_mut() else {
                self.done = true;
                return Ok(None);
            };
            let node = NodeView::open(page.body(), *pno, self.key_len)?;
            if node.is_leaf() {
                if *at < node.count() {
                    let k: K = node.key(*at);
                    let v = node.value(*at)?;
                    *at += 1;
                    if past_end(&self.end, &k) {
                        self.done = true;
                        return Ok(None);
                    }
                    let v = V::from_bytes(v)?;
                    return Ok(Some((k, v)));
                }
                self.stack.pop();
                continue;
            }
            if *at > node.count() {
                self.stack.pop();
                continue;
            }
            let child = node.child(*at);
            *at += 1;
            let page = self.load(child)?;
            self.stack.push((page, child, 0));
        }
    }
}

fn past_end<K: Ord>(end: &Bound<K>, k: &K) -> bool {
    match end {
        Bound::Unbounded => false,
        Bound::Included(e) => k > e,
        Bound::Excluded(e) => k >= e,
    }
}

impl<K: FixedKey, V: Value> Iterator for Iter<'_, K, V> {
    type Item = Result<(K, V)>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        match self.step() {
            Ok(Some(kv)) => Some(Ok(kv)),
            Ok(None) => None,
            Err(e) => {
                self.done = true;
                Some(Err(e))
            }
        }
    }
}
