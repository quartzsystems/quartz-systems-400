// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CPYTOIMPF` and `CPYFRMIMPF`: records as delimited or fixed-column
//! UTF-8 text. The storage service writes it to a stream file
//! (`TOSTMF`) or reads it from one (`FROMSTMF`) through the job's IFS,
//! converting to and from the file's CCSID (`ifscmd::stmf::impf`); a
//! name without a `/` is a console `xfer` buffer.

mod csv;
mod from;
mod to;

pub use from::cpyfrmimpf;
pub use to::cpytoimpf;

use dds::{DataType, DateFmt, FieldDef, TimeFmt};

use super::ErrLvl;

/// `DTAFMT`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DtaFmt {
    /// `*DLM`: delimited fields.
    #[default]
    Csv,
    /// `*FIXED`: every field at its display width.
    Fixed,
}

/// `RCDDLM`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RcdDlm {
    /// `*EOR`: carriage return and line feed written; either read.
    #[default]
    Eor,
    /// `*LF`.
    Lf,
    /// `*CR`.
    Cr,
    /// `*CRLF`.
    Crlf,
}

impl RcdDlm {
    fn text(self) -> &'static str {
        match self {
            Self::Eor | Self::Crlf => "\r\n",
            Self::Lf => "\n",
            Self::Cr => "\r",
        }
    }
}

/// `RMVBLANK`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RmvBlank {
    /// `*NONE`.
    #[default]
    None,
    /// `*LEADING`.
    Leading,
    /// `*TRAILING`.
    Trailing,
    /// `*BOTH`.
    Both,
}

impl RmvBlank {
    fn apply(self, s: &str) -> &str {
        match self {
            Self::None => s,
            Self::Leading => s.trim_start(),
            Self::Trailing => s.trim_end(),
            Self::Both => s.trim(),
        }
    }
}

/// The parameters both commands share.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImpfOpts {
    /// `DTAFMT`.
    pub dtafmt: DtaFmt,
    /// `RCDDLM`.
    pub rcddlm: RcdDlm,
    /// `STRDLM`, `None` for `*NONE`.
    pub strdlm: Option<char>,
    /// `FLDDLM`.
    pub flddlm: char,
    /// `DECPNT`.
    pub decpnt: char,
    /// `DATFMT`, `None` for each field's own.
    pub datfmt: Option<DateFmt>,
    /// `DATSEP`.
    pub datsep: Option<char>,
    /// `TIMFMT`, `None` for each field's own.
    pub timfmt: Option<TimeFmt>,
    /// `TIMSEP`.
    pub timsep: Option<char>,
    /// `RMVBLANK`.
    pub rmvblank: RmvBlank,
    /// `NULLS(*YES)`: a null field is empty in the text.
    pub nulls: bool,
    /// `ERRLVL` (import).
    pub errlvl: ErrLvl,
}

impl Default for ImpfOpts {
    fn default() -> Self {
        Self {
            dtafmt: DtaFmt::Csv,
            rcddlm: RcdDlm::Eor,
            strdlm: Some('"'),
            flddlm: ',',
            decpnt: '.',
            datfmt: None,
            datsep: None,
            timfmt: None,
            timsep: None,
            rmvblank: RmvBlank::None,
            nulls: false,
            errlvl: ErrLvl::None,
        }
    }
}

/// What an import did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImpfReport {
    /// Records written.
    pub records: u64,
    /// Records not written.
    pub errors: u64,
    /// The records not written, numbered from 1, with the reason.
    pub skipped: Vec<(u64, String)>,
}

/// A field's width in `*FIXED` text: text at its length, numbers with
/// room for a sign and a point, dates and times as stored.
#[must_use]
pub fn fixed_width(f: &FieldDef) -> usize {
    let n = |x: u32| usize::try_from(x).expect("fits");
    match f.data_type {
        DataType::Char | DataType::Hex | DataType::Graphic | DataType::DbcsOpen => n(f.length),
        DataType::Zoned | DataType::Packed | DataType::Binary | DataType::Float => {
            n(f.length) + 1 + usize::from(f.decimals > 0)
        }
        DataType::Date | DataType::Time | DataType::Timestamp => n(f.byte_len),
    }
}

/// Whether the field is written as text (quoted in `*DLM`).
fn is_text(f: &FieldDef) -> bool {
    !f.data_type.is_numeric()
}
