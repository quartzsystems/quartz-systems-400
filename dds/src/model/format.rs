// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A record format: fields in buffer order and the keys over them.

use qsname::Name;

use super::{FieldDef, FileRef, KeyField};

/// One record format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordFormat {
    /// The format name.
    pub name: Name,
    /// `TEXT`.
    pub text: Option<String>,
    /// Fields in buffer order, offsets filled in.
    pub fields: Vec<FieldDef>,
    /// Key fields in key order; empty for an arrival sequence file.
    pub keys: Vec<KeyField>,
    /// Bytes per record.
    pub record_len: u32,
    /// `FORMAT`: the file the fields were copied from.
    pub from: Option<FileRef>,
    /// Source line index of the `R` specification.
    pub line: usize,
}

impl RecordFormat {
    /// An empty format.
    #[must_use]
    pub fn new(name: Name) -> Self {
        Self {
            name,
            text: None,
            fields: Vec::new(),
            keys: Vec::new(),
            record_len: 0,
            from: None,
            line: 0,
        }
    }

    /// A field by name, with its index.
    #[must_use]
    pub fn field(&self, name: &Name) -> Option<(usize, &FieldDef)> {
        self.fields
            .iter()
            .enumerate()
            .find(|(_, f)| f.name == *name)
    }

    /// Bytes of the key, every key field's data length added up.
    #[must_use]
    pub fn key_len(&self) -> u32 {
        self.keys
            .iter()
            .map(|k| self.fields[k.field].data_len())
            .sum()
    }

    /// Assign offsets in field order and set the record length.
    pub fn lay_out(&mut self) {
        let mut at = 0;
        for f in &mut self.fields {
            f.offset = at;
            at += f.byte_len;
        }
        self.record_len = at;
    }
}
