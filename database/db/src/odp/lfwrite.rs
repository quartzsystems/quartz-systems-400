// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Writes through a logical file: the logical record mapped back onto
//! the based-on record it came from. A format can be written through
//! when every field of it is a field of its one based-on file, renamed
//! or not; a join format or a derived field (`CONCAT`, `SST`) refuses.
//! Fields of the based-on record the format does not have keep their
//! value on an update and take `DFT`, else zero or blank, on a write.

use dds::{DefaultValue, FieldSource, Literal};
use qdecimal::Decimal;

use crate::error::{DbError, Result};
use crate::format::FormatDef;
use crate::lf::LogicalDef;
use crate::record::{self, Value};

fn refused(lf: &FormatDef, why: &str) -> DbError {
    DbError::Definition {
        what: format!(
            "logical format {} cannot be written through: {why}",
            lf.format.name
        ),
    }
}

/// The based-on field index each field of the logical format maps to.
pub fn mapping(lf: &FormatDef, log: &LogicalDef, base: &FormatDef) -> Result<Vec<usize>> {
    if log.join {
        return Err(refused(lf, "it is a join format"));
    }
    lf.format
        .fields
        .iter()
        .enumerate()
        .map(|(i, f)| match log.sources.get(i) {
            Some(FieldSource::Base { file: 0, field }) => base
                .format
                .field(field)
                .map(|(bi, _)| bi)
                .ok_or_else(|| DbError::Corrupt {
                    what: format!("based-on field {field} missing from {}", base.format.name),
                }),
            Some(FieldSource::Base { .. }) => Err(refused(lf, "it has fields of a secondary file")),
            _ => Err(refused(lf, &format!("field {} is derived", f.name))),
        })
        .collect()
}

/// A record of `base` holding what a new record gets when nothing is
/// written to a field: `DFT`, else zero or blank.
pub fn default_record(base: &FormatDef) -> Result<(Vec<u8>, Vec<u8>)> {
    let mut out = vec![0u8; usize::try_from(base.format.record_len).expect("fits")];
    let mut nulls = vec![0u8; usize::from(base.null_map_len)];
    for (i, f) in base.format.fields.iter().enumerate() {
        let v = match &f.default {
            Some(DefaultValue::Null) => Value::Null,
            Some(DefaultValue::Quoted(q)) => record::literal_value(f, &Literal::Quoted(q.clone()))?,
            Some(DefaultValue::Hex(h)) => Value::Char(h.clone()),
            Some(DefaultValue::Number(n)) => record::literal_value(f, &Literal::Word(n.clone()))?,
            Some(DefaultValue::Expr(_)) | None if f.data_type.is_numeric() => {
                Value::Num(Decimal::zero())
            }
            Some(DefaultValue::Expr(_)) | None => Value::Char(Vec::new()),
        };
        if v == Value::Null
            && let Some(ni) = base.null_index(i)
        {
            record::set_null(&mut nulls, ni, true);
        }
        record::write_field(f, &mut out, &v)?;
    }
    Ok((out, nulls))
}

/// The based-on record a logical record writes: `start` (the record
/// being updated) or the defaults, with the mapped fields replaced.
pub fn to_base(
    lf: &FormatDef,
    log: &LogicalDef,
    base: &FormatDef,
    data: &[u8],
    null_map: &[u8],
    start: Option<(&[u8], &[u8])>,
) -> Result<(Vec<u8>, Vec<u8>)> {
    let map = mapping(lf, log, base)?;
    let (mut out, mut nulls) = match start {
        Some((r, n)) => (r.to_vec(), n.to_vec()),
        None => default_record(base)?,
    };
    for (i, f) in lf.format.fields.iter().enumerate() {
        let bf = &base.format.fields[map[i]];
        let v = record::read_field(f, data, null_map, lf.null_index(i))?;
        match (&v, base.null_index(map[i])) {
            (Value::Null, Some(ni)) => record::set_null(&mut nulls, ni, true),
            (Value::Null, None) => {
                return Err(DbError::DataMapping {
                    field: f.name.to_string(),
                    what: "null into a based-on field that is not null-capable".into(),
                });
            }
            (_, Some(ni)) => record::set_null(&mut nulls, ni, false),
            (_, None) => {}
        }
        record::write_field(bf, &mut out, &v)?;
    }
    Ok((out, nulls))
}
