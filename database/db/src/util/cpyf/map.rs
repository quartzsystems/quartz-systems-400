// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `FMTOPT`: how a from-record becomes a to-record.

use dds::DefaultValue;
use qdecimal::Decimal;

use super::FmtOpt;
use crate::error::{DbError, Result};
use crate::file::FileKind;
use crate::format::FormatDef;
use crate::member::Slot;
use crate::record::{self, Value};

/// One to-field: taken from a from-field or defaulted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    /// To-field index.
    pub to: usize,
    /// From-field index, `None` for the default.
    pub from: Option<usize>,
}

/// The conversion decided once per copy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Plan {
    /// Same format: record and null map copied.
    Same,
    /// `*NOCHK`: bytes cut or blank padded, no nulls.
    Bytes,
    /// Field by field.
    Fields(Vec<Step>),
    /// `*CVTSRC` from a source member: `SRCDTA` is the record.
    SrcToData,
    /// `*CVTSRC` into a source member: the record is `SRCDTA`.
    DataToSrc,
}

/// Decide the plan, or refuse the copy.
pub fn plan(
    from: &FormatDef,
    to: &FormatDef,
    from_kind: FileKind,
    to_kind: FileKind,
    opt: FmtOpt,
) -> Result<Plan> {
    let same = from.level_id == to.level_id && from.null_map_len == to.null_map_len;
    match opt {
        FmtOpt::None if same => Ok(Plan::Same),
        FmtOpt::None if from_kind == FileKind::Source && to_kind == FileKind::Source => {
            fields(from, to, true, true)
        }
        FmtOpt::None => Err(DbError::Copy {
            what: format!(
                "record formats {} and {} differ; FMTOPT(*NOCHK), *MAP or *DROP is needed",
                from.format.name, to.format.name
            ),
        }),
        FmtOpt::NoChk => Ok(Plan::Bytes),
        FmtOpt::Map => fields(from, to, true, false),
        FmtOpt::Drop => fields(from, to, false, true),
        FmtOpt::MapDrop => fields(from, to, true, true),
        FmtOpt::CvtSrc => match (from_kind, to_kind) {
            (FileKind::Source, FileKind::Physical) => Ok(Plan::SrcToData),
            (FileKind::Physical, FileKind::Source) => Ok(Plan::DataToSrc),
            _ => Err(DbError::Copy {
                what: "FMTOPT(*CVTSRC) needs one source member and one data member".into(),
            }),
        },
    }
}

/// Fields by name; `map` allows defaults for missing to-fields, `drop`
/// allows from-fields with no home.
fn fields(from: &FormatDef, to: &FormatDef, map: bool, drop: bool) -> Result<Plan> {
    let mut steps = Vec::new();
    for (ti, tf) in to.format.fields.iter().enumerate() {
        let found = from.format.field(&tf.name).map(|(fi, _)| fi);
        if found.is_none() && !map {
            return Err(DbError::Copy {
                what: format!("field {} of the to-file is not in the from-file", tf.name),
            });
        }
        steps.push(Step {
            to: ti,
            from: found,
        });
    }
    if !drop
        && let Some(f) = from
            .format
            .fields
            .iter()
            .find(|f| to.format.field(&f.name).is_none())
    {
        return Err(DbError::Copy {
            what: format!(
                "field {} of the from-file is not in the to-file; FMTOPT(*DROP) drops it",
                f.name
            ),
        });
    }
    Ok(Plan::Fields(steps))
}

/// A field's `DFT`, with whether it is the null value.
fn default_of(f: &dds::FieldDef) -> Result<(Value, bool)> {
    Ok(match &f.default {
        Some(DefaultValue::Null) => (Value::Null, f.allow_null),
        Some(DefaultValue::Quoted(q)) => (
            Value::Char(
                q.chars()
                    .map(|c| ccsid::byte_of(c).unwrap_or(ccsid::SPACE))
                    .collect(),
            ),
            false,
        ),
        Some(DefaultValue::Hex(h)) => (Value::Char(h.clone()), false),
        Some(DefaultValue::Number(n)) => (
            Value::Num(Decimal::parse(n).map_err(|e| DbError::DataMapping {
                field: f.name.to_string(),
                what: e.to_string(),
            })?),
            false,
        ),
        // An expression default is SQL's to evaluate: the type's zero.
        Some(DefaultValue::Expr(_)) | None => (Value::Null, false),
    })
}

impl Plan {
    /// Fill `record` and `nulls` (sized for the to-format) from `slot`;
    /// `seq` numbers source records.
    pub fn apply(
        &self,
        from: &FormatDef,
        to: &FormatDef,
        slot: &Slot,
        seq: u64,
        record: &mut [u8],
        nulls: &mut [u8],
    ) -> Result<()> {
        nulls.fill(0);
        match self {
            Self::Same => {
                record.copy_from_slice(&slot.record);
                nulls.copy_from_slice(&slot.null_map);
            }
            Self::Bytes => {
                let n = record.len().min(slot.record.len());
                record[..n].copy_from_slice(&slot.record[..n]);
                record[n..].fill(ccsid::SPACE);
            }
            Self::Fields(steps) => {
                record.fill(0);
                for s in steps {
                    let tf = &to.format.fields[s.to];
                    let (v, null) = match s.from {
                        Some(fi) => {
                            let ff = &from.format.fields[fi];
                            let v = record::read_field(
                                ff,
                                &slot.record,
                                &slot.null_map,
                                from.null_index(fi),
                            )?;
                            let null = v == Value::Null && tf.allow_null;
                            (v, null)
                        }
                        None => default_of(tf)?,
                    };
                    record::write_field(tf, record, &v)?;
                    if let Some(ni) = to.null_index(s.to) {
                        record::set_null(nulls, ni, null);
                    }
                }
            }
            Self::SrcToData => {
                let data = record::field_bytes(&from.format.fields[2], &slot.record);
                let n = record.len().min(data.len());
                record[..n].copy_from_slice(&data[..n]);
                record[n..].fill(ccsid::SPACE);
            }
            Self::DataToSrc => {
                let f = &to.format.fields;
                let seq = Decimal::new(i128::from(seq), 0).map_err(|e| DbError::DataMapping {
                    field: "SRCSEQ".into(),
                    what: e.to_string(),
                })?;
                record::write_field(&f[0], record, &Value::Num(seq))?;
                record::write_field(&f[1], record, &Value::Num(Decimal::zero()))?;
                record::write_field(&f[2], record, &Value::Char(slot.record.clone()))?;
            }
        }
        Ok(())
    }
}
