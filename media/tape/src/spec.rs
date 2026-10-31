// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The byte layouts `docs/MEDIA.md` specifies, as the code uses them.
//! `tests/spec.rs` compares these with the document's tables.

use crate::label::layout as lab;
use crate::virt::format as vf;

/// One field of a layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    /// Field name, as in the spec table.
    pub name: &'static str,
    /// Byte offset.
    pub offset: usize,
    /// Size in bytes.
    pub size: usize,
}

/// A fixed-size structure.
#[derive(Debug, Clone, Copy)]
pub struct Layout {
    /// The spec table's name.
    pub name: &'static str,
    /// Total size in bytes.
    pub size: usize,
    /// The fields, in order.
    pub fields: &'static [Field],
}

/// Every layout the spec documents.
#[must_use]
pub fn layouts() -> Vec<&'static Layout> {
    vec![&vf::HEADER, &vf::FRAME, &lab::VOL1, &lab::HDR1, &lab::HDR2]
}
