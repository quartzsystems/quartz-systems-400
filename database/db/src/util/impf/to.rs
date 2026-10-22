// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CPYTOIMPF`: a member as text.

use dds::{DataType, FieldDef};
use qsname::MemberSpec;
use store::obj::{JobHandle, LockState, ObjectId};
use store::txn::Txn;

use super::{DtaFmt, ImpfOpts, csv, fixed_width, is_text};
use crate::error::Result;
use crate::file;
use crate::format::FormatDef;
use crate::member::{Member, Slot};
use crate::record::datetime::{Date, Time};
use crate::record::{self, Value};
use crate::util::{Hold, Order, each_record, member_of};

/// Write the member's records as UTF-8 text, in key order when the
/// member is keyed.
pub fn cpytoimpf(
    txn: &mut Txn<'_>,
    job: JobHandle,
    file: ObjectId,
    member: MemberSpec,
    opts: &ImpfOpts,
) -> Result<Vec<u8>> {
    crate::auth::check(txn, "file.open-input", file)?;
    let desc = file::describe(txn, file)?;
    let entry = member_of(&desc, member)?;
    let hold = Hold::take(
        txn,
        job,
        &[
            (file, LockState::SharedRead),
            (entry.object, LockState::SharedRead),
        ],
        &desc.name().to_string(),
    )?;
    let result = Member::open(txn, &desc, &entry).and_then(|mut m| {
        let fmt = m.format().clone();
        let order = Order {
            keyed: fmt.key_len > 0,
            ..Order::default()
        };
        let mut out = String::new();
        each_record(txn, &mut m, &order, &mut |_, slot| {
            out.push_str(&render(&fmt, &slot, opts)?);
            out.push_str(opts.rcddlm.text());
            Ok(true)
        })?;
        Ok(out.into_bytes())
    });
    hold.release(txn);
    result
}

/// One record as text, without the record delimiter.
pub fn render(fmt: &FormatDef, slot: &Slot, opts: &ImpfOpts) -> Result<String> {
    let mut out = String::new();
    for (i, f) in fmt.format.fields.iter().enumerate() {
        let null = record::is_null(&slot.null_map, fmt.null_index(i));
        let v = record::read_field(f, &slot.record, &[], None)?;
        let text = if null && opts.nulls {
            String::new()
        } else {
            field_text(f, &v, opts)
        };
        match opts.dtafmt {
            DtaFmt::Csv => {
                if i > 0 {
                    out.push(opts.flddlm);
                }
                if is_text(f) && !(null && opts.nulls) {
                    out.push_str(&csv::quote(&text, opts.strdlm));
                } else {
                    out.push_str(&text);
                }
            }
            DtaFmt::Fixed => {
                let w = fixed_width(f);
                let n = text.chars().count();
                if is_text(f) {
                    out.push_str(&text);
                    out.extend(std::iter::repeat_n(' ', w.saturating_sub(n)));
                } else {
                    out.extend(std::iter::repeat_n(' ', w.saturating_sub(n)));
                    out.push_str(&text);
                }
            }
        }
    }
    Ok(out)
}

/// A value as the text of its field.
fn field_text(f: &FieldDef, v: &Value, opts: &ImpfOpts) -> String {
    match v {
        Value::Null => String::new(),
        Value::Num(d) => d.to_string().replace('.', &opts.decpnt.to_string()),
        Value::Char(bytes) => {
            let s = ccsid::from_ebcdic(bytes);
            match f.data_type {
                DataType::Date => {
                    let own = f.date_fmt.unwrap_or_default();
                    match (opts.datfmt, Date::parse(&s, own)) {
                        (Some(want), Some(date)) => date.render(want, opts.datsep),
                        _ => s.trim_end().to_string(),
                    }
                }
                DataType::Time => {
                    let own = f.time_fmt.unwrap_or_default();
                    match (opts.timfmt, Time::parse(&s, own)) {
                        (Some(want), Some(time)) => time.render(want, opts.timsep),
                        _ => s.trim_end().to_string(),
                    }
                }
                DataType::Timestamp => s.trim_end().to_string(),
                _ => opts.rmvblank.apply(&s).to_string(),
            }
        }
    }
}
