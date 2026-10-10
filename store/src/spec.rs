// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Byte layouts of the on-disk structures, as data, so a test can compare
//! them with the tables in `docs/STORE_FORMAT.md`.

/// One field: name, byte offset, byte size. A size of `None` is a
/// key-length-sized field; an offset of `None` is "after the key".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    /// Field name as in the spec table.
    pub name: &'static str,
    /// Offset in bytes, or `None` for `key_len`.
    pub offset: Option<usize>,
    /// Size in bytes, or `None` for `key_len`.
    pub size: Option<usize>,
}

/// A structure's layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    /// Name as in the spec heading.
    pub name: &'static str,
    /// Total size in bytes, or `None` when variable.
    pub size: Option<usize>,
    /// Fields in offset order.
    pub fields: &'static [Field],
}

/// A fixed field.
pub const fn field(name: &'static str, offset: usize, size: usize) -> Field {
    Field {
        name,
        offset: Some(offset),
        size: Some(size),
    }
}

/// Every layout the code defines.
#[must_use]
pub fn layouts() -> Vec<&'static Layout> {
    vec![
        &crate::page::trailer::LAYOUT,
        &crate::obj::id::LAYOUT,
        &crate::btree::node::HEADER_LAYOUT,
        &crate::btree::node::INTERNAL_ENTRY_LAYOUT,
        &crate::btree::node::LEAF_SLOT_LAYOUT,
    ]
}
