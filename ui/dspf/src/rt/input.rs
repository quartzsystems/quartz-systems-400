// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What came back from the display: the key, the typed values checked
//! and converted to the fields' bytes, and the response indicators.

use dds::model::{CompOp, Literal, Validity};
use qdecimal::Decimal;
use tn5250::ds::aid;

use super::edit::{self, BadValue};
use crate::model::{Field, TypeClass};

/// The key that ended the read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyPressed {
    /// Enter, or Auto Enter.
    Enter,
    /// A function key, 1 to 24.
    F(u8),
    /// Roll Up (Page Down).
    RollUp,
    /// Roll Down (Page Up).
    RollDown,
    /// Help.
    Help,
    /// Print.
    Print,
    /// Clear.
    Clear,
    /// Record Backspace.
    RecordBackspace,
    /// Something else.
    Other(u8),
}

impl KeyPressed {
    /// From the attention identifier.
    #[must_use]
    pub fn from_aid(a: u8) -> Self {
        match a {
            aid::ENTER | aid::AUTO_ENTER => Self::Enter,
            aid::ROLL_UP => Self::RollUp,
            aid::ROLL_DOWN => Self::RollDown,
            aid::HELP => Self::Help,
            aid::PRINT => Self::Print,
            aid::CLEAR => Self::Clear,
            aid::RECORD_BACKSPACE => Self::RecordBackspace,
            other => aid::function_of(other).map_or(Self::Other(other), Self::F),
        }
    }
}

/// What a read gives the program.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Input {
    /// The input buffer as the record format lays it out.
    pub record: Vec<u8>,
    /// The separate indicator area (`INDARA`): 99 bytes of `0`/`1`.
    pub indicators: Vec<u8>,
    /// The attention identifier.
    pub aid: u8,
    /// Cursor row and column, 1-based.
    pub cursor: (u16, u16),
    /// The subfile record the cursor was in (a control record's read)
    /// or the one read (`CHAIN`, `READC`), 0 when none.
    pub sfl_rrn: u32,
    /// The lowest subfile record number on the display, 0 when none.
    pub sfl_top: u32,
    /// How many records the subfile holds.
    pub sfl_count: u32,
}

/// Why a read did not complete: the display must be shown the problem
/// and read again.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Problem {
    /// The key is not enabled for the record or the file.
    #[error("the key is not allowed here")]
    KeyNotAllowed,
    /// A typed value fails the field's checks; where, and the message.
    #[error("{text}")]
    Field {
        /// Row of the field.
        row: usize,
        /// Column of the field.
        col: usize,
        /// The message for the error line.
        text: String,
    },
    /// A roll key moved a subfile page; the screen was redrawn.
    #[error("the subfile rolled")]
    Rolled,
    /// The format is not on the display.
    #[error("record format {0} is not on the display")]
    NotShown(String),
}

/// A typed value as the field's bytes.
pub fn typed_bytes(f: &Field, text: &str) -> Result<Vec<u8>, BadValue> {
    let len = f.bytes as usize;
    match f.ty.class {
        TypeClass::Zoned | TypeClass::Packed | TypeClass::Binary | TypeClass::Float => {
            let v = edit::parse_typed(f, text)?;
            edit::encode_number(f, &v)
        }
        TypeClass::Date | TypeClass::Time | TypeClass::Timestamp => {
            let t = text.trim_end();
            if !t.is_empty() && t.chars().count() != len {
                return Err(BadValue::BadDateTime);
            }
            Ok(padded(text, len))
        }
        TypeClass::Char | TypeClass::Graphic => Ok(padded(text, len)),
    }
}

/// Text in EBCDIC, blank-padded or cut to `len`.
#[must_use]
pub fn padded(text: &str, len: usize) -> Vec<u8> {
    let mut out: Vec<u8> = text
        .chars()
        .take(len)
        .map(|c| ccsid::byte_of(c).unwrap_or(0x6F))
        .collect();
    out.resize(len, ccsid::SPACE);
    out
}

/// The message for a value that does not fit.
#[must_use]
pub fn bad_value_text(e: &BadValue) -> String {
    match e {
        BadValue::NotNumeric => "Only digits, a sign and a decimal point go in this field.".into(),
        BadValue::TooLong => "The value has more digits than the field holds.".into(),
        BadValue::BadDateTime => "The date or time is not valid.".into(),
    }
}

/// The field's validity checks against the typed text: `None` when it
/// passes, else the message.
#[must_use]
pub fn validate(f: &Field, text: &str) -> Option<String> {
    let trimmed = text.trim_end();
    let check = |code: &str| f.check.iter().any(|c| c == code);
    if check("ME") && trimmed.is_empty() {
        return Some("A value is required in this field.".into());
    }
    if check("MF") && !trimmed.is_empty() && trimmed.chars().count() < f.length as usize {
        return Some("The field must be filled completely.".into());
    }
    if check("VN") && !trimmed.is_empty() && qsname::Name::parse(trimmed).is_err() {
        return Some("The value is not a valid name.".into());
    }
    if trimmed.is_empty() && f.validity.is_empty() {
        return None;
    }
    let numeric = f.ty.is_numeric();
    let num = if numeric {
        edit::parse_typed(f, text).ok()
    } else {
        None
    };
    for v in &f.validity {
        let ok = match v {
            Validity::Check(_) => true,
            Validity::Values(lits) => lits.iter().any(|l| matches(f, trimmed, num.as_ref(), l)),
            Validity::Range(lo, hi) => {
                compare(f, trimmed, num.as_ref(), lo).is_some_and(|c| c != std::cmp::Ordering::Less)
                    && compare(f, trimmed, num.as_ref(), hi)
                        .is_some_and(|c| c != std::cmp::Ordering::Greater)
            }
            Validity::Comp(op, lit) => {
                use std::cmp::Ordering::{Equal, Greater, Less};
                compare(f, trimmed, num.as_ref(), lit).is_some_and(|c| match op {
                    CompOp::Eq => c == Equal,
                    CompOp::Ne => c != Equal,
                    CompOp::Lt => c == Less,
                    CompOp::Nl | CompOp::Ge => c != Less,
                    CompOp::Gt => c == Greater,
                    CompOp::Ng | CompOp::Le => c != Greater,
                })
            }
        };
        if !ok {
            return Some(match v {
                Validity::Values(_) => "The value is not one of those allowed.".into(),
                Validity::Range(..) => "The value is outside the allowed range.".into(),
                Validity::Comp(..) => "The value fails the field's comparison check.".into(),
                Validity::Check(_) => "The value is not valid.".into(),
            });
        }
    }
    None
}

fn literal_text(l: &Literal) -> String {
    match l {
        Literal::Word(w) | Literal::Quoted(w) => w.clone(),
        Literal::Hex(h) => ccsid::from_ebcdic(h),
    }
}

fn matches(f: &Field, text: &str, num: Option<&Decimal>, l: &Literal) -> bool {
    compare(f, text, num, l) == Some(std::cmp::Ordering::Equal)
}

/// The typed value against a literal: numbers as numbers, text as
/// text with trailing blanks ignored.
fn compare(
    f: &Field,
    text: &str,
    num: Option<&Decimal>,
    literal: &Literal,
) -> Option<std::cmp::Ordering> {
    if f.ty.is_numeric() {
        let value = num?;
        let lit = Decimal::parse(&literal_text(literal)).ok()?;
        let scale = value.scale().max(lit.scale());
        let left = value.rescale(scale, qdecimal::Rounding::Truncate).ok()?;
        let right = lit.rescale(scale, qdecimal::Rounding::Truncate).ok()?;
        return Some(left.mantissa_i128()?.cmp(&right.mantissa_i128()?));
    }
    let lit = literal_text(literal);
    Some(ccsid::ebcdic_cmp_str(text.trim_end(), lit.trim_end()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FieldType, Usage};

    #[test]
    fn keys() {
        assert_eq!(KeyPressed::from_aid(aid::ENTER), KeyPressed::Enter);
        assert_eq!(KeyPressed::from_aid(0x33), KeyPressed::F(3));
        assert_eq!(KeyPressed::from_aid(0xBC), KeyPressed::F(24));
        assert_eq!(KeyPressed::from_aid(aid::ROLL_UP), KeyPressed::RollUp);
    }

    #[test]
    fn checks() {
        let mut f = Field::new(Some("ST".into()), FieldType::char(), 1, 0, Usage::Both);
        f.bytes = 1;
        f.validity.push(Validity::Values(vec![
            Literal::Quoted("A".into()),
            Literal::Quoted("I".into()),
        ]));
        assert_eq!(validate(&f, "A"), None);
        assert!(validate(&f, "X").is_some());
        f.check.push("ME".into());
        assert!(validate(&f, " ").is_some());
        let mut n = Field::new(Some("N".into()), FieldType::zoned(), 5, 0, Usage::Both);
        n.bytes = 5;
        n.validity
            .push(Validity::Range(Literal::Word("1".into()), Literal::Word("100".into())));
        assert_eq!(validate(&n, "  50"), None);
        assert!(validate(&n, "101").is_some());
        n.validity.clear();
        n.validity
            .push(Validity::Comp(CompOp::Ne, Literal::Word("0".into())));
        assert!(validate(&n, "0").is_some());
        assert_eq!(validate(&n, "7"), None);
        assert_eq!(typed_bytes(&n, "7").unwrap(), vec![0xF0, 0xF0, 0xF0, 0xF0, 0xC7]);
        assert_eq!(typed_bytes(&f, "a").unwrap(), vec![0x81]);
    }
}
