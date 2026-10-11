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

/// Derive the logical record from a based-on record of file 0 of the
/// format. Fields of other files of a join format are blank or zero.
pub fn derive(
    lf: &FormatDef,
    log: &LogicalDef,
    base: &FormatDef,
    record: &[u8],
    null_map: &[u8],
) -> Result<(Vec<u8>, Vec<u8>)> {
    let mut out = vec![0u8; usize::try_from(lf.format.record_len).expect("fits")];
    let mut nulls = vec![0u8; usize::from(lf.null_map_len)];
    for (i, f) in lf.format.fields.iter().enumerate() {
        let source = log.sources.get(i).ok_or_else(|| DbError::Corrupt {
            what: format!("field {} of {} has no source", f.name, lf.format.name),
        })?;
        let value = match source {
            FieldSource::Base { file, field } => {
                if *file != 0 {
                    Value::Char(Vec::new())
                } else {
                    let (bi, bf) = base_field(base, field)?;
                    record::read_field(bf, record, null_map, base.null_index(bi))?
                }
            }
            FieldSource::Concat { file, fields } if *file == 0 => {
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
            FieldSource::Substring {
                file,
                field,
                start,
                length,
            } if *file == 0 => {
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
            FieldSource::Concat { .. } | FieldSource::Substring { .. } => Value::Char(Vec::new()),
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
