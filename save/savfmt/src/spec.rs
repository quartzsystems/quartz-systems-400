// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The stream's layouts as data, so a test can compare them with the
//! tables in `docs/SAVE_FORMAT.md`.

/// One field: name, byte offset, byte size; `None` for a variable one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    /// Field name as in the spec table.
    pub name: &'static str,
    /// Offset in bytes.
    pub offset: Option<usize>,
    /// Size in bytes.
    pub size: Option<usize>,
}

/// A structure's layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    /// Name as in the spec heading.
    pub name: &'static str,
    /// Total size, `None` when variable.
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

/// A variable field.
pub const fn var(name: &'static str, offset: usize) -> Field {
    Field {
        name,
        offset: Some(offset),
        size: None,
    }
}

/// Every layout the code defines.
#[must_use]
pub fn layouts() -> Vec<&'static Layout> {
    vec![
        &crate::header::LAYOUT,
        &crate::record::LAYOUT,
        &crate::object::DESCRIPTOR_LAYOUT,
        &crate::object::AUTH_HEADER_LAYOUT,
        &crate::object::AUTH_ENTRY_LAYOUT,
        &crate::object::CHUNK_LAYOUT,
        &crate::object::END_LAYOUT,
        &crate::object::SIGNATURE_LAYOUT,
        &crate::object::TREE_LAYOUT,
        &crate::object::REF_LAYOUT,
        &crate::object::INDEX_LAYOUT,
        &crate::object::TRAILER_LAYOUT,
    ]
}
