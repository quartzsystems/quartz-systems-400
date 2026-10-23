// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A field or constant of a record format: its shape, where it is on
//! each display size, how it is shown and what it checks.

use dds::model::{DateFmt, FileRef, TimeFmt, Validity};

use super::KeywordUse;
use crate::cond::Cond;

/// Position 38.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Usage {
    /// Blank or `O`: the program writes it.
    Output,
    /// `I`: the operator types it.
    Input,
    /// `B`: both.
    Both,
    /// `H`: carried in both buffers, never shown.
    Hidden,
    /// `M`: a message field.
    Message,
    /// `P`: program-to-system data.
    Program,
}

impl Usage {
    /// From position 38, blank meaning output.
    #[must_use]
    pub fn from_letter(ch: Option<char>) -> Option<Self> {
        Some(match ch {
            None | Some('O') => Self::Output,
            Some('I') => Self::Input,
            Some('B') => Self::Both,
            Some('H') => Self::Hidden,
            Some('M') => Self::Message,
            Some('P') => Self::Program,
            _ => return None,
        })
    }

    /// The letter.
    #[must_use]
    pub fn letter(self) -> char {
        match self {
            Self::Output => 'O',
            Self::Input => 'I',
            Self::Both => 'B',
            Self::Hidden => 'H',
            Self::Message => 'M',
            Self::Program => 'P',
        }
    }

    /// Whether the operator can type into it.
    #[must_use]
    pub fn is_input(self) -> bool {
        matches!(self, Self::Input | Self::Both)
    }

    /// Whether it appears on the display.
    #[must_use]
    pub fn is_displayed(self) -> bool {
        !matches!(self, Self::Hidden | Self::Program)
    }

    /// Whether it is in the output buffer.
    #[must_use]
    pub fn in_output(self) -> bool {
        !matches!(self, Self::Input)
    }

    /// Whether it is in the input buffer.
    #[must_use]
    pub fn in_input(self) -> bool {
        matches!(self, Self::Input | Self::Both | Self::Hidden)
    }
}

/// How a field's bytes are laid out in the buffers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TypeClass {
    /// One byte per position.
    Char,
    /// Two bytes per position.
    Graphic,
    /// Zoned decimal, one byte per digit.
    Zoned,
    /// Packed decimal.
    Packed,
    /// Binary, 2, 4 or 8 bytes by digits.
    Binary,
    /// Floating point, 4 or 8 bytes by digits.
    Float,
    /// A date in its `DATFMT`.
    Date,
    /// A time, 8 bytes.
    Time,
    /// A timestamp, 26 bytes.
    Timestamp,
}

/// Position 35 and what it means for the buffers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FieldType {
    /// The letter as written, or the default.
    pub letter: char,
    /// The buffer representation.
    pub class: TypeClass,
}

impl FieldType {
    /// From the letter in position 35. `N` is character without decimal
    /// positions and zoned with them.
    #[must_use]
    pub fn from_letter(ch: char, decimals: bool) -> Option<Self> {
        let class = match ch {
            'A' | 'X' | 'W' | 'I' | 'D' | 'M' | 'E' | 'J' | 'O' => TypeClass::Char,
            'N' if !decimals => TypeClass::Char,
            'N' | 'S' | 'Y' => TypeClass::Zoned,
            'G' => TypeClass::Graphic,
            'P' => TypeClass::Packed,
            'B' => TypeClass::Binary,
            'F' => TypeClass::Float,
            'L' => TypeClass::Date,
            'T' => TypeClass::Time,
            'Z' => TypeClass::Timestamp,
            _ => return None,
        };
        Some(Self { letter: ch, class })
    }

    /// Character, blank in position 35.
    #[must_use]
    pub fn char() -> Self {
        Self {
            letter: 'A',
            class: TypeClass::Char,
        }
    }

    /// Zoned, the default with decimal positions.
    #[must_use]
    pub fn zoned() -> Self {
        Self {
            letter: 'S',
            class: TypeClass::Zoned,
        }
    }

    /// Whether it holds a number.
    #[must_use]
    pub fn is_numeric(self) -> bool {
        matches!(
            self.class,
            TypeClass::Zoned | TypeClass::Packed | TypeClass::Binary | TypeClass::Float
        )
    }

    /// Whether `DATFMT`, `TIMFMT` and their separators apply.
    #[must_use]
    pub fn is_datetime(self) -> bool {
        matches!(
            self.class,
            TypeClass::Date | TypeClass::Time | TypeClass::Timestamp
        )
    }

    /// Whether a length must be written.
    #[must_use]
    pub fn needs_length(self) -> bool {
        !self.is_datetime()
    }

    /// The longest field: positions or digits.
    #[must_use]
    pub fn max_length(self) -> u32 {
        match self.class {
            TypeClass::Char => 32_767,
            TypeClass::Graphic => 16_383,
            TypeClass::Zoned | TypeClass::Packed => 63,
            TypeClass::Binary => 18,
            TypeClass::Float => 17,
            TypeClass::Date => 10,
            TypeClass::Time => 8,
            TypeClass::Timestamp => 26,
        }
    }

    /// Bytes in a buffer for a field of `length` positions or digits.
    #[must_use]
    pub fn bytes(self, length: u32) -> u32 {
        match self.class {
            TypeClass::Char | TypeClass::Zoned | TypeClass::Date => length,
            TypeClass::Graphic => length * 2,
            TypeClass::Packed => length / 2 + 1,
            TypeClass::Binary => match length {
                0..=4 => 2,
                5..=9 => 4,
                _ => 8,
            },
            TypeClass::Float => {
                if length <= 9 {
                    4
                } else {
                    8
                }
            }
            TypeClass::Time => 8,
            TypeClass::Timestamp => 26,
        }
    }
}

/// What a constant field shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Constant {
    /// `'text'` or `DFT('text')`.
    Text(String),
    /// `DATE`: the job or system date, six digits, eight with a
    /// four-digit year.
    Date {
        /// `*SYS` rather than `*JOB`.
        sys: bool,
        /// `*YY`: four-digit year.
        year4: bool,
    },
    /// `TIME`: the time of day, `hh:mm:ss`.
    Time {
        /// `*SYS` rather than `*JOB`.
        sys: bool,
    },
    /// `USER`: the user profile name, ten positions.
    User,
    /// `SYSNAME`: the system name, eight positions.
    SysName,
    /// `MSGCON`: the text of a message description.
    Message {
        /// Positions.
        len: u32,
        /// The message identifier.
        id: String,
        /// The message file.
        file: FileRef,
    },
}

/// A row and column, 1-based.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Position {
    /// Line.
    pub row: u8,
    /// Column.
    pub col: u8,
}

/// `EDTCDE` or `EDTWRD`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Edit {
    /// Digits as they are.
    #[default]
    None,
    /// `EDTCDE(code [* | currency])`.
    Code(char, Option<String>),
    /// `EDTWRD('...')`.
    Word(String),
}

/// `DSPATR(...)` or `COLOR(...)` with its conditioning; a `&field`
/// value names a program-to-system field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attr {
    /// The values as written.
    pub values: Vec<String>,
    /// When they apply.
    pub cond: Cond,
    /// Source line index.
    pub line: usize,
}

/// `ERRMSG` or `ERRMSGID`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErrMsg {
    /// `ERRMSG`: the text.
    pub text: Option<String>,
    /// `ERRMSGID`: the identifier and file.
    pub id: Option<(String, FileRef)>,
    /// The indicator reset when the message is shown.
    pub ind: Option<u8>,
    /// When the message is shown.
    pub cond: Cond,
    /// Source line index.
    pub line: usize,
}

/// A field-level subfile keyword that fixes the field's shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SflRole {
    /// `SFLRCDNBR`: the record to show, zoned 4,0.
    RcdNbr {
        /// `CURSOR`: put the cursor on that record.
        cursor: bool,
        /// `*TOP`: show it on the first line.
        top: bool,
    },
    /// `SFLPGMQ`: the program message queue name, 10 or 276 positions.
    PgmQ,
    /// `SFLMSGKEY`: a message key, 4 positions.
    MsgKey,
    /// `SFLROLVAL`: the roll amount typed by the operator, zoned 4,0.
    RolVal,
}

/// A field or constant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    /// The name; `None` for a constant.
    pub name: Option<String>,
    /// What a constant shows.
    pub constant: Option<Constant>,
    /// Position 35 or its default.
    pub ty: FieldType,
    /// Positions or digits; a constant's display length.
    pub length: u32,
    /// Decimal positions.
    pub decimals: u8,
    /// Position 38.
    pub usage: Usage,
    /// Where it is, one entry per display size; empty for a field
    /// that is not displayed.
    pub positions: Vec<Position>,
    /// The primary position was `+n`.
    pub relative: bool,
    /// Positions on the display, editing and the sign included.
    pub screen_len: u32,
    /// Bytes in a buffer.
    pub bytes: u32,
    /// When the field is displayed.
    pub cond: Cond,
    /// `DSPATR`, in order.
    pub dspatr: Vec<Attr>,
    /// `COLOR`, in order.
    pub color: Vec<Attr>,
    /// Editing.
    pub edit: Edit,
    /// `CHECK` codes.
    pub check: Vec<String>,
    /// `VALUES`, `RANGE`, `COMP`.
    pub validity: Vec<Validity>,
    /// `ERRMSG` and `ERRMSGID`.
    pub errmsg: Vec<ErrMsg>,
    /// `DFT` of a named field.
    pub dft: Option<String>,
    /// `TEXT`.
    pub text: Option<String>,
    /// `REFFLD` or `R`: the field and file the shape came from.
    pub reference: Option<(String, Option<FileRef>)>,
    /// `DATFMT`, dates only.
    pub date_fmt: Option<DateFmt>,
    /// `DATSEP`.
    pub date_sep: Option<char>,
    /// `TIMFMT`, times only.
    pub time_fmt: Option<TimeFmt>,
    /// `TIMSEP`.
    pub time_sep: Option<char>,
    /// A subfile keyword that shapes the field.
    pub sfl_role: Option<SflRole>,
    /// Response indicators the field sets, with the line that says so.
    pub responses: Vec<(u8, usize)>,
    /// Every accepted field-level keyword.
    pub keywords: Vec<KeywordUse>,
    /// Source line index of the specification.
    pub line: usize,
}

impl Field {
    /// A named field with the given shape and nothing else.
    #[must_use]
    pub fn new(
        name: Option<String>,
        ty: FieldType,
        length: u32,
        decimals: u8,
        usage: Usage,
    ) -> Self {
        Self {
            name,
            constant: None,
            ty,
            length,
            decimals,
            usage,
            positions: Vec::new(),
            relative: false,
            screen_len: 0,
            bytes: 0,
            cond: Cond::always(),
            dspatr: Vec::new(),
            color: Vec::new(),
            edit: Edit::None,
            check: Vec::new(),
            validity: Vec::new(),
            errmsg: Vec::new(),
            dft: None,
            text: None,
            reference: None,
            date_fmt: None,
            date_sep: None,
            time_fmt: None,
            time_sep: None,
            sfl_role: None,
            responses: Vec::new(),
            keywords: Vec::new(),
            line: 0,
        }
    }

    /// Whether it is a constant.
    #[must_use]
    pub fn is_constant(&self) -> bool {
        self.constant.is_some()
    }

    /// Whether it is on the display. `SFLPGMQ` and `SFLMSGKEY` fields
    /// carry data for the system and are never shown.
    #[must_use]
    pub fn is_displayed(&self) -> bool {
        self.is_constant()
            || (self.usage.is_displayed()
                && !matches!(self.sfl_role, Some(SflRole::PgmQ | SflRole::MsgKey)))
    }

    /// Whether it has bytes in the output buffer.
    #[must_use]
    pub fn in_output(&self) -> bool {
        !self.is_constant() && self.usage.in_output()
    }

    /// Whether it has bytes in the input buffer.
    #[must_use]
    pub fn in_input(&self) -> bool {
        !self.is_constant() && self.usage.in_input()
    }

    /// The name for messages: the field's, or `'text'` of a constant.
    #[must_use]
    pub fn label(&self) -> String {
        if let Some(n) = &self.name {
            return n.clone();
        }
        match &self.constant {
            Some(Constant::Text(t)) => {
                let short: String = t.chars().take(12).collect();
                if t.chars().count() > 12 {
                    format!("'{short}...'")
                } else {
                    format!("'{short}'")
                }
            }
            Some(Constant::Date { .. }) => "DATE".into(),
            Some(Constant::Time { .. }) => "TIME".into(),
            Some(Constant::User) => "USER".into(),
            Some(Constant::SysName) => "SYSNAME".into(),
            Some(Constant::Message { .. }) => "MSGCON".into(),
            None => "(unnamed)".into(),
        }
    }

    /// The position on display size `i`, the primary's when none was
    /// given for it.
    #[must_use]
    pub fn position(&self, i: usize) -> Option<Position> {
        self.positions
            .get(i)
            .or_else(|| self.positions.first())
            .copied()
    }

    /// `A 30`, `S 7,2`, `L *ISO` for listings.
    #[must_use]
    pub fn shape(&self) -> String {
        match self.ty.class {
            TypeClass::Date => format!("L {}", self.date_fmt.unwrap_or_default().name()),
            TypeClass::Time => format!("T {}", self.time_fmt.unwrap_or_default().name()),
            _ if self.ty.is_numeric() => {
                format!("{} {},{}", self.ty.letter, self.length, self.decimals)
            }
            _ => format!("{} {}", self.ty.letter, self.length),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types() {
        assert_eq!(
            FieldType::from_letter('N', false).unwrap().class,
            TypeClass::Char
        );
        assert_eq!(
            FieldType::from_letter('N', true).unwrap().class,
            TypeClass::Zoned
        );
        assert_eq!(
            FieldType::from_letter('Y', false).unwrap().class,
            TypeClass::Zoned
        );
        assert!(FieldType::from_letter('Q', false).is_none());
        let p = FieldType::from_letter('P', true).unwrap();
        assert_eq!(p.bytes(7), 4);
        assert_eq!(FieldType::from_letter('B', true).unwrap().bytes(9), 4);
        assert_eq!(FieldType::from_letter('F', true).unwrap().bytes(17), 8);
        assert_eq!(FieldType::from_letter('G', false).unwrap().bytes(5), 10);
        assert_eq!(FieldType::from_letter('Z', false).unwrap().bytes(26), 26);
        assert!(!FieldType::from_letter('L', false).unwrap().needs_length());
    }

    #[test]
    fn usages() {
        assert_eq!(Usage::from_letter(None), Some(Usage::Output));
        assert_eq!(Usage::from_letter(Some('B')), Some(Usage::Both));
        assert!(Usage::from_letter(Some('N')).is_none());
        assert!(Usage::Hidden.in_input() && Usage::Hidden.in_output());
        assert!(!Usage::Hidden.is_displayed());
        assert!(!Usage::Input.in_output());
        assert!(Usage::Message.in_output() && !Usage::Message.in_input());
    }

    #[test]
    fn labels() {
        let mut f = Field::new(None, FieldType::char(), 20, 0, Usage::Output);
        f.constant = Some(Constant::Text("Customer maintenance".into()));
        assert_eq!(f.label(), "'Customer mai...'");
        assert_eq!(f.position(1), None);
        f.positions.push(Position { row: 1, col: 2 });
        assert_eq!(f.position(1), Some(Position { row: 1, col: 2 }));
        let n = Field::new(Some("AMT".into()), FieldType::zoned(), 7, 2, Usage::Both);
        assert_eq!(n.shape(), "S 7,2");
    }
}
