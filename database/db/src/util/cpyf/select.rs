// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `INCCHAR` and `INCREL`: which records a copy takes.

use std::cmp::Ordering;

use qsname::Name;

use crate::error::{DbError, Result};
use crate::format::FormatDef;
use crate::member::Slot;
use crate::record::{self, Value};

/// A comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompareOp {
    /// `*EQ`.
    Eq,
    /// `*NE`.
    Ne,
    /// `*GT`.
    Gt,
    /// `*LT`.
    Lt,
    /// `*GE`.
    Ge,
    /// `*LE`.
    Le,
    /// `*CT`: contains (`INCCHAR` only).
    Ct,
}

impl CompareOp {
    /// Parse `*EQ` and the rest.
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        Some(match word.to_ascii_uppercase().as_str() {
            "*EQ" => Self::Eq,
            "*NE" => Self::Ne,
            "*GT" => Self::Gt,
            "*LT" => Self::Lt,
            "*GE" => Self::Ge,
            "*LE" => Self::Le,
            "*CT" => Self::Ct,
            _ => return None,
        })
    }

    fn holds(self, o: Ordering) -> bool {
        match self {
            Self::Eq => o == Ordering::Equal,
            Self::Ne => o != Ordering::Equal,
            Self::Gt => o == Ordering::Greater,
            Self::Lt => o == Ordering::Less,
            Self::Ge => o != Ordering::Less,
            Self::Le => o != Ordering::Greater,
            Self::Ct => false,
        }
    }
}

/// `INCCHAR(field position op 'value')`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IncChar {
    /// The field, `None` for `*RCD`.
    pub field: Option<Name>,
    /// 1-based position within the field or record.
    pub position: u32,
    /// The comparison.
    pub op: CompareOp,
    /// The value, CP037.
    pub value: Vec<u8>,
}

impl IncChar {
    /// The field exists and the position lies within it.
    pub fn check(&self, fmt: &FormatDef) -> Result<()> {
        let len = match self.field {
            None => fmt.format.record_len,
            Some(n) => {
                fmt.format
                    .field(&n)
                    .ok_or_else(|| DbError::Copy {
                        what: format!("INCCHAR: no field {n} in format {}", fmt.format.name),
                    })?
                    .1
                    .byte_len
            }
        };
        if self.position == 0 || self.position > len {
            return Err(DbError::Copy {
                what: format!("INCCHAR: position {} is outside the field", self.position),
            });
        }
        Ok(())
    }

    fn passes(&self, fmt: &FormatDef, slot: &Slot) -> bool {
        let bytes: &[u8] = match self.field {
            None => &slot.record,
            Some(n) => {
                let (_, f) = fmt.format.field(&n).expect("checked");
                record::field_bytes(f, &slot.record)
            }
        };
        let start = usize::try_from(self.position - 1).expect("fits");
        let Some(rest) = bytes.get(start..) else {
            return false;
        };
        if self.op == CompareOp::Ct {
            return rest
                .windows(self.value.len())
                .any(|w| w == self.value.as_slice());
        }
        let end = self.value.len().min(rest.len());
        self.op.holds(ccsid::ebcdic_cmp(&rest[..end], &self.value))
    }
}

/// `*IF`, `*AND`, `*OR`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conj {
    /// The first relation.
    If,
    /// Joins the relation to the current group.
    And,
    /// Starts a new group.
    Or,
}

/// One `INCREL` relation: `(conj field op value)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IncRel {
    /// The conjunction.
    pub conj: Conj,
    /// The field.
    pub field: Name,
    /// The comparison.
    pub op: CompareOp,
    /// The value as typed: a number for numeric fields, text otherwise,
    /// `*NULL` for the null test.
    pub value: String,
}

impl IncRel {
    /// The field exists and the value suits it.
    pub fn check(&self, fmt: &FormatDef) -> Result<()> {
        let (_, f) = fmt.format.field(&self.field).ok_or_else(|| DbError::Copy {
            what: format!(
                "INCREL: no field {} in format {}",
                self.field, fmt.format.name
            ),
        })?;
        if self.op == CompareOp::Ct {
            return Err(DbError::Copy {
                what: "INCREL: *CT is not a relation".into(),
            });
        }
        if self.value != "*NULL" && f.data_type.is_numeric() {
            qdecimal::Decimal::parse(&self.value).map_err(|e| DbError::Copy {
                what: format!("INCREL: {}: {e}", self.field),
            })?;
        }
        Ok(())
    }

    fn holds(&self, fmt: &FormatDef, slot: &Slot) -> Result<bool> {
        let (i, f) = fmt.format.field(&self.field).expect("checked");
        let v = record::read_field(f, &slot.record, &slot.null_map, fmt.null_index(i))?;
        if self.value == "*NULL" {
            let is_null = v == Value::Null;
            return Ok(match self.op {
                CompareOp::Eq => is_null,
                CompareOp::Ne => !is_null,
                _ => false,
            });
        }
        if v == Value::Null {
            return Ok(false);
        }
        let want = if f.data_type.is_numeric() {
            Value::Num(qdecimal::Decimal::parse(&self.value).expect("checked"))
        } else {
            Value::Char(
                self.value
                    .chars()
                    .map(|c| ccsid::byte_of(c).unwrap_or(ccsid::SPACE))
                    .collect(),
            )
        };
        Ok(self.op.holds(record::compare(&v, &want)))
    }
}

/// Whether a record passes `INCCHAR` and `INCREL`: the relations form
/// `*AND` groups separated by `*OR`.
pub fn passes(
    fmt: &FormatDef,
    slot: &Slot,
    incchar: Option<&IncChar>,
    increl: &[IncRel],
) -> Result<bool> {
    if let Some(ic) = incchar
        && !ic.passes(fmt, slot)
    {
        return Ok(false);
    }
    if increl.is_empty() {
        return Ok(true);
    }
    let mut group = true;
    for r in increl {
        if r.conj == Conj::Or {
            if group {
                return Ok(true);
            }
            group = true;
        }
        if group {
            group = r.holds(fmt, slot)?;
        }
    }
    Ok(group)
}
