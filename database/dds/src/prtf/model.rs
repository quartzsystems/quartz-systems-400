// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A compiled printer file: record formats of fields and constants at
//! lines and positions, with spacing, editing and conditioning.

/// Spacing and skipping around a record or a field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Spacing {
    /// `SKIPB`: skip to this line before; 0 for none.
    pub skipb: u16,
    /// `SPACEB`: lines to space before.
    pub spaceb: u16,
    /// `SPACEA`: lines to space after.
    pub spacea: u16,
    /// `SKIPA`: skip to this line after; 0 for none.
    pub skipa: u16,
}

impl Spacing {
    /// Whether anything is asked before the line.
    #[must_use]
    pub fn before(&self) -> bool {
        self.skipb > 0 || self.spaceb > 0
    }

    /// Whether anything is asked at all.
    #[must_use]
    pub fn any(&self) -> bool {
        self.before() || self.spacea > 0 || self.skipa > 0
    }
}

/// What a field prints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrtKind {
    /// Program data, character, this many bytes.
    Char(u16),
    /// Program data, zoned decimal: digits and decimal positions.
    Zoned(u8, u8),
    /// A constant.
    Constant(String),
    /// `DATE`: the job date.
    Date,
    /// `TIME`: the time of day.
    Time,
    /// `PAGNBR`: the page number, four digits.
    PagNbr,
}

/// How a numeric field is edited.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Edit {
    /// As it is (zoned digits with the sign shown).
    #[default]
    None,
    /// `EDTCDE(code [*|fill])`: the code and an asterisk or currency fill.
    Code(char, Option<char>),
    /// `EDTWRD('...')`.
    Word(String),
}

/// One field or constant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrtField {
    /// The field name; empty for a constant or a `DATE`/`TIME`/`PAGNBR`
    /// without one.
    pub name: String,
    /// What it prints.
    pub kind: PrtKind,
    /// The line, 0 for the record's current line.
    pub line: u16,
    /// The position: absolute, or after the previous field when
    /// `relative`.
    pub pos: u16,
    /// `+n` in positions 42-44.
    pub relative: bool,
    /// Conditioning: every indicator must be on (or off when negated).
    pub cond: Vec<(u8, bool)>,
    /// Editing.
    pub edit: Edit,
    /// `UNDERLINE`.
    pub underline: bool,
    /// `HIGHLIGHT`.
    pub highlight: bool,
    /// Field-level spacing: a field with spacing before starts a line.
    pub spacing: Spacing,
}

impl PrtField {
    /// Bytes the field takes in the record buffer; 0 for one that is not
    /// program data.
    #[must_use]
    pub fn buffer_len(&self) -> usize {
        match self.kind {
            PrtKind::Char(n) => usize::from(n),
            PrtKind::Zoned(d, _) => usize::from(d),
            _ => 0,
        }
    }
}

/// One record format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrtFormat {
    /// The format name.
    pub name: String,
    /// `TEXT`.
    pub text: Option<String>,
    /// Record-level spacing.
    pub spacing: Spacing,
    /// Fields and constants in source order.
    pub fields: Vec<PrtField>,
}

impl PrtFormat {
    /// The option indicators the format uses, ascending. Without
    /// `INDARA` the record buffer starts with one byte for each.
    #[must_use]
    pub fn indicators(&self) -> Vec<u8> {
        let mut v: Vec<u8> = self
            .fields
            .iter()
            .flat_map(|f| f.cond.iter().map(|(n, _)| *n))
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// Bytes of the record buffer.
    #[must_use]
    pub fn buffer_len(&self, indara: bool) -> usize {
        let ind = if indara { 0 } else { self.indicators().len() };
        ind + self.fields.iter().map(PrtField::buffer_len).sum::<usize>()
    }
}

/// A printer file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PrtfDefinition {
    /// `INDARA`: indicators are kept apart from the record buffer.
    pub indara: bool,
    /// `TEXT` at file level.
    pub text: Option<String>,
    /// The record formats.
    pub formats: Vec<PrtFormat>,
}

impl PrtfDefinition {
    /// A format by name.
    #[must_use]
    pub fn format(&self, name: &str) -> Option<&PrtFormat> {
        self.formats.iter().find(|f| f.name == name)
    }
}
