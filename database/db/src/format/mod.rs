// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Record formats as the file keeps them: the DDS model plus what the
//! database adds (level ID, null map, key encoding sizes, the logical
//! definition), and their section encoding. See `docs/DB_FORMAT.md` §3,
//! §5 and §6.

pub mod keys;
mod level_id;
pub mod sections;

pub use level_id::fmt_level_id;

use dds::{DataType, DupOrder, NullKeys, RecordFormat};

use crate::error::{DbError, Result};
use crate::lf::LogicalDef;

/// Longest encoded key: the store's key less the format tag and RRN.
pub const MAX_KEY_LEN: usize = 1014;

/// A record format with the database's view of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormatDef {
    /// The fields and keys, offsets laid out.
    pub format: RecordFormat,
    /// The 13-character level identifier.
    pub level_id: String,
    /// Bytes of null map per record, 0 when no field is null-capable.
    pub null_map_len: u16,
    /// `UNIQUE`.
    pub unique: Option<NullKeys>,
    /// `FIFO`, `LIFO` or `FCFO`.
    pub dup_order: Option<DupOrder>,
    /// Bytes of the encoded key, 0 for arrival sequence.
    pub key_len: u16,
    /// Set for a format of a logical file.
    pub logical: Option<LogicalDef>,
}

impl FormatDef {
    /// From a compiled format and its file's access path attributes.
    pub fn new(
        format: RecordFormat,
        unique: Option<NullKeys>,
        dup_order: Option<DupOrder>,
    ) -> Result<Self> {
        let nullable = format.fields.iter().filter(|f| f.allow_null).count();
        let null_map_len = u16::try_from(nullable.div_ceil(8)).expect("at most 1000");
        let key_len: usize = format
            .keys
            .iter()
            .map(|k| keys::part_len(&format.fields[k.field], k.transform))
            .sum();
        if key_len > MAX_KEY_LEN {
            return Err(DbError::Definition {
                what: format!(
                    "key of format {} is {key_len} bytes; at most {MAX_KEY_LEN}",
                    format.name
                ),
            });
        }
        if format.record_len > 32766 {
            return Err(DbError::Definition {
                what: format!("record length {} exceeds 32766", format.record_len),
            });
        }
        Ok(Self {
            level_id: fmt_level_id(&format.fields),
            null_map_len,
            unique,
            dup_order,
            key_len: u16::try_from(key_len).expect("checked"),
            format,
            logical: None,
        })
    }

    /// Whether the format has key fields.
    #[must_use]
    pub fn is_keyed(&self) -> bool {
        !self.format.keys.is_empty()
    }

    /// Bytes per record slot: status, record, null map.
    #[must_use]
    pub fn slot_len(&self) -> u32 {
        1 + self.format.record_len + u32::from(self.null_map_len)
    }

    /// The null-map bit of field `index`, when it is null-capable.
    #[must_use]
    pub fn null_index(&self, index: usize) -> Option<u16> {
        let f = self.format.fields.get(index)?;
        if !f.allow_null {
            return None;
        }
        let before = self.format.fields[..index]
            .iter()
            .filter(|f| f.allow_null)
            .count();
        Some(u16::try_from(before).expect("at most 8000"))
    }
}

/// The DDS letter of a type, as the file stores it.
#[must_use]
pub fn type_letter(t: DataType) -> u8 {
    t.letter() as u8
}
