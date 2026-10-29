// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Source members: text in, text out, and a `dds::Source` to compile.

use qdecimal::{Decimal, Rounding};
use qsname::{MemberSpec, Name};
use store::obj::{JobHandle, LockState, ObjectId};
use store::txn::Txn;

use crate::error::{DbError, Result};
use crate::file::{self, FileKind};
use crate::member::Member;
use crate::record::datetime::Date;
use crate::record::{self, Value};
use crate::util::{Hold, member_of};

fn source_file(txn: &mut Txn<'_>, file: ObjectId) -> Result<file::FileDesc> {
    let desc = file::describe(txn, file)?;
    if desc.attr.kind != FileKind::Source {
        return Err(DbError::Definition {
            what: format!("{} is not a source physical file", desc.name()),
        });
    }
    Ok(desc)
}

/// `SRCDAT` for a time: `yymmdd`.
fn srcdat(now: u64) -> Decimal {
    let d = Date::from_unix(now);
    let n = i128::from(d.year % 100) * 10_000 + i128::from(d.month) * 100 + i128::from(d.day);
    Decimal::new(n, 0).expect("six digits")
}

/// Replace the member's records with the lines of `text`, numbered
/// from 1.00 and dated `now`; set `SRCTYPE`. Returns the line count.
pub fn srcput(
    txn: &mut Txn<'_>,
    job: JobHandle,
    file: ObjectId,
    member: MemberSpec,
    text: &str,
    srctype: Option<Name>,
    now: u64,
) -> Result<u64> {
    crate::auth::check(txn, "file.open-output", file)?;
    let desc = source_file(txn, file)?;
    let entry = member_of(&desc, member)?;
    let hold = Hold::take(
        txn,
        job,
        &[
            (file, LockState::SharedUpdate),
            (entry.object, LockState::Exclusive),
        ],
        &desc.name().to_string(),
    )?;
    let result = Member::open(txn, &desc, &entry).and_then(|mut m| {
        m.jrn.job = job;
        m.clear(txn)?;
        let fmt = m.format().clone();
        let f = &fmt.format.fields;
        let mut record = vec![0u8; usize::try_from(fmt.format.record_len).expect("fits")];
        let date = srcdat(now);
        let mut n = 0u64;
        for (i, line) in text.lines().enumerate() {
            n += 1;
            let seq = Decimal::new(i128::try_from(i + 1).expect("fits"), 0).expect("small");
            record::write_field(&f[0], &mut record, &Value::Num(seq))?;
            record::write_field(&f[1], &mut record, &Value::Num(date.clone()))?;
            let bytes: Vec<u8> = line
                .chars()
                .map(|c| ccsid::byte_of(c).unwrap_or(ccsid::SPACE))
                .collect();
            record::write_field(&f[2], &mut record, &Value::Char(bytes))?;
            m.write_new(txn, &record, &[])?;
        }
        Ok(n)
    });
    hold.release(txn);
    let n = result?;
    file::chgpfm(txn, file, entry.name, None, Some(srctype), now)?;
    Ok(n)
}

/// Replace a source member's records with `lines` of data bytes
/// (already in the file's CCSID; each padded or cut to the data
/// width), numbered from 1.00 and dated `now`. The stream file
/// systems write members through this. Returns the record count.
pub fn put_records(
    txn: &mut Txn<'_>,
    job: JobHandle,
    file: ObjectId,
    member: MemberSpec,
    lines: &[Vec<u8>],
    now: u64,
) -> Result<u64> {
    crate::auth::check(txn, "file.open-output", file)?;
    let desc = source_file(txn, file)?;
    let entry = member_of(&desc, member)?;
    let hold = Hold::take(
        txn,
        job,
        &[
            (file, LockState::SharedUpdate),
            (entry.object, LockState::Exclusive),
        ],
        &desc.name().to_string(),
    )?;
    let result = Member::open(txn, &desc, &entry).and_then(|mut m| {
        m.jrn.job = job;
        m.clear(txn)?;
        let fmt = m.format().clone();
        let f = &fmt.format.fields;
        let width = f[2].length as usize;
        let mut record = vec![0u8; usize::try_from(fmt.format.record_len).expect("fits")];
        let date = srcdat(now);
        let mut n = 0u64;
        for line in lines {
            n += 1;
            let seq = Decimal::new(i128::from(n), 0).expect("small");
            record::write_field(&f[0], &mut record, &Value::Num(seq))?;
            record::write_field(&f[1], &mut record, &Value::Num(date.clone()))?;
            let mut bytes = line[..line.len().min(width)].to_vec();
            bytes.resize(width, ccsid::SPACE);
            record::write_field(&f[2], &mut record, &Value::Char(bytes))?;
            m.write_new(txn, &record, &[])?;
        }
        Ok(n)
    });
    hold.release(txn);
    let n = result?;
    file::chgpfm(txn, file, entry.name, None, None, now)?;
    Ok(n)
}

/// The member's records as (sequence in hundredths, date, data bytes).
pub fn records(
    txn: &mut Txn<'_>,
    file: ObjectId,
    member: MemberSpec,
) -> Result<Vec<(u32, u32, Vec<u8>)>> {
    let desc = source_file(txn, file)?;
    let entry = member_of(&desc, member)?;
    let m = Member::open(txn, &desc, &entry)?;
    let fmt = m.format().clone();
    let f = &fmt.format.fields;
    let num = |v: Value, scale: u8| -> u32 {
        match v {
            Value::Num(d) => d
                .rescale(scale, Rounding::Truncate)
                .ok()
                .and_then(|d| d.mantissa_i128())
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or(0),
            _ => 0,
        }
    };
    let mut out = Vec::new();
    for s in m.scan(txn, 1, usize::MAX)? {
        let seq = num(record::read_field(&f[0], &s.record, &[], None)?, 2);
        let date = num(record::read_field(&f[1], &s.record, &[], None)?, 0);
        out.push((seq, date, record::field_bytes(&f[2], &s.record).to_vec()));
    }
    Ok(out)
}

/// A source member as an editor reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceMember {
    /// The member's `*MBR` object, for locks.
    pub object: ObjectId,
    /// Its source type; blank for none.
    pub srctype: String,
    /// Its text.
    pub text: String,
    /// The data length of a record.
    pub width: usize,
    /// The records: sequence in hundredths, date `yymmdd`, data.
    pub records: Vec<(u32, u32, String)>,
}

/// The data length of a source file's records.
pub fn data_width(txn: &mut Txn<'_>, file: ObjectId) -> Result<usize> {
    crate::auth::check(txn, "file.open-input", file)?;
    let desc = source_file(txn, file)?;
    let width = desc
        .formats
        .first()
        .and_then(|f| f.format.fields.get(2))
        .map_or(80, |f| f.length as usize);
    Ok(width)
}

/// Read a source member whole for an editor (`STRSEU`).
pub fn read_member(txn: &mut Txn<'_>, file: ObjectId, member: MemberSpec) -> Result<SourceMember> {
    crate::auth::check(txn, "file.open-input", file)?;
    let desc = source_file(txn, file)?;
    let entry = member_of(&desc, member)?;
    let width = data_width(txn, file)?;
    let text = store::obj::ops::describe(txn, entry.object)
        .map(|h| ccsid::from_ebcdic(&h.text).trim_end().to_string())
        .unwrap_or_default();
    let records = records(txn, file, member)?
        .into_iter()
        .map(|(s, d, data)| (s, d, ccsid::from_ebcdic(&data).trim_end().to_string()))
        .collect();
    Ok(SourceMember {
        object: entry.object,
        srctype: entry.srctype.map(|n| n.to_string()).unwrap_or_default(),
        text,
        width,
        records,
    })
}

/// Replace a member's records with an editor's: each keeps its own
/// sequence number and date. The member is added when missing; its
/// source type and text are set.
#[allow(clippy::too_many_arguments)] // the member, its records and attributes
pub fn write_member(
    txn: &mut Txn<'_>,
    job: JobHandle,
    file: ObjectId,
    member: Name,
    lines: &[(u32, u32, String)],
    srctype: Option<Name>,
    text: &str,
    now: u64,
) -> Result<u64> {
    crate::auth::check(txn, "file.open-output", file)?;
    let mut desc = source_file(txn, file)?;
    if desc.member(MemberSpec::Name(member)).is_none() {
        file::add_member(txn, file, member, text, srctype, now)?;
        desc = source_file(txn, file)?;
    }
    let entry = member_of(&desc, MemberSpec::Name(member))?;
    // The editor holds the member itself; only the file is taken here.
    let hold = Hold::take(
        txn,
        job,
        &[(file, LockState::SharedUpdate)],
        &desc.name().to_string(),
    )?;
    let result = Member::open(txn, &desc, &entry).and_then(|mut m| {
        m.jrn.job = job;
        m.clear(txn)?;
        let fmt = m.format().clone();
        let f = &fmt.format.fields;
        let mut record = vec![0u8; usize::try_from(fmt.format.record_len).expect("fits")];
        let mut n = 0u64;
        for (seq, date, line) in lines {
            n += 1;
            let s = Decimal::new(i128::from(*seq), 2).expect("small");
            record::write_field(&f[0], &mut record, &Value::Num(s))?;
            let d = Decimal::new(i128::from(*date), 0).expect("six digits");
            record::write_field(&f[1], &mut record, &Value::Num(d))?;
            let bytes: Vec<u8> = line
                .chars()
                .map(|c| ccsid::byte_of(c).unwrap_or(ccsid::SPACE))
                .collect();
            record::write_field(&f[2], &mut record, &Value::Char(bytes))?;
            m.write_new(txn, &record, &[])?;
        }
        Ok(n)
    });
    hold.release(txn);
    let n = result?;
    file::chgpfm(txn, file, entry.name, Some(text), Some(srctype), now)?;
    Ok(n)
}

/// The member as UTF-8 text, one line per record, trailing blanks gone.
pub fn srcget(txn: &mut Txn<'_>, file: ObjectId, member: MemberSpec) -> Result<String> {
    crate::auth::check(txn, "file.open-input", file)?;
    let mut out = String::new();
    for (_, _, data) in records(txn, file, member)? {
        out.push_str(ccsid::from_ebcdic(&data).trim_end());
        out.push('\n');
    }
    Ok(out)
}

/// The member as DDS source to compile.
pub fn source_of(txn: &mut Txn<'_>, file: ObjectId, member: MemberSpec) -> Result<dds::Source> {
    let recs = records(txn, file, member)?;
    Ok(dds::Source::from_records(
        recs.iter().map(|(s, d, data)| (*s, *d, data.as_slice())),
    ))
}
