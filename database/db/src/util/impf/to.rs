// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CPYTOIMPF`: a member as text.

use std::ops::Bound;

use dds::{DataType, FieldDef};
use qsname::MemberSpec;
use store::obj::{JobHandle, LockState, ObjectId};
use store::txn::Txn;

use super::{DtaFmt, ImpfOpts, csv, fixed_width, is_text};
use crate::error::Result;
use crate::file;
use crate::format::FormatDef;
use crate::member::{Member, Slot};
use crate::path::PathKey;
use crate::record::datetime::{Date, Time};
use crate::record::{self, Value};
use crate::util::{Hold, Order, each_record, member_of};

/// Write the member's records as UTF-8 text in arrival sequence, as the
/// machine does even for a keyed file (the pub400 capture); a join
/// logical file in the order [`join_order`] gives.
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
        let mut out = String::new();
        if let Some(rrns) = join_order(txn, &m)? {
            for rrn in rrns {
                let entry = PathKey {
                    key: Vec::new(),
                    format: 0,
                    rrn,
                };
                for slot in m.read_entries(txn, &entry)? {
                    out.push_str(&render(&fmt, &slot, opts)?);
                    out.push_str(opts.rcddlm.text());
                }
            }
            return Ok(out.into_bytes());
        }
        each_record(txn, &mut m, &Order::default(), &mut |_, slot| {
            out.push_str(&render(&fmt, &slot, opts)?);
            out.push_str(opts.rcddlm.text());
            Ok(true)
        })?;
        Ok(out.into_bytes())
    });
    hold.release(txn);
    result
}

/// A join logical file has no arrival sequence of its own: read that
/// way, the machine drives the join from the primary file through a
/// keyed path over it whose first key is the join field, when one
/// exists, else in the primary's arrival sequence. The primary's RRNs
/// in that order; `None` for any other member.
fn join_order(txn: &mut Txn<'_>, m: &Member) -> Result<Option<Vec<u64>>> {
    if !m.is_logical() {
        return Ok(None);
    }
    let def = m.logical(0)?.clone();
    if !def.join {
        return Ok(None);
    }
    let primary_member = m.bases[0].member;
    let mut rrns = Vec::new();
    let field = def
        .joins
        .iter()
        .find(|j| j.from == 0)
        .and_then(|j| j.fields.first())
        .map(|(f, _)| *f);
    if let (Some(field), Some((pf, _))) = (field, def.based_on.first()) {
        let pdesc = file::describe(txn, *pf)?;
        for d in &pdesc.dependents {
            if d.base_member != primary_member || d.lf_member == m.id() {
                continue;
            }
            let ldesc = file::describe(txn, d.lf_file)?;
            let leads = ldesc
                .formats
                .first()
                .and_then(|f| f.format.keys.first())
                .is_some_and(|k| k.name == field);
            if !leads || ldesc.formats.len() != 1 {
                continue;
            }
            let Some(path) = crate::member::Core::load(txn, d.lf_member)?.path() else {
                continue;
            };
            let mut batch = path.scan(txn, Bound::Unbounded, Bound::Unbounded, 256)?;
            while let Some(last) = batch.last().cloned() {
                for k in &batch {
                    if !rrns.contains(&k.rrn) {
                        rrns.push(k.rrn);
                    }
                }
                batch = path.scan_from(txn, Bound::Excluded(&last), 256)?;
            }
            return Ok(Some(rrns));
        }
    }
    let slots = crate::member::Core::load(txn, primary_member)?.attr.slots;
    rrns.extend(1..=slots);
    Ok(Some(rrns))
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
                // The machine writes a hex field's bytes unconverted: as
                // Latin-1 they are what `*PCASCII` shows.
                DataType::Hex => bytes.iter().map(|&b| char::from(b)).collect(),
                _ => opts.rmvblank.apply(&s).to_string(),
            }
        }
    }
}
