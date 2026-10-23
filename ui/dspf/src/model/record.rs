// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A record format: its kind, subfile and window attributes, its keys,
//! its fields and the buffers a program exchanges with it.

use super::field::Field;
use super::{CmdKey, KeywordUse};

/// What kind of record a format is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RecordKind {
    /// An ordinary record.
    #[default]
    Normal,
    /// `SFL`: one subfile record, repeated down the page.
    Subfile,
    /// `SFLCTL`: the record that controls the subfile before it.
    SubfileControl,
    /// `MNUBAR`.
    MenuBar,
    /// `PULLDOWN`.
    PullDown,
}

impl RecordKind {
    /// For listings.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "record",
            Self::Subfile => "subfile record",
            Self::SubfileControl => "subfile control record",
            Self::MenuBar => "menu bar record",
            Self::PullDown => "pull-down record",
        }
    }
}

/// `SFLSIZ` and `SFLPAG` for one display size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SflSize {
    /// `SFLSIZ`: records the subfile can hold.
    pub size: u16,
    /// `SFLPAG`: records on one page.
    pub page: u16,
}

/// `SFLEND`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SflEnd {
    /// `SFLEND` or `SFLEND(*PLUS)`: a plus sign on the last line.
    Plus,
    /// `*MORE`: "More..." and "Bottom".
    More,
    /// `*SCRBAR`: a scroll bar.
    ScrBar,
    /// `*TEXT`: the program's own words.
    Text,
}

impl SflEnd {
    /// The keyword value.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Plus => "*PLUS",
            Self::More => "*MORE",
            Self::ScrBar => "*SCRBAR",
            Self::Text => "*TEXT",
        }
    }
}

/// What a subfile control record says about its subfile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Subfile {
    /// The `SFL` record.
    pub record: String,
    /// Size and page, one entry per display size.
    pub sizes: Vec<SflSize>,
    /// `SFLLIN`: positions between columns of a horizontal subfile.
    pub lin: Option<u16>,
    /// `SFLEND`.
    pub end: Option<SflEnd>,
    /// A message subfile (`SFLMSGRCD` on the subfile record).
    pub message: bool,
    /// Lines one subfile record takes.
    pub record_lines: u8,
    /// The first line of the page, by display size; 0 when the subfile
    /// record has nothing displayed.
    pub first_line: Vec<u8>,
}

impl Subfile {
    /// Size and page on display size `i`.
    #[must_use]
    pub fn size(&self, i: usize) -> SflSize {
        self.sizes
            .get(i)
            .or_else(|| self.sizes.first())
            .copied()
            .unwrap_or_default()
    }

    /// The last line of the page on display size `i`.
    #[must_use]
    pub fn last_line(&self, i: usize) -> u32 {
        let first = u32::from(self.first_line.get(i).copied().unwrap_or(0));
        if first == 0 {
            return 0;
        }
        first + u32::from(self.size(i).page) * u32::from(self.record_lines) - 1
    }
}

/// Where a window is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WindowOrigin {
    /// `*DFT`: the system places it near the cursor.
    Default,
    /// Line and column of the upper-left border corner.
    At(u8, u8),
    /// Line and column from program fields.
    Vars(String, String),
    /// `WINDOW(record)`: the window of another record.
    From(String),
}

/// `WINDOW`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Window {
    /// Where.
    pub origin: WindowOrigin,
    /// Usable lines inside the border.
    pub lines: u8,
    /// Usable columns inside the border.
    pub cols: u8,
}

/// `CLRL`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Clrl {
    /// Clear this many lines from the record's first line.
    Lines(u8),
    /// `*NO`: clear nothing.
    No,
    /// `*END`: clear to the end of the display.
    End,
    /// `*ALL`: clear the whole display.
    All,
}

/// `SLNO`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Slno {
    /// A fixed starting line.
    Line(u8),
    /// `*VAR`: the program gives it.
    Var,
}

/// One field's place in a buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BufferField {
    /// Index into the record's fields.
    pub field: usize,
    /// Byte offset.
    pub offset: u32,
    /// Bytes.
    pub len: u32,
}

/// One record buffer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Buffer {
    /// Indicators at the front, one byte each, ascending; empty with
    /// `INDARA`.
    pub indicators: Vec<u8>,
    /// Fields after them, in source order.
    pub fields: Vec<BufferField>,
    /// Total bytes.
    pub len: u32,
}

impl Buffer {
    /// The entry of field `index`.
    #[must_use]
    pub fn field(&self, index: usize) -> Option<BufferField> {
        self.fields.iter().find(|b| b.field == index).copied()
    }
}

/// The buffers and identifiers of a record format.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecordLayout {
    /// What the program writes.
    pub output: Buffer,
    /// What the program reads.
    pub input: Buffer,
    /// Every option indicator the format uses, ascending.
    pub option_inds: Vec<u8>,
    /// Every response indicator the format sets, ascending.
    pub response_inds: Vec<u8>,
    /// The 13-character format level identifier.
    pub level_id: String,
}

/// One record format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    /// The name.
    pub name: String,
    /// `TEXT`.
    pub text: Option<String>,
    /// What kind of record.
    pub kind: RecordKind,
    /// The subfile a control record drives.
    pub subfile: Option<Subfile>,
    /// `WINDOW`.
    pub window: Option<Window>,
    /// Keys enabled while the record is on the display.
    pub keys: Vec<CmdKey>,
    /// `CLRL`.
    pub clrl: Option<Clrl>,
    /// `SLNO`.
    pub slno: Option<Slno>,
    /// `SFLMSGRCD`: the line a message subfile shows on.
    pub msgrcd: Option<u8>,
    /// Response indicators record keywords set, with their lines.
    pub responses: Vec<(u8, usize)>,
    /// Fields and constants in source order.
    pub fields: Vec<Field>,
    /// The buffers.
    pub layout: RecordLayout,
    /// Every accepted record-level keyword.
    pub keywords: Vec<KeywordUse>,
    /// Source line index of the `R` specification.
    pub line: usize,
}

impl Record {
    /// An empty record.
    #[must_use]
    pub fn new(name: String, line: usize) -> Self {
        Self {
            name,
            text: None,
            kind: RecordKind::Normal,
            subfile: None,
            window: None,
            keys: Vec::new(),
            clrl: None,
            slno: None,
            msgrcd: None,
            responses: Vec::new(),
            fields: Vec::new(),
            layout: RecordLayout::default(),
            keywords: Vec::new(),
            line,
        }
    }

    /// A field by name, with its index.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<(usize, &Field)> {
        self.fields
            .iter()
            .enumerate()
            .find(|(_, f)| f.name.as_deref() == Some(name))
    }

    /// The first keyword called `name`.
    #[must_use]
    pub fn keyword(&self, name: &str) -> Option<&KeywordUse> {
        self.keywords.iter().find(|k| k.name == name)
    }

    /// Whether the record carries `name`.
    #[must_use]
    pub fn has(&self, name: &str) -> bool {
        self.keyword(name).is_some()
    }

    /// The lines the displayed fields use on display size `i`: first
    /// and last, or `None` when nothing is displayed.
    #[must_use]
    pub fn extent(&self, i: usize) -> Option<(u8, u8)> {
        let mut lo = u8::MAX;
        let mut hi = 0;
        for f in &self.fields {
            let Some(p) = f.position(i) else {
                continue;
            };
            lo = lo.min(p.row);
            hi = hi.max(p.row);
        }
        (hi > 0).then_some((lo, hi))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subfile_lines() {
        let s = Subfile {
            record: "SFL1".into(),
            sizes: vec![SflSize { size: 50, page: 10 }],
            lin: None,
            end: Some(SflEnd::More),
            message: false,
            record_lines: 2,
            first_line: vec![5],
        };
        assert_eq!(s.last_line(0), 24);
        assert_eq!(s.size(1).page, 10);
        assert_eq!(SflEnd::ScrBar.name(), "*SCRBAR");
        assert_eq!(RecordKind::default().name(), "record");
    }
}
