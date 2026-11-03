// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CPYFRMIMPF`: text into a member.

use dds::{DataType, FieldDef};
use qdecimal::Decimal;
use qsname::MemberSpec;
use store::obj::{JobHandle, LockState, ObjectId};
use store::txn::Txn;

use super::{DtaFmt, ImpfOpts, ImpfReport, csv, fixed_width};
use crate::error::{DbError, Result};
use crate::file;
use crate::format::FormatDef;
use crate::member::{Member, lob};
use crate::record::datetime::{Date, Time};
use crate::record::{self, Value};
use crate::util::{Hold, member_of, physical};

/// Add the records in `text` to the member. A record that does not
/// convert is skipped and counted against `ERRLVL`.
pub fn cpyfrmimpf(
    txn: &mut Txn<'_>,
    job: JobHandle,
    file: ObjectId,
    member: MemberSpec,
    text: &[u8],
    opts: &ImpfOpts,
) -> Result<ImpfReport> {
    crate::auth::check(txn, "file.open-output", file)?;
    let desc = file::describe(txn, file)?;
    physical(&desc, "CPYFRMIMPF")?;
    let entry = member_of(&desc, member)?;
    let hold = Hold::take(
        txn,
        job,
        &[
            (file, LockState::SharedUpdate),
            (entry.object, LockState::SharedUpdate),
        ],
        &desc.name().to_string(),
    )?;
    let text = String::from_utf8_lossy(text);
    let result = Member::open(txn, &desc, &entry).and_then(|mut m| {
        m.jrn.job = job;
        load(txn, &mut m, &text, opts)
    });
    hold.release(txn);
    result
}

fn load(txn: &mut Txn<'_>, m: &mut Member, text: &str, opts: &ImpfOpts) -> Result<ImpfReport> {
    let fmt = m.format().clone();
    let mut report = ImpfReport::default();
    let mut record = vec![0u8; usize::try_from(fmt.format.record_len).expect("fits")];
    let mut nulls = vec![0u8; usize::from(fmt.null_map_len)];
    for (i, line) in csv::records(text, opts.rcddlm).iter().enumerate() {
        let number = i as u64 + 1;
        let fields = match opts.dtafmt {
            DtaFmt::Csv => csv::split(line, opts.flddlm, opts.strdlm),
            DtaFmt::Fixed => fixed_fields(&fmt, line),
        };
        // A LOB field's value goes in the member's LOB tree first; a
        // record that then fails frees what it put.
        let id = m.id();
        let mut put: Vec<Vec<u8>> = Vec::new();
        let converted = convert(&fmt, &fields, opts, &mut record, &mut nulls, &mut |data| {
            let r = lob::put(txn, id, data)?;
            put.push(r.clone());
            Ok(r)
        });
        let outcome = converted.and_then(|()| m.write_new(txn, &record, &nulls).map(|_| ()));
        if outcome.is_err() {
            for r in &put {
                lob::free(txn, id, &lob::LobRef::decode(r)?)?;
            }
        }
        match outcome {
            Ok(()) => report.records += 1,
            Err(e @ (DbError::DataMapping { .. } | DbError::DuplicateKey { .. })) => {
                report.errors += 1;
                report.skipped.push((number, e.to_string()));
                if !opts.errlvl.allows(report.errors) {
                    return Err(DbError::Copy {
                        what: format!("record {number}: {e}"),
                    });
                }
            }
            Err(e) => return Err(e),
        }
    }
    Ok(report)
}

/// Cut a `*FIXED` record at the field widths.
fn fixed_fields(fmt: &FormatDef, line: &str) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut at = 0;
    fmt.format
        .fields
        .iter()
        .map(|f| {
            let w = fixed_width(f);
            let end = (at + w).min(chars.len());
            let s: String = chars.get(at..end).unwrap_or(&[]).iter().collect();
            at = end;
            s
        })
        .collect()
}

/// Fill the record from the texts, one per field.
fn convert(
    fmt: &FormatDef,
    fields: &[String],
    opts: &ImpfOpts,
    record: &mut [u8],
    nulls: &mut [u8],
    lob_put: &mut dyn FnMut(&[u8]) -> Result<Vec<u8>>,
) -> Result<()> {
    nulls.fill(0);
    record.fill(0);
    for (i, f) in fmt.format.fields.iter().enumerate() {
        let text = fields.get(i).ok_or_else(|| DbError::DataMapping {
            field: f.name.to_string(),
            what: "no field in the record".into(),
        })?;
        let text = opts.rmvblank.apply(text);
        let (value, null) = if text.trim().is_empty() && f.allow_null {
            (Value::Null, true)
        } else if text.trim().is_empty() && f.data_type.is_numeric() {
            (Value::Num(Decimal::zero()), false)
        } else {
            (field_value(f, text, opts)?, false)
        };
        let value = match value {
            Value::Char(data) if f.data_type.is_lob() => {
                let units = if f.data_type == DataType::Dbclob { data.len() / 2 } else { data.len() };
                if units > usize::try_from(f.length).expect("fits") {
                    return Err(bad(f, text));
                }
                Value::Char(lob_put(&data)?)
            }
            v => v,
        };
        record::write_field(f, record, &value)?;
        if let Some(ni) = fmt.null_index(i) {
            record::set_null(nulls, ni, null);
        }
    }
    Ok(())
}

fn bad(f: &FieldDef, text: &str) -> DbError {
    DbError::DataMapping {
        field: f.name.to_string(),
        what: format!("cannot convert '{text}'"),
    }
}

/// The text as the field's value.
pub(super) fn field_value(f: &FieldDef, text: &str, opts: &ImpfOpts) -> Result<Value> {
    // A float field also takes exponent notation.
    if f.data_type == DataType::Float {
        let t = text.trim().replace(opts.decpnt, ".");
        return t.parse::<f64>().map(Value::Float).map_err(|_| bad(f, text));
    }
    if f.data_type.is_numeric() {
        let t = text.trim().replace(opts.decpnt, ".");
        return Decimal::parse(&t).map(Value::Num).map_err(|_| bad(f, text));
    }
    match f.data_type {
        DataType::DecFloat => {
            let t = text.trim().replace(opts.decpnt, ".");
            return qdecimal::DecFloat::parse(&t, record::decfloat_format(f))
                .map(Value::DecFloat)
                .map_err(|_| bad(f, text));
        }
        DataType::Bytes | DataType::Blob => {
            let t = text.trim();
            if !t.len().is_multiple_of(2) {
                return Err(bad(f, text));
            }
            return (0..t.len())
                .step_by(2)
                .map(|i| t.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok()))
                .collect::<Option<Vec<u8>>>()
                .map(Value::Char)
                .ok_or_else(|| bad(f, text));
        }
        DataType::Boolean => {
            return match text.trim().to_ascii_uppercase().as_str() {
                "TRUE" | "1" | "T" | "Y" => Ok(Value::Char(vec![0xF1])),
                "FALSE" | "0" | "F" | "N" => Ok(Value::Char(vec![0xF0])),
                _ => Err(bad(f, text)),
            };
        }
        _ => {}
    }
    let s = match f.data_type {
        DataType::Date => {
            let own = f.date_fmt.unwrap_or_default();
            let d = Date::parse(text, opts.datfmt.unwrap_or(own)).ok_or_else(|| bad(f, text))?;
            d.render(own, f.date_sep)
        }
        DataType::Time => {
            let own = f.time_fmt.unwrap_or_default();
            let t = Time::parse(text, opts.timfmt.unwrap_or(own)).ok_or_else(|| bad(f, text))?;
            t.stored(own, f.time_sep)
        }
        // A VARLEN field keeps no trailing blanks: its length is the data.
        _ if f.varlen.is_some() => text.trim_end().to_string(),
        _ => text.to_string(),
    };
    if record::is_utf16(f) || f.data_type == DataType::Dbclob {
        return Ok(Value::Char(s.encode_utf16().flat_map(u16::to_be_bytes).collect()));
    }
    ccsid::to_ebcdic(&s)
        .map(Value::Char)
        .map_err(|_| bad(f, text))
}
