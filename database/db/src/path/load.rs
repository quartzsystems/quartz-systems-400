// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A path built from the records in steps: the entries gathered and
//! sorted in runs as they come, then merged into the tree by a
//! [`store::btree::Loader`], its root published and a pause taken
//! every [`LOAD_EVERY`] leaves. Each step is short, so the
//! caller's pauses let other jobs in all through a large build.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use store::PageNo;
use store::btree::{DynKey, Loader};
use store::txn::Txn;

use super::{Path, PathKey};
use crate::error::Result;

/// Entries sorted at once: one short step.
pub const RUN: usize = 16_384;

/// Leaves written between two pauses of the load.
pub const LOAD_EVERY: usize = 32;

/// Entries gathered for a build.
pub struct Gather {
    path: Path,
    run: Vec<(DynKey, bool)>,
    runs: Vec<Vec<(DynKey, bool)>>,
}

/// How a build ended.
#[derive(Debug, PartialEq, Eq)]
pub enum Built {
    /// The tree and its entries.
    Tree {
        /// Its root.
        root: Option<PageNo>,
        /// Entries in it.
        entries: u64,
    },
    /// Two entries that count against `UNIQUE` have this key; what was
    /// loaded is published, to be freed.
    Repeated(Vec<u8>),
}

impl Path {
    /// An empty gathering for a build of this path.
    #[must_use]
    pub fn gather(&self) -> Gather {
        Gather {
            path: self.clone(),
            run: Vec::new(),
            runs: Vec::new(),
        }
    }
}

impl Gather {
    /// One entry; `once` when its key counts against `UNIQUE`. A full
    /// run is sorted here.
    pub fn push(&mut self, k: &PathKey, once: bool) {
        self.run.push((self.path.encode(k), once));
        if self.run.len() >= RUN {
            self.close_run();
        }
    }

    fn close_run(&mut self) {
        let mut run = std::mem::take(&mut self.run);
        run.sort_unstable();
        self.runs.push(run);
    }

    /// Merge the runs into the tree. `publish` stores a root at each
    /// step (and on a repeated key) before `pause` may commit; a load
    /// cut short leaves a published tree to free.
    pub fn load(
        mut self,
        txn: &mut Txn<'_>,
        publish: &mut dyn FnMut(&mut Txn<'_>, Option<PageNo>) -> Result<()>,
        pause: crate::Pause<'_>,
    ) -> Result<Built> {
        if !self.run.is_empty() {
            self.close_run();
        }
        let key_len = self.path.key_len;
        let mut runs: Vec<_> = self.runs.into_iter().map(Vec::into_iter).collect();
        let mut heap = BinaryHeap::with_capacity(runs.len());
        for (i, r) in runs.iter_mut().enumerate() {
            if let Some((k, once)) = r.next() {
                heap.push(Reverse((k, once, i)));
            }
        }
        let mut loader: Loader<DynKey, ()> = Loader::new(self.path.owner, self.path.tree_key_len());
        let mut prev: Option<DynKey> = None;
        let mut last_once: Option<Vec<u8>> = None;
        let mut entries = 0u64;
        while let Some(Reverse((k, once, i))) = heap.pop() {
            if let Some((next, o)) = runs[i].next() {
                heap.push(Reverse((next, o, i)));
            }
            if prev.as_ref() == Some(&k) {
                continue;
            }
            if once {
                let bytes = &k.0[..key_len];
                if last_once.as_deref() == Some(bytes) {
                    let root = loader.snapshot(txn)?;
                    publish(txn, root)?;
                    return Ok(Built::Repeated(bytes.to_vec()));
                }
                last_once = Some(bytes.to_vec());
            }
            loader.push(txn, k.clone(), &())?;
            prev = Some(k);
            entries += 1;
            if loader.leaves() >= LOAD_EVERY {
                let root = loader.snapshot(txn)?;
                publish(txn, root)?;
                pause(txn)?;
            }
        }
        let tree = loader.finish(txn)?;
        Ok(Built::Tree {
            root: tree.root(),
            entries,
        })
    }
}
