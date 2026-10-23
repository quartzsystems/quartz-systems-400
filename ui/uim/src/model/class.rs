// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Classes: a base type, a display width and the translation list and
//! checks that go with it.

/// A `:CLASS.`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Class {
    /// Name.
    pub name: String,
    /// `BASETYPE`.
    pub basetype: BaseType,
    /// `WIDTH`, when given.
    pub width: Option<u32>,
    /// `SHIFT`.
    pub shift: Shift,
    /// `CASE`.
    pub case: Case,
    /// `CHRID`.
    pub chrid: Option<String>,
    /// `ECPARAM`: the parameter the exit program gets.
    pub ecparam: Option<String>,
    /// The `:TL.` translation list, in order.
    pub translations: Vec<TranslateItem>,
    /// The message a value outside the translation list gets (`:TL MSG=`).
    pub tl_msg: Option<String>,
    /// `:CHECK.` items.
    pub checks: Vec<Check>,
    /// Source line.
    pub line: u32,
}

impl Class {
    /// The width a field of this class shows, from `WIDTH` or the base type.
    #[must_use]
    pub fn display_width(&self) -> Option<u32> {
        self.width.or_else(|| self.basetype.width())
    }
}

/// `BASETYPE`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BaseType {
    /// `CHAR n`.
    Char(u32),
    /// `IGC n`: double-byte capable text.
    Igc(u32),
    /// `BIN n`: 15 or 31 bits.
    Bin(u32),
    /// `PACKED d p`.
    Packed(u32, u32),
    /// `ZONED d p`.
    Zoned(u32, u32),
    /// `FLOAT n`: 4 or 8 bytes.
    Float(u32),
    /// `DATE`.
    Date,
    /// `TIME`.
    Time,
    /// `PTR`: a space pointer.
    Ptr,
    /// `OBJNAME`.
    ObjName,
    /// `OBJNAMEQ`: a qualified object name.
    ObjNameQ,
    /// `NAME`: a simple name.
    Name,
    /// `SNAME`: a system name.
    SName,
    /// `DEC`: a decimal number of any precision.
    Dec,
    /// `ACTION`: a list action.
    Action,
    /// `CMDLINE`.
    CmdLine,
}

impl BaseType {
    /// The width the base type implies, or `None` when `WIDTH` must say.
    #[must_use]
    pub fn width(&self) -> Option<u32> {
        Some(match self {
            Self::Char(n) | Self::Igc(n) => *n,
            Self::Bin(15) => 6,
            Self::Bin(_) => 11,
            Self::Packed(d, p) | Self::Zoned(d, p) => d + 1 + u32::from(*p > 0),
            Self::Float(_) | Self::Dec | Self::Ptr | Self::CmdLine => return None,
            Self::Date | Self::ObjName | Self::Name | Self::SName => 10,
            Self::Time => 8,
            Self::ObjNameQ => 21,
            Self::Action => 2,
        })
    }
}

/// `SHIFT`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Shift {
    /// No shift: as the base type.
    #[default]
    None,
    /// Alphabetic shift.
    Alpha,
    /// Numeric shift.
    Numeric,
    /// Digits only.
    DigitsOnly,
    /// Numeric only.
    NumericOnly,
    /// Signed numeric.
    Signed,
    /// Katakana.
    Katakana,
}

/// `CASE`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Case {
    /// Keep as typed.
    #[default]
    Mixed,
    /// Fold to upper case.
    Upper,
}

/// One `:TI.` of a translation list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranslateItem {
    /// The internal value.
    pub value: String,
    /// The text shown for it.
    pub text: String,
}

/// One `:CHECK.`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Check {
    /// `TYPE`: `VALUES`, `RANGE`, `NOTBLANK`, `NOTSYS`, `NOTLIB`, ...
    pub kind: String,
    /// `MSG`: the message sent when the check fails.
    pub msg: Option<String>,
    /// `VAR`: a variable the check reads.
    pub var: Option<String>,
    /// `MSGID`: the message identifier.
    pub msgid: Option<String>,
}
