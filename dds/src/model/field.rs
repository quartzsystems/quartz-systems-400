// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A field of a record format and everything DDS can say about it.

use std::fmt;

use qsname::Name;

use super::FileRef;
use crate::lex::Parm;

/// Position 35.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DataType {
    /// `A`: character.
    Char,
    /// `H`: hexadecimal.
    Hex,
    /// `G`: graphic (DBCS), two bytes per character.
    Graphic,
    /// `O`: DBCS-open.
    DbcsOpen,
    /// `S`: zoned decimal.
    Zoned,
    /// `P`: packed decimal.
    Packed,
    /// `B`: binary.
    Binary,
    /// `F`: floating point.
    Float,
    /// `L`: date.
    Date,
    /// `T`: time.
    Time,
    /// `Z`: timestamp.
    Timestamp,
}

impl DataType {
    /// From the letter in position 35.
    #[must_use]
    pub fn from_letter(ch: char) -> Option<Self> {
        Some(match ch {
            'A' => Self::Char,
            'H' => Self::Hex,
            'G' => Self::Graphic,
            'O' => Self::DbcsOpen,
            'S' => Self::Zoned,
            'P' => Self::Packed,
            'B' => Self::Binary,
            'F' => Self::Float,
            'L' => Self::Date,
            'T' => Self::Time,
            'Z' => Self::Timestamp,
            _ => return None,
        })
    }

    /// The letter.
    #[must_use]
    pub fn letter(self) -> char {
        match self {
            Self::Char => 'A',
            Self::Hex => 'H',
            Self::Graphic => 'G',
            Self::DbcsOpen => 'O',
            Self::Zoned => 'S',
            Self::Packed => 'P',
            Self::Binary => 'B',
            Self::Float => 'F',
            Self::Date => 'L',
            Self::Time => 'T',
            Self::Timestamp => 'Z',
        }
    }

    /// `S`, `P`, `B` or `F`.
    #[must_use]
    pub fn is_numeric(self) -> bool {
        matches!(
            self,
            Self::Zoned | Self::Packed | Self::Binary | Self::Float
        )
    }

    /// `A`, `H`, `G` or `O`.
    #[must_use]
    pub fn is_character(self) -> bool {
        matches!(
            self,
            Self::Char | Self::Hex | Self::Graphic | Self::DbcsOpen
        )
    }

    /// `L`, `T` or `Z`.
    #[must_use]
    pub fn is_datetime(self) -> bool {
        matches!(self, Self::Date | Self::Time | Self::Timestamp)
    }

    /// Whether a length must be written; dates, times and timestamps
    /// take theirs from the format.
    #[must_use]
    pub fn needs_length(self) -> bool {
        !self.is_datetime()
    }

    /// Allowed lengths, in characters or digits.
    #[must_use]
    pub fn length_range(self) -> (u32, u32) {
        match self {
            Self::Char | Self::Hex => (1, 32766),
            Self::Graphic => (1, 16383),
            Self::DbcsOpen => (4, 32766),
            Self::Zoned | Self::Packed => (1, 63),
            Self::Binary => (1, 18),
            Self::Float => (1, 17),
            Self::Date => (6, 10),
            Self::Time => (8, 8),
            Self::Timestamp => (26, 26),
        }
    }

    /// Bytes a field of this type takes in the record.
    #[must_use]
    pub fn byte_len(self, length: u32, precision: FloatPrecision) -> u32 {
        match self {
            Self::Char | Self::Hex | Self::DbcsOpen | Self::Zoned | Self::Date => length,
            Self::Graphic => length * 2,
            Self::Packed => length / 2 + 1,
            Self::Binary => match length {
                0..=4 => 2,
                5..=9 => 4,
                _ => 8,
            },
            Self::Float => match precision {
                FloatPrecision::Single => 4,
                FloatPrecision::Double => 8,
            },
            Self::Time => 8,
            Self::Timestamp => 26,
        }
    }
}

/// `FLTPCN`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FloatPrecision {
    /// `*SINGLE`, 4 bytes, up to 9 digits.
    #[default]
    Single,
    /// `*DOUBLE`, 8 bytes, up to 17 digits.
    Double,
}

/// `DATFMT`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DateFmt {
    /// `yyyy-mm-dd`.
    #[default]
    Iso,
    /// `mm/dd/yyyy`.
    Usa,
    /// `dd.mm.yyyy`.
    Eur,
    /// `yyyy-mm-dd`.
    Jis,
    /// `mm/dd/yy`.
    Mdy,
    /// `dd/mm/yy`.
    Dmy,
    /// `yy/mm/dd`.
    Ymd,
    /// `yy/ddd`.
    Jul,
}

impl DateFmt {
    /// From `*ISO` and the like.
    #[must_use]
    pub fn from_word(w: &str) -> Option<Self> {
        Some(match w {
            "*ISO" => Self::Iso,
            "*USA" => Self::Usa,
            "*EUR" => Self::Eur,
            "*JIS" => Self::Jis,
            "*MDY" => Self::Mdy,
            "*DMY" => Self::Dmy,
            "*YMD" => Self::Ymd,
            "*JUL" => Self::Jul,
            _ => return None,
        })
    }

    /// The keyword value.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Iso => "*ISO",
            Self::Usa => "*USA",
            Self::Eur => "*EUR",
            Self::Jis => "*JIS",
            Self::Mdy => "*MDY",
            Self::Dmy => "*DMY",
            Self::Ymd => "*YMD",
            Self::Jul => "*JUL",
        }
    }

    /// Field length in this format.
    #[must_use]
    pub fn length(self) -> u32 {
        match self {
            Self::Iso | Self::Usa | Self::Eur | Self::Jis => 10,
            Self::Mdy | Self::Dmy | Self::Ymd => 8,
            Self::Jul => 6,
        }
    }

    /// The separator the format fixes; `None` when `DATSEP` chooses it.
    #[must_use]
    pub fn fixed_separator(self) -> Option<char> {
        match self {
            Self::Iso | Self::Jis => Some('-'),
            Self::Usa => Some('/'),
            Self::Eur => Some('.'),
            Self::Mdy | Self::Dmy | Self::Ymd | Self::Jul => None,
        }
    }
}

/// `TIMFMT`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TimeFmt {
    /// `hh:mm:ss`.
    Hms,
    /// `hh.mm.ss`.
    #[default]
    Iso,
    /// `hh:mm AM`.
    Usa,
    /// `hh.mm.ss`.
    Eur,
    /// `hh:mm:ss`.
    Jis,
}

impl TimeFmt {
    /// From `*ISO` and the like.
    #[must_use]
    pub fn from_word(w: &str) -> Option<Self> {
        Some(match w {
            "*HMS" => Self::Hms,
            "*ISO" => Self::Iso,
            "*USA" => Self::Usa,
            "*EUR" => Self::Eur,
            "*JIS" => Self::Jis,
            _ => return None,
        })
    }

    /// The keyword value.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Hms => "*HMS",
            Self::Iso => "*ISO",
            Self::Usa => "*USA",
            Self::Eur => "*EUR",
            Self::Jis => "*JIS",
        }
    }

    /// The separator the format fixes; `None` when `TIMSEP` chooses it.
    #[must_use]
    pub fn fixed_separator(self) -> Option<char> {
        match self {
            Self::Hms => None,
            Self::Iso | Self::Eur => Some('.'),
            Self::Usa | Self::Jis => Some(':'),
        }
    }
}

/// Position 38.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Usage {
    /// `B`.
    Both,
    /// `I`.
    Input,
    /// `O`.
    Output,
    /// `N`.
    Neither,
    /// `H`.
    Hidden,
    /// `M`.
    Message,
    /// `P`.
    Program,
}

impl Usage {
    /// From the letter in position 38.
    #[must_use]
    pub fn from_letter(ch: char) -> Option<Self> {
        Some(match ch {
            'B' => Self::Both,
            'I' => Self::Input,
            'O' => Self::Output,
            'N' => Self::Neither,
            'H' => Self::Hidden,
            'M' => Self::Message,
            'P' => Self::Program,
            _ => return None,
        })
    }
}

/// `VARLEN`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VarLen {
    /// Bytes kept in the record itself; the rest overflows.
    pub allocated: Option<u32>,
}

/// A literal in a keyword.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Literal {
    /// An unquoted word: a number or special value.
    Word(String),
    /// A quoted string.
    Quoted(String),
    /// `X'...'`.
    Hex(Vec<u8>),
}

impl Literal {
    /// From a parameter; groups are not literals.
    #[must_use]
    pub fn from_parm(p: &Parm) -> Option<Self> {
        Some(match p {
            Parm::Word(w) => Self::Word(w.clone()),
            Parm::Quoted(q) => Self::Quoted(q.clone()),
            Parm::Hex(h) => Self::Hex(h.clone()),
            Parm::Group(_) => return None,
        })
    }
}

impl fmt::Display for Literal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Word(w) => f.write_str(w),
            Self::Quoted(q) => write!(f, "'{}'", q.replace('\'', "''")),
            Self::Hex(h) => {
                f.write_str("X'")?;
                for b in h {
                    write!(f, "{b:02X}")?;
                }
                f.write_str("'")
            }
        }
    }
}

/// `DFT`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DefaultValue {
    /// `*NULL`.
    Null,
    /// A quoted string: character, date, time or timestamp.
    Quoted(String),
    /// `X'...'`.
    Hex(Vec<u8>),
    /// A numeric literal as written.
    Number(String),
}

/// The relational operator of `COMP`/`CMP`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CompOp {
    /// Equal.
    Eq,
    /// Not equal.
    Ne,
    /// Less than.
    Lt,
    /// Not less than.
    Nl,
    /// Greater than.
    Gt,
    /// Not greater than.
    Ng,
    /// Less than or equal.
    Le,
    /// Greater than or equal.
    Ge,
}

impl CompOp {
    /// From `EQ`, `NE`, ...
    #[must_use]
    pub fn from_word(w: &str) -> Option<Self> {
        Some(match w {
            "EQ" => Self::Eq,
            "NE" => Self::Ne,
            "LT" => Self::Lt,
            "NL" => Self::Nl,
            "GT" => Self::Gt,
            "NG" => Self::Ng,
            "LE" => Self::Le,
            "GE" => Self::Ge,
            _ => return None,
        })
    }

    /// The keyword spelling.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Eq => "EQ",
            Self::Ne => "NE",
            Self::Lt => "LT",
            Self::Nl => "NL",
            Self::Gt => "GT",
            Self::Ng => "NG",
            Self::Le => "LE",
            Self::Ge => "GE",
        }
    }
}

/// Validity checking keywords, kept for DSPFFD and the cross-reference;
/// database I/O does not enforce them.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Validity {
    /// `CHECK(...)` codes.
    Check(Vec<String>),
    /// `VALUES(...)`.
    Values(Vec<Literal>),
    /// `RANGE(low high)`.
    Range(Literal, Literal),
    /// `COMP(op value)`.
    Comp(CompOp, Literal),
}

/// `EDTCDE`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EditCode {
    /// The code: `1`-`4`, `A`-`D`, `J`-`Q`, `X`, `Y`, `Z`, or a user code `5`-`9`.
    pub code: char,
    /// `*CR`, `-`, or a floating currency symbol.
    pub modifier: Option<String>,
}

/// Where a referenced field came from.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RefSource {
    /// Another file.
    File(FileRef),
    /// `*SRC`: a field earlier in the same source.
    Source,
}

/// A field's reference, when it has one.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Reference {
    /// Where.
    pub source: RefSource,
    /// The referenced field.
    pub field: Name,
    /// The record format it was found in; `None` for `*SRC`.
    pub format: Option<Name>,
}

/// One field of a record format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldDef {
    /// The name.
    pub name: Name,
    /// Position 35, or the default.
    pub data_type: DataType,
    /// Characters or digits.
    pub length: u32,
    /// Decimal positions, 0 for non-numeric types.
    pub decimals: u8,
    /// `FLTPCN`, floats only.
    pub precision: FloatPrecision,
    /// Bytes in the record, the `VARLEN` prefix included.
    pub byte_len: u32,
    /// 0-based byte offset in the record.
    pub offset: u32,
    /// Position 38.
    pub usage: Option<Usage>,
    /// `VARLEN`.
    pub varlen: Option<VarLen>,
    /// `ALWNULL`.
    pub allow_null: bool,
    /// `CCSID`.
    pub ccsid: Option<u16>,
    /// `TEXT`.
    pub text: Option<String>,
    /// `ALIAS`.
    pub alias: Option<String>,
    /// `COLHDG`, up to three lines.
    pub colhdg: Vec<String>,
    /// `DFT`.
    pub default: Option<DefaultValue>,
    /// `DATFMT`, dates only.
    pub date_fmt: Option<DateFmt>,
    /// The date separator in effect, dates only.
    pub date_sep: Option<char>,
    /// `TIMFMT`, times only.
    pub time_fmt: Option<TimeFmt>,
    /// The time separator in effect, times only.
    pub time_sep: Option<char>,
    /// `EDTCDE`.
    pub edit_code: Option<EditCode>,
    /// `EDTWRD`.
    pub edit_word: Option<String>,
    /// `CHECK`, `VALUES`, `RANGE`, `COMP`.
    pub validity: Vec<Validity>,
    /// The reference the attributes came from.
    pub reference: Option<Reference>,
    /// Source line index of the specification.
    pub line: usize,
}

impl FieldDef {
    /// A field with the given shape and nothing else.
    #[must_use]
    pub fn new(name: Name, data_type: DataType, length: u32, decimals: u8) -> Self {
        let precision = FloatPrecision::default();
        Self {
            name,
            data_type,
            length,
            decimals,
            precision,
            byte_len: data_type.byte_len(length, precision),
            offset: 0,
            usage: None,
            varlen: None,
            allow_null: false,
            ccsid: None,
            text: None,
            alias: None,
            colhdg: Vec::new(),
            default: None,
            date_fmt: None,
            date_sep: None,
            time_fmt: None,
            time_sep: None,
            edit_code: None,
            edit_word: None,
            validity: Vec::new(),
            reference: None,
            line: 0,
        }
    }

    /// Bytes of data without the `VARLEN` prefix.
    #[must_use]
    pub fn data_len(&self) -> u32 {
        self.data_type.byte_len(self.length, self.precision)
    }

    /// `A 30`, `P 7,2`, `L *ISO` for listings.
    #[must_use]
    pub fn shape(&self) -> String {
        match self.data_type {
            t if t.is_numeric() => format!("{} {},{}", t.letter(), self.length, self.decimals),
            DataType::Date => format!("L {}", self.date_fmt.unwrap_or_default().name()),
            DataType::Time => format!("T {}", self.time_fmt.unwrap_or_default().name()),
            t => format!("{} {}", t.letter(), self.length),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_lengths() {
        use DataType as D;
        let s = FloatPrecision::Single;
        assert_eq!(D::Char.byte_len(30, s), 30);
        assert_eq!(D::Graphic.byte_len(10, s), 20);
        assert_eq!(D::Packed.byte_len(7, s), 4);
        assert_eq!(D::Packed.byte_len(8, s), 5);
        assert_eq!(D::Zoned.byte_len(9, s), 9);
        assert_eq!(D::Binary.byte_len(4, s), 2);
        assert_eq!(D::Binary.byte_len(9, s), 4);
        assert_eq!(D::Binary.byte_len(18, s), 8);
        assert_eq!(D::Float.byte_len(9, s), 4);
        assert_eq!(D::Float.byte_len(17, FloatPrecision::Double), 8);
        assert_eq!(D::Date.byte_len(10, s), 10);
        assert_eq!(D::Time.byte_len(8, s), 8);
        assert_eq!(D::Timestamp.byte_len(26, s), 26);
        for ch in "AHGOSPBFLTZ".chars() {
            assert_eq!(D::from_letter(ch).unwrap().letter(), ch);
        }
        assert!(D::from_letter('X').is_none());
    }

    #[test]
    fn formats() {
        assert_eq!(DateFmt::from_word("*MDY").unwrap().length(), 8);
        assert_eq!(DateFmt::Jul.length(), 6);
        assert_eq!(DateFmt::Iso.fixed_separator(), Some('-'));
        assert_eq!(DateFmt::Mdy.fixed_separator(), None);
        assert_eq!(TimeFmt::from_word("*HMS").unwrap().fixed_separator(), None);
        assert_eq!(TimeFmt::Usa.fixed_separator(), Some(':'));
        assert_eq!(CompOp::from_word("GE").unwrap().name(), "GE");
    }
}
