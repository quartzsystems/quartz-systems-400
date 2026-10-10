// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The free-space tree: `(unit, lba) -> len`, runs never touching,
//! bulk-loaded at every checkpoint.

use crate::Result;
use crate::btree::BTree;
use crate::page::pager::Pager;
use crate::spec::{Layout, field};

use super::Extent;

/// The tree type.
pub type FreeTree = BTree<(u16, u64), u64>;

/// Key layout for the spec test.
pub static KEY_LAYOUT: Layout = Layout {
    name: "FreeKey",
    size: Some(10),
    fields: &[field("unit", 0, 2), field("lba", 2, 8)],
};

/// Value layout for the spec test.
pub static LEN_LAYOUT: Layout = Layout {
    name: "FreeLen",
    size: Some(8),
    fields: &[field("len", 0, 8)],
};

/// Every free run, ascending.
pub fn scan(tree: &FreeTree, p: &mut dyn Pager) -> Result<Vec<Extent>> {
    tree.entries(p)?
        .map(|r| r.map(|((unit, lba), len)| Extent { unit, lba, len }))
        .collect()
}
