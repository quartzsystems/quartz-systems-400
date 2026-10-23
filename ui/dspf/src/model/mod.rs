// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a compile produces: the file attributes, the record formats
//! with their fields and buffers, and the source. Every keyword that
//! was accepted is kept with its conditioning in a `keywords` list at
//! its level; the typed fields beside the lists are the ones the
//! compiler checks and the runtime acts on.

pub mod field;
pub mod record;

use std::collections::BTreeMap;
use std::fmt;

use dds::model::FileRef;
use dds::{Parm, Source};

pub use field::{
    Attr, Constant, Edit, ErrMsg, Field, FieldType, Position, SflRole, TypeClass, Usage,
};
pub use record::{
    Buffer, BufferField, Clrl, Record, RecordKind, RecordLayout, SflEnd, SflSize, Slno, Subfile,
    Window, WindowOrigin,
};

use crate::cond::Cond;

/// One keyword as written, with its conditioning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeywordUse {
    /// The name, uppercased.
    pub name: String,
    /// Parameters in order.
    pub parms: Vec<Parm>,
    /// When it is in effect.
    pub cond: Cond,
    /// Source line index.
    pub line: usize,
}

impl fmt::Display for KeywordUse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)?;
        if !self.parms.is_empty() {
            f.write_str("(")?;
            for (i, p) in self.parms.iter().enumerate() {
                if i > 0 {
                    f.write_str(" ")?;
                }
                write!(f, "{p}")?;
            }
            f.write_str(")")?;
        }
        Ok(())
    }
}

/// One `DSPSIZ` entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplaySize {
    /// Lines.
    pub rows: u8,
    /// Columns.
    pub cols: u8,
    /// The condition name, `*DS3` or `*DS4` by default.
    pub name: String,
}

impl DisplaySize {
    /// 24 by 80.
    #[must_use]
    pub fn ds3() -> Self {
        Self {
            rows: 24,
            cols: 80,
            name: "*DS3".into(),
        }
    }

    /// 27 by 132.
    #[must_use]
    pub fn ds4() -> Self {
        Self {
            rows: 27,
            cols: 132,
            name: "*DS4".into(),
        }
    }

    /// Positions on the display.
    #[must_use]
    pub fn cells(&self) -> u32 {
        u32::from(self.rows) * u32::from(self.cols)
    }

    /// `24x80 (*DS3)` for messages.
    #[must_use]
    pub fn label(&self) -> String {
        format!("{}x{} ({})", self.rows, self.cols, self.name)
    }
}

impl Default for DisplaySize {
    fn default() -> Self {
        Self::ds3()
    }
}

/// A key or event a program is told about through a response indicator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum KeyName {
    /// `CAnn`: the key returns no data.
    Ca(u8),
    /// `CFnn`: the key returns the changed data.
    Cf(u8),
    /// `ROLLUP`.
    RollUp,
    /// `ROLLDOWN`.
    RollDown,
    /// `PAGEUP`.
    PageUp,
    /// `PAGEDOWN`.
    PageDown,
    /// `CLEAR`.
    Clear,
    /// `HOME`.
    Home,
    /// `HELP`.
    Help,
    /// `PRINT`.
    Print,
    /// `VLDCMDKEY`: any key the record allows.
    VldCmdKey,
}

impl KeyName {
    /// From the keyword name.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        if let Some(n) = crate::kw::cmd_key_number(name) {
            return match &name[..2] {
                "CA" => Some(Self::Ca(n)),
                _ => Some(Self::Cf(n)),
            };
        }
        Some(match name {
            "ROLLUP" => Self::RollUp,
            "ROLLDOWN" => Self::RollDown,
            "PAGEUP" => Self::PageUp,
            "PAGEDOWN" => Self::PageDown,
            "CLEAR" => Self::Clear,
            "HOME" => Self::Home,
            "HELP" => Self::Help,
            "PRINT" => Self::Print,
            "VLDCMDKEY" => Self::VldCmdKey,
            _ => return None,
        })
    }

    /// The keyword name.
    #[must_use]
    pub fn name(self) -> String {
        match self {
            Self::Ca(n) => format!("CA{n:02}"),
            Self::Cf(n) => format!("CF{n:02}"),
            Self::RollUp => "ROLLUP".into(),
            Self::RollDown => "ROLLDOWN".into(),
            Self::PageUp => "PAGEUP".into(),
            Self::PageDown => "PAGEDOWN".into(),
            Self::Clear => "CLEAR".into(),
            Self::Home => "HOME".into(),
            Self::Help => "HELP".into(),
            Self::Print => "PRINT".into(),
            Self::VldCmdKey => "VLDCMDKEY".into(),
        }
    }

    /// The function key number of `CAnn`/`CFnn`.
    #[must_use]
    pub fn number(self) -> Option<u8> {
        match self {
            Self::Ca(n) | Self::Cf(n) => Some(n),
            _ => None,
        }
    }
}

/// A key the program is told about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CmdKey {
    /// Which key.
    pub key: KeyName,
    /// The response indicator set when it is pressed.
    pub ind: Option<u8>,
    /// The indicator text.
    pub text: Option<String>,
    /// When the key is enabled.
    pub cond: Cond,
    /// Source line index.
    pub line: usize,
}

/// The file-level keywords.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // one flag per keyword that is there or not
pub struct FileAttrs {
    /// `DSPSIZ`: one or two sizes, the first primary.
    pub sizes: Vec<DisplaySize>,
    /// `INDARA`: indicators in a separate 99-byte area.
    pub indara: bool,
    /// `INDTXT` at every level: indicator and text.
    pub indtxt: Vec<(u8, String)>,
    /// `CHGINPDFT` at file level: the attribute words, empty for
    /// `CHGINPDFT` alone.
    pub chginpdft: Option<Vec<String>>,
    /// `REF`.
    pub ref_file: Option<FileRef>,
    /// `MSGLOC` at file level.
    pub msgloc: Option<u8>,
    /// `RSTDSP`.
    pub rstdsp: bool,
    /// `USRDSPMGT`.
    pub usrdspmgt: bool,
    /// `CSRINPONLY`.
    pub csrinponly: bool,
    /// `DSPRL`.
    pub dsprl: bool,
    /// `ERRSFL`.
    pub errsfl: bool,
    /// `ALTHELP`: the key that also means Help.
    pub alt_help: Option<KeyName>,
    /// `ALTPAGEUP`.
    pub alt_pageup: Option<KeyName>,
    /// `ALTPAGEDWN`.
    pub alt_pagedwn: Option<KeyName>,
    /// Keys enabled for every record.
    pub keys: Vec<CmdKey>,
    /// `CCSID` at file level.
    pub ccsid: Option<u16>,
    /// Response indicators file keywords other than keys set
    /// (`MNUCNL`), with their lines.
    pub responses: Vec<(u8, usize)>,
    /// Every accepted file-level keyword.
    pub keywords: Vec<KeywordUse>,
}

impl Default for FileAttrs {
    fn default() -> Self {
        Self {
            sizes: vec![DisplaySize::ds3()],
            indara: false,
            indtxt: Vec::new(),
            chginpdft: None,
            ref_file: None,
            msgloc: None,
            rstdsp: false,
            usrdspmgt: false,
            csrinponly: false,
            dsprl: false,
            errsfl: false,
            alt_help: None,
            alt_pageup: None,
            alt_pagedwn: None,
            keys: Vec::new(),
            ccsid: None,
            responses: Vec::new(),
            keywords: Vec::new(),
        }
    }
}

impl FileAttrs {
    /// The `*DSn` names in order.
    #[must_use]
    pub fn size_names(&self) -> Vec<String> {
        self.sizes.iter().map(|s| s.name.clone()).collect()
    }
}

/// Where an indicator is used, as source line indices.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IndUse {
    /// Lines conditioning something by it.
    pub option: Vec<usize>,
    /// Lines setting it as a response.
    pub response: Vec<usize>,
}

/// A compiled display file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DisplayFile {
    /// The file-level keywords.
    pub file: FileAttrs,
    /// The record formats in source order.
    pub records: Vec<Record>,
    /// The DDS it was compiled from.
    pub source: Source,
}

impl DisplayFile {
    /// A record format by name.
    #[must_use]
    pub fn record(&self, name: &str) -> Option<&Record> {
        self.records.iter().find(|r| r.name == name)
    }

    /// Bytes of the separate indicator area: 99 with `INDARA`.
    #[must_use]
    pub fn indicator_area_len(&self) -> Option<usize> {
        self.file.indara.then_some(99)
    }

    /// Every indicator and where it is used.
    #[must_use]
    pub fn indicator_usage(&self) -> BTreeMap<u8, IndUse> {
        let mut map: BTreeMap<u8, IndUse> = BTreeMap::new();
        let mut option = |cond: &Cond, line: usize| {
            for n in cond.indicators() {
                map.entry(n).or_default().option.push(line);
            }
        };
        for k in &self.file.keywords {
            option(&k.cond, k.line);
        }
        for r in &self.records {
            for k in &r.keywords {
                option(&k.cond, k.line);
            }
            for f in &r.fields {
                option(&f.cond, f.line);
                for k in &f.keywords {
                    option(&k.cond, k.line);
                }
            }
        }
        let mut response = |ind: u8, line: usize| {
            map.entry(ind).or_default().response.push(line);
        };
        for k in &self.file.keys {
            if let Some(i) = k.ind {
                response(i, k.line);
            }
        }
        for &(i, line) in &self.file.responses {
            response(i, line);
        }
        for r in &self.records {
            for k in &r.keys {
                if let Some(i) = k.ind {
                    response(i, k.line);
                }
            }
            for &(i, line) in &r.responses {
                response(i, line);
            }
            for f in &r.fields {
                for &(i, line) in &f.responses {
                    response(i, line);
                }
            }
        }
        for u in map.values_mut() {
            u.option.sort_unstable();
            u.option.dedup();
            u.response.sort_unstable();
            u.response.dedup();
        }
        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_names() {
        assert_eq!(KeyName::parse("CA03"), Some(KeyName::Ca(3)));
        assert_eq!(KeyName::parse("CF12").unwrap().name(), "CF12");
        assert_eq!(KeyName::parse("ROLLUP"), Some(KeyName::RollUp));
        assert_eq!(KeyName::parse("TEXT"), None);
        assert_eq!(KeyName::Cf(5).number(), Some(5));
        assert_eq!(KeyName::Clear.number(), None);
        assert_eq!(DisplaySize::ds4().cells(), 3564);
        assert_eq!(DisplaySize::default().label(), "24x80 (*DS3)");
    }
}
