// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a logical file's DDS defines: formats over physical files,
//! joins, select/omit.

use qsname::Name;

use super::{DupOrder, FileRef, NullKeys, RecordFormat, Validity};

/// Where a logical field's value comes from. File indexes point into
/// [`LfFormat::based_on`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldSource {
    /// A field of a based-on file, possibly renamed.
    Base {
        /// The file.
        file: usize,
        /// The field there.
        field: Name,
    },
    /// `CONCAT`: fields of one based-on file, in order.
    Concat {
        /// The file.
        file: usize,
        /// The fields.
        fields: Vec<Name>,
    },
    /// `SST`: part of a field.
    Substring {
        /// The file.
        file: usize,
        /// The field.
        field: Name,
        /// 1-based first position.
        start: u32,
        /// Positions taken.
        length: u32,
    },
}

/// One test of a select/omit condition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Condition {
    /// The field tested.
    pub field: Name,
    /// `VALUES`, `RANGE` or `COMP`.
    pub test: Validity,
    /// Source line index.
    pub line: usize,
}

/// An `S` or `O` specification with its AND conditions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectOmit {
    /// `S` when true, `O` when false.
    pub select: bool,
    /// `ALL`: every record not decided so far.
    pub all: bool,
    /// Conditions that must all hold.
    pub conditions: Vec<Condition>,
    /// Source line index of the `S` or `O` line.
    pub line: usize,
}

/// One `J` specification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Join {
    /// Index of the from-file in `based_on`.
    pub from: usize,
    /// Index of the to-file.
    pub to: usize,
    /// `JFLD` pairs: field of the from-file, field of the to-file.
    pub fields: Vec<(Name, Name)>,
    /// `JDUPSEQ`: a to-file field and whether it descends.
    pub dupseq: Option<(Name, bool)>,
    /// Source line index.
    pub line: usize,
}

/// One record format of a logical file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LfFormat {
    /// The fields and keys.
    pub format: RecordFormat,
    /// `PFILE` or `JFILE` files, in order.
    pub based_on: Vec<FileRef>,
    /// The record format used in each based-on file.
    pub base_formats: Vec<Name>,
    /// One source per field of `format`.
    pub sources: Vec<FieldSource>,
    /// Select/omit groups in order.
    pub select_omit: Vec<SelectOmit>,
    /// `K *NONE`.
    pub arrival: bool,
    /// `JFILE` rather than `PFILE`.
    pub join: bool,
    /// The `J` specifications of a join format.
    pub joins: Vec<Join>,
}

/// A compiled logical file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LfDefinition {
    /// The formats, in source order.
    pub formats: Vec<LfFormat>,
    /// `UNIQUE`.
    pub unique: Option<NullKeys>,
    /// `FIFO`, `LIFO` or `FCFO`.
    pub dup_order: Option<DupOrder>,
    /// `REFACCPTH`.
    pub refaccpth: Option<FileRef>,
    /// `ALTSEQ`.
    pub altseq: Option<FileRef>,
    /// File-level `TEXT`.
    pub text: Option<String>,
    /// `DYNSLT`: select/omit at read time.
    pub dynslt: bool,
    /// `JDFTVAL`: defaults for unmatched join records.
    pub jdftval: bool,
}

impl LfDefinition {
    /// Whether any format has key fields.
    #[must_use]
    pub fn is_keyed(&self) -> bool {
        self.formats.iter().any(|f| !f.format.keys.is_empty())
    }

    /// Whether this is a join logical file.
    #[must_use]
    pub fn is_join(&self) -> bool {
        self.formats.iter().any(|f| f.join)
    }
}
