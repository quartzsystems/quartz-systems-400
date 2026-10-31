// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The payload layouts as data, so a test can compare them with the
//! tables in `docs/LICPGM.md`.

/// One field: name, offset, size. `None` is a variable offset or size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    /// Field name as in the spec table.
    pub name: &'static str,
    /// Offset in bytes, or `None` when it follows a variable field.
    pub offset: Option<usize>,
    /// Size in bytes, or `None` when variable.
    pub size: Option<usize>,
}

/// A structure's layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    /// Name as in the spec heading.
    pub name: &'static str,
    /// Total size, or `None` when variable.
    pub size: Option<usize>,
    /// Fields in offset order.
    pub fields: &'static [Field],
}

/// A fixed field.
#[must_use]
pub const fn field(name: &'static str, offset: usize, size: usize) -> Field {
    Field {
        name,
        offset: Some(offset),
        size: Some(size),
    }
}

/// A field after a variable one.
#[must_use]
pub const fn var(name: &'static str) -> Field {
    Field {
        name,
        offset: None,
        size: None,
    }
}

/// Every layout the code defines.
#[must_use]
pub fn layouts() -> Vec<&'static Layout> {
    use crate::payload::{prddfn, prdlod};
    vec![
        &prddfn::HEADER,
        &prddfn::OPTION,
        &prddfn::PREREQ,
        &prddfn::LANGUAGE,
        &prddfn::CCSID,
        &prddfn::LICENSE,
        &prdlod::HEADER,
        &prdlod::LIBRARY,
        &prdlod::EXIT,
        &prdlod::FOLDER,
        &prdlod::DIRECTORY,
        &prdlod::OBJECT,
        &prdlod::TEXT,
    ]
}
