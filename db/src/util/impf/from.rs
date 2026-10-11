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
use crate::member::Member;
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
    let result = Member::open(txn, &desc, &entry).and_then(|mut m| load(txn, &mut m, &text, opts));
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
        let outcome = convert(&fmt, &fields, opts, &mut record, &mut nulls)
            .and_then(|()| m.write_new(txn, &record, &nulls).map(|_| ()));
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
fn field_value(f: &FieldDef, text: &str, opts: &ImpfOpts) -> Result<Value> {
    if f.data_type.is_numeric() {
        let t = text.trim().replace(opts.decpnt, ".");
        return Decimal::parse(&t).map(Value::Num).map_err(|_| bad(f, text));
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
            t.render(own, f.time_sep)
        }
        // A VARLEN field keeps no trailing blanks: its length is the data.
        _ if f.varlen.is_some() => text.trim_end().to_string(),
        _ => text.to_string(),
    };
    ccsid::to_ebcdic(&s)
        .map(Value::Char)
        .map_err(|_| bad(f, text))
}
