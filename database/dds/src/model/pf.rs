// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a physical file's DDS defines.

use super::{FileRef, RecordFormat};

/// `UNIQUE`: how null keys count.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NullKeys {
    /// `*INCNULL`: nulls compare equal to each other.
    #[default]
    Include,
    /// `*EXCNULL`: a null key never duplicates another.
    Exclude,
}

/// Order of records with equal keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DupOrder {
    /// `FIFO`.
    Fifo,
    /// `LIFO`.
    Lifo,
    /// `FCFO`.
    Fcfo,
}

impl DupOrder {
    /// The keyword.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Fifo => "FIFO",
            Self::Lifo => "LIFO",
            Self::Fcfo => "FCFO",
        }
    }
}

/// A compiled physical file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PfDefinition {
    /// The one record format.
    pub format: RecordFormat,
    /// `UNIQUE`.
    pub unique: Option<NullKeys>,
    /// `FIFO`, `LIFO` or `FCFO`.
    pub dup_order: Option<DupOrder>,
    /// `REF`: the default file for `R` fields.
    pub reference: Option<FileRef>,
    /// `REFACCPTH`: the file the keys were copied from.
    pub refaccpth: Option<FileRef>,
    /// `ALTSEQ`: the table.
    pub altseq: Option<FileRef>,
    /// File-level `CCSID`.
    pub ccsid: Option<u16>,
    /// File-level `TEXT`.
    pub text: Option<String>,
}

impl PfDefinition {
    /// Whether the file has an access path.
    #[must_use]
    pub fn is_keyed(&self) -> bool {
        !self.format.keys.is_empty()
    }
}
