// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Logical formats: how a record of a based-on member becomes a record
//! of the logical format, and whether select/omit keeps it. The
//! section encoding is in `format::sections`.

use dds::{CompOp, FieldSource, SelectOmit, Validity};
use qsname::Name;
use store::obj::ObjectId;

use crate::error::{DbError, Result};
use crate::format::FormatDef;
use crate::record::{self, Value};

/// One `J` specification, by field names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JoinDef {
    /// Based-on file index of the from-file.
    pub from: u8,
    /// Based-on file index of the to-file.
    pub to: u8,
    /// `JFLD` pairs.
    pub fields: Vec<(Name, Name)>,
    /// `JDUPSEQ` field and whether it descends.
    pub dupseq: Option<(Name, bool)>,
}

/// What makes a format logical.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogicalDef {
    /// Based-on physical files with the format used, in `PFILE`/`JFILE` order.
    pub based_on: Vec<(ObjectId, Name)>,
    /// One source per field.
    pub sources: Vec<FieldSource>,
    /// Select/omit groups in order.
    pub select_omit: Vec<SelectOmit>,
    /// `J` specifications of a join format.
    pub joins: Vec<JoinDef>,
    /// `JFILE` rather than `PFILE`.
    pub join: bool,
}

fn base_field<'a>(base: &'a FormatDef, name: &Name) -> Result<(usize, &'a dds::FieldDef)> {
    base.format.field(name).ok_or_else(|| DbError::Corrupt {
        what: format!("based-on field {name} missing from {}", base.format.name),
    })
}

/// A based-on record: its format, record bytes and null map.
pub type BaseRecord<'a> = (&'a FormatDef, &'a [u8], &'a [u8]);

/// What a field takes from a secondary record that was not found
/// (`JDFTVAL`): zero or blank.
fn absent(f: &dds::FieldDef) -> Value {
    if f.data_type.is_numeric() {
        Value::Num(qdecimal::Decimal::zero())
    } else {
        Value::Char(Vec::new())
    }
}

/// Whether select/omit tests a field that comes from a secondary file,
/// which a path built from the primary records alone cannot decide:
/// the test then runs at read time.
#[must_use]
pub fn read_time_select(lf: &FormatDef, log: &LogicalDef) -> bool {
    log.select_omit.iter().any(|g| {
        g.conditions.iter().any(|c| {
            lf.format
                .field(&c.field)
                .and_then(|(i, _)| log.sources.get(i))
                .is_some_and(|s| match s {
                    FieldSource::Base { file, .. }
                    | FieldSource::Concat { file, .. }
                    | FieldSource::Substring { file, .. } => *file != 0,
                })
        })
    })
}

/// Derive the logical record from a based-on record of file 0 of the
/// format. Fields of other files of a join format are blank or zero.
pub fn derive(
    lf: &FormatDef,
    log: &LogicalDef,
    base: &FormatDef,
    record: &[u8],
    null_map: &[u8],
) -> Result<(Vec<u8>, Vec<u8>)> {
    derive_join(lf, log, &[Some((base, record, null_map))])
}

/// Derive the logical record from the based-on records by file index.
/// A file without a record (`JDFTVAL`) gives blank or zero fields.
pub fn derive_join(
    lf: &FormatDef,
    log: &LogicalDef,
    bases: &[Option<BaseRecord<'_>>],
) -> Result<(Vec<u8>, Vec<u8>)> {
    let mut out = vec![0u8; usize::try_from(lf.format.record_len).expect("fits")];
    let mut nulls = vec![0u8; usize::from(lf.null_map_len)];
    let at = |file: usize| bases.get(file).copied().flatten();
    for (i, f) in lf.format.fields.iter().enumerate() {
        let source = log.sources.get(i).ok_or_else(|| DbError::Corrupt {
            what: format!("field {} of {} has no source", f.name, lf.format.name),
        })?;
        let value = match source {
            FieldSource::Base { file, field } => match at(*file) {
                Some((base, record, null_map)) => {
                    let (bi, bf) = base_field(base, field)?;
                    record::read_field(bf, record, null_map, base.null_index(bi))?
                }
                None => absent(f),
            },
            FieldSource::Concat { file, fields } => match at(*file) {
                Some((base, record, null_map)) => {
                    let mut bytes = Vec::new();
                    let last = fields.len() - 1;
                    for (n, name) in fields.iter().enumerate() {
                        let (bi, bf) = base_field(base, name)?;
                        if record::is_null(null_map, base.null_index(bi)) {
                            bytes.clear();
                            break;
                        }
                        let mut part = record::field_bytes(bf, record).to_vec();
                        if bf.data_type == dds::DataType::Zoned && n != last {
                            // Interior parts carry no sign: zone F throughout.
                            for b in &mut part {
                                *b |= 0xF0;
                            }
                        }
                        bytes.extend(part);
                    }
                    if bytes.is_empty() {
                        Value::Null
                    } else {
                        Value::Char(bytes)
                    }
                }
                None => absent(f),
            },
            FieldSource::Substring {
                file,
                field,
                start,
                length,
            } => match at(*file) {
                Some((base, record, null_map)) => {
                    let (bi, bf) = base_field(base, field)?;
                    if record::is_null(null_map, base.null_index(bi)) {
                        Value::Null
                    } else {
                        let data = record::field_bytes(bf, record);
                        let from = usize::try_from(*start - 1).expect("fits");
                        let to = (from + usize::try_from(*length).expect("fits")).min(data.len());
                        Value::Char(data[from.min(to)..to].to_vec())
                    }
                }
                None => absent(f),
            },
        };
        if value == Value::Null {
            match lf.null_index(i) {
                Some(ni) => record::set_null(&mut nulls, ni, true),
                None => {
                    return Err(DbError::DataMapping {
                        field: f.name.to_string(),
                        what: "null from the based-on record into a field that is not null-capable"
                            .into(),
                    });
                }
            }
        }
        record::write_field(f, &mut out, &value)?;
    }
    Ok((out, nulls))
}

/// Whether every `JFLD` pair of `j` matches between the from-record and
/// the to-record: numbers by value, text without trailing blanks, a null
/// never.
pub fn join_matches(join: &JoinDef, from: BaseRecord<'_>, to: BaseRecord<'_>) -> Result<bool> {
    for (from_name, to_name) in &join.fields {
        let (from_index, from_field) = base_field(from.0, from_name)?;
        let (to_index, to_field) = base_field(to.0, to_name)?;
        let left = record::read_field(from_field, from.1, from.2, from.0.null_index(from_index))?;
        let right = record::read_field(to_field, to.1, to.2, to.0.null_index(to_index))?;
        let same = match (&left, &right) {
            (Value::Num(left), Value::Num(right)) => left == right,
            (Value::Char(left), Value::Char(right)) => trim_blanks(left) == trim_blanks(right),
            _ => false,
        };
        if !same {
            return Ok(false);
        }
    }
    Ok(true)
}

fn trim_blanks(b: &[u8]) -> &[u8] {
    let end = b.iter().rposition(|&c| c != 0x40).map_or(0, |p| p + 1);
    &b[..end]
}

fn op_holds(op: CompOp, ord: std::cmp::Ordering) -> bool {
    use std::cmp::Ordering::{Equal, Greater, Less};
    match op {
        CompOp::Eq => ord == Equal,
        CompOp::Ne => ord != Equal,
        CompOp::Lt => ord == Less,
        CompOp::Gt => ord == Greater,
        CompOp::Nl | CompOp::Ge => ord != Less,
        CompOp::Ng | CompOp::Le => ord != Greater,
    }
}

/// Whether the select/omit groups keep a logical record. With no groups
/// every record is kept.
pub fn selected(lf: &FormatDef, log: &LogicalDef, record: &[u8], null_map: &[u8]) -> Result<bool> {
    let Some(last) = log.select_omit.last() else {
        return Ok(true);
    };
    for g in &log.select_omit {
        if g.all {
            return Ok(g.select);
        }
        let mut holds = true;
        for c in &g.conditions {
            let (i, f) = lf.format.field(&c.field).ok_or_else(|| DbError::Corrupt {
                what: format!(
                    "select/omit field {} missing from {}",
                    c.field, lf.format.name
                ),
            })?;
            let v = record::read_field(f, record, null_map, lf.null_index(i))?;
            let ok = match &c.test {
                Validity::Values(lits) => lits
                    .iter()
                    .map(|l| record::literal_value(f, l))
                    .collect::<Result<Vec<_>>>()?
                    .iter()
                    .any(|w| record::compare(&v, w) == std::cmp::Ordering::Equal),
                Validity::Range(lo, hi) => {
                    let lo = record::literal_value(f, lo)?;
                    let hi = record::literal_value(f, hi)?;
                    record::compare(&v, &lo) != std::cmp::Ordering::Less
                        && record::compare(&v, &hi) != std::cmp::Ordering::Greater
                }
                Validity::Comp(op, lit) => {
                    let w = record::literal_value(f, lit)?;
                    op_holds(*op, record::compare(&v, &w))
                }
                Validity::Check(_) => true,
            };
            if !ok {
                holds = false;
                break;
            }
        }
        if holds {
            return Ok(g.select);
        }
    }
    // Nothing applied: the last statement's opposite.
    Ok(!last.select)
}
