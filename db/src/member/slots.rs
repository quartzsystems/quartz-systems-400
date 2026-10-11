// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Record slots in a member's byte space. See `docs/DB_FORMAT.md` §4.

use crate::spec::{Field, Layout};

/// Slot never written.
pub const STATUS_EMPTY: u8 = 0;
/// Slot holds a record.
pub const STATUS_ACTIVE: u8 = 1;
/// Slot held a record that was deleted.
pub const STATUS_DELETED: u8 = 2;

/// The spec layout.
pub static LAYOUT: Layout = Layout {
    name: "RecordSlot",
    size: None,
    fields: &[
        Field {
            name: "status",
            offset: Some(0),
            size: Some(1),
        },
        Field {
            name: "record",
            offset: Some(1),
            size: None,
        },
        Field {
            name: "null_map",
            offset: None,
            size: None,
        },
    ],
};

/// A record read from a slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slot {
    /// Relative record number.
    pub rrn: u64,
    /// The record bytes, `record_len` of them.
    pub record: Vec<u8>,
    /// The null map, `null_map_len` bytes (empty when the format has none).
    pub null_map: Vec<u8>,
    /// Whether the record is deleted.
    pub deleted: bool,
}

impl Slot {
    /// Whether null-capable field `null_index` is null.
    #[must_use]
    pub fn is_null(&self, null_index: u16) -> bool {
        let i = usize::from(null_index);
        self.null_map
            .get(i / 8)
            .is_some_and(|b| b & (1 << (i % 8)) != 0)
    }
}

/// Byte offset of slot `rrn` (1-based).
#[must_use]
pub fn offset(rrn: u64, slot_len: u32) -> u64 {
    (rrn - 1) * u64::from(slot_len)
}
