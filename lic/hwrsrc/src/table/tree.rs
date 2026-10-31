// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The two trees over the table: packaging (what a resource sits in, by
//! sysfs ancestry: bridge, adapter, port or unit) and logical (what
//! drives it: a port under its network adapter, a unit under its
//! controller). Lists show a chosen set of resources each under the
//! nearest chosen one above it.

use super::{Kind, Resource, Table};

/// One row of a tree: the resource's index in the table and its depth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeRow {
    /// Index into `Table::resources`.
    pub index: usize,
    /// Levels below the top.
    pub depth: usize,
}

/// Rows from `i` down, each once.
fn walk(i: usize, depth: usize, kids: &[Vec<usize>], seen: &mut [bool], out: &mut Vec<TreeRow>) {
    if std::mem::replace(&mut seen[i], true) {
        return;
    }
    out.push(TreeRow { index: i, depth });
    for &k in &kids[i] {
        walk(k, depth + 1, kids, seen, out);
    }
}

/// Whether `parent` is a proper ancestor of `child` in sysfs.
#[must_use]
pub fn under(child: &str, parent: &str) -> bool {
    !parent.is_empty()
        && child.len() > parent.len()
        && child.starts_with(parent)
        && child.as_bytes()[parent.len()] == b'/'
}

impl Table {
    /// The parent of a resource in one tree, by name.
    fn up(&self, r: &Resource, logical: bool) -> Option<usize> {
        let name = if logical { &r.logical } else { &r.parent };
        if name.is_empty() {
            return None;
        }
        self.resources.iter().position(|p| &p.name == name)
    }

    /// The resources `pick` chooses, in tree order, each indented under
    /// the nearest chosen resource above it. With `bridges`, the bridges
    /// above a chosen resource are shown too (a bridge alone is not).
    #[must_use]
    pub fn tree(&self, pick: impl Fn(&Resource) -> bool, logical: bool, bridges: bool) -> Vec<TreeRow> {
        let n = self.resources.len();
        let mut chosen: Vec<bool> = self.resources.iter().map(&pick).collect();
        if bridges {
            for i in 0..n {
                if !chosen[i] || self.resources[i].kind == Kind::Bridge {
                    continue;
                }
                let mut at = i;
                for _ in 0..n {
                    let Some(p) = self.up(&self.resources[at], false) else { break };
                    if self.resources[p].kind == Kind::Bridge {
                        chosen[p] = true;
                    }
                    at = p;
                }
            }
            // A bridge chosen by `pick` with nothing chosen under it goes.
            for i in 0..n {
                if chosen[i] && self.resources[i].kind == Kind::Bridge {
                    let has = (0..n).any(|j| {
                        chosen[j]
                            && self.resources[j].kind != Kind::Bridge
                            && self.ancestor(j, i, false)
                    });
                    if !has && !pick(&self.resources[i]) {
                        chosen[i] = false;
                    }
                }
            }
        }
        // Each chosen resource's nearest chosen ancestor.
        let mut kids = vec![Vec::new(); n];
        let mut roots = Vec::new();
        for i in (0..n).filter(|&i| chosen[i]) {
            let mut at = i;
            let mut top = None;
            for _ in 0..n {
                // A unit or port without a logical parent falls back to
                // the packaging one.
                let Some(p) = self.up(&self.resources[at], logical) else { break };
                if chosen[p] {
                    top = Some(p);
                    break;
                }
                at = p;
            }
            match top {
                Some(p) if p != i => kids[p].push(i),
                _ => roots.push(i),
            }
        }
        let mut out = Vec::new();
        let mut seen = vec![false; n];
        for r in roots {
            walk(r, 0, &kids, &mut seen, &mut out);
        }
        out
    }

    /// Whether `a` is above `j` in a tree.
    fn ancestor(&self, j: usize, a: usize, logical: bool) -> bool {
        let mut at = j;
        for _ in 0..self.resources.len() {
            match self.up(&self.resources[at], logical) {
                Some(p) if p == a => return true,
                Some(p) => at = p,
                None => return false,
            }
        }
        false
    }

    /// The names from the top of a tree down to `name`, itself last.
    #[must_use]
    pub fn path_to(&self, name: &str, logical: bool) -> Vec<String> {
        let Some(mut at) = self.resources.iter().position(|r| r.name == name) else {
            return Vec::new();
        };
        let mut out = vec![self.resources[at].name.clone()];
        for _ in 0..self.resources.len() {
            match self.up(&self.resources[at], logical) {
                Some(p) => {
                    out.push(self.resources[p].name.clone());
                    at = p;
                }
                None => break,
            }
        }
        out.reverse();
        out
    }
}
