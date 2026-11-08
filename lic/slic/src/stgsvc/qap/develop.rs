// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QAP's `Develop` members (docs/QAP.md, version 8): `ReadMember`,
//! `WriteMember` and `UnlockMember` through `db::source`, as the job's
//! signed-on user (the store's guard checks it), with the member's
//! locks the job's own: `*SHRNUP` while the client edits, as `STRSEU`
//! holds a member. Data goes between UTF-8 and the file's CCSID; dates
//! follow `qapd::develop::dated`. The other operations run in the job
//! through the commands (`work/qsjob/src/work/qap/develop.rs`).

use qap_proto::ops::develop::MemberData;
use qap_proto::types::{MemberName, SourceRecord};
use qsname::{MemberSpec, Name};
use stg::QapRefused;
use store::obj::{JobHandle, LockState, ObjectId, types};
use store::txn::Txn;

use super::super::served::JobCtx;
use super::{ERROR, INVALID_REQUEST, NOT_AUTHORIZED, connection_job, refused};
use crate::ipl::system::System;

const NOT_FOUND: u16 = 7;
const IN_USE: u16 = 14;

/// A refusal from the store or the database: the status by its message.
fn failed(msgid: &str, text: &str) -> QapRefused {
    let status = match msgid {
        "CPF9801" | "CPF9810" | "CPF9812" | "CPF9815" | "CPF2110" => NOT_FOUND,
        "CPF9802" | "CPF9820" | "CPF2182" | "CPF2189" | "CPF22E9" | "CPF2207" => NOT_AUTHORIZED,
        "CPF1002" | "CPF3330" | "CPF9803" | "CPF4128" => IN_USE,
        _ => ERROR,
    };
    refused(status, msgid, text, &[])
}

fn db_failed(e: &db::DbError) -> QapRefused {
    match e {
        db::DbError::Store(s) => failed(s.msgid().unwrap_or("CPF9898"), &s.to_string()),
        e => failed(e.msgid().unwrap_or("CPF9898"), &e.to_string()),
    }
}

/// `CPF1002`: another job holds the member.
fn held(m: &MemberName) -> QapRefused {
    refused(
        IN_USE,
        qapd::develop::HELD,
        "",
        &[m.member.trim(), m.file.name.trim(), "*MBR"],
    )
}

/// The source file, its member and the member's object.
fn resolve(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    m: &MemberName,
) -> Result<(ObjectId, Name, ObjectId, db::file::FileDesc), QapRefused> {
    let names = crate::svc::ile::name_context(txn, qsys);
    let (lib, file) = (m.file.library.trim(), m.file.name.trim());
    let fid = wm::obj::find(txn, qsys, &names, lib, file, types::FILE).map_err(|e| match e {
        wm::Error::Store(s) if s.msgid().is_some_and(|i| i != "CPF9801") => {
            failed(s.msgid().unwrap_or("CPF9898"), &s.to_string())
        }
        wm::Error::Value { msgid, what } => refused(INVALID_REQUEST, msgid, &what, &[]),
        _ => refused(NOT_FOUND, "CPF9812", "", &[file, lib]),
    })?;
    let name = Name::parse(m.member.trim()).map_err(|e| {
        refused(
            INVALID_REQUEST,
            "CPD0078",
            &format!("{}: {e}", m.member),
            &[],
        )
    })?;
    let desc = db::file::describe(txn, fid).map_err(|e| db_failed(&e))?;
    let entry = desc
        .member(MemberSpec::Name(name))
        .copied()
        .ok_or_else(|| refused(NOT_FOUND, "CPF9815", "", &[&name.to_string(), file, lib]))?;
    Ok((fid, name, entry.object, desc))
}

/// Whether a job other than `job` holds `obj` in a state that keeps
/// this one from changing it (anything but `*SHRRD`), or, with
/// `exclusive_only`, holds it `*EXCL`.
fn other_holds(txn: &mut Txn<'_>, job: JobHandle, obj: ObjectId, exclusive_only: bool) -> bool {
    txn.store().locks_mut().holders(obj).iter().any(|h| {
        h.job != job
            && if exclusive_only {
                h.state == LockState::Exclusive
            } else {
                h.state != LockState::SharedRead
            }
    })
}

/// Whether `job` holds `obj` `*SHRNUP`.
fn holds(txn: &mut Txn<'_>, job: JobHandle, obj: ObjectId) -> bool {
    txn.store()
        .locks_mut()
        .held_by(job)
        .iter()
        .any(|(o, s)| *o == obj && *s == LockState::SharedNoUpdate)
}

/// The file's CCSID for conversion: `TCP1A5A` when binary.
fn file_ccsid(desc: &db::file::FileDesc, to: bool) -> Result<u16, QapRefused> {
    let c = if desc.attr.ccsid == 0 {
        37
    } else {
        desc.attr.ccsid
    };
    if c == 65535 || ccsid::conv::Codec::of(c).ok().flatten().is_none() {
        let (a, b) = if to {
            ("1208".to_string(), c.to_string())
        } else {
            (c.to_string(), "1208".to_string())
        };
        return Err(refused(ERROR, qapd::develop::NO_CONVERSION, "", &[&a, &b]));
    }
    Ok(c)
}

fn records_of(
    txn: &mut Txn<'_>,
    fid: ObjectId,
    name: Name,
    ccsid: u16,
) -> Result<Vec<SourceRecord>, QapRefused> {
    let raw = db::source::records(txn, fid, MemberSpec::Name(name)).map_err(|e| db_failed(&e))?;
    Ok(raw
        .into_iter()
        .map(|(seq, date, data)| SourceRecord {
            seq,
            date,
            data: ccsid::conv::to_string(ccsid, &data)
                .unwrap_or_default()
                .trim_end()
                .to_string(),
        })
        .collect())
}

/// `ReadMember`.
pub fn read(
    system: &mut System,
    ctx: &JobCtx,
    m: &MemberName,
    lock: bool,
) -> Result<MemberData, QapRefused> {
    connection_job(ctx)?;
    let job = JobHandle(ctx.job.handle);
    let r = super::super::spool::with_store(system, |txn, qsys| {
        let (fid, name, obj, desc) = match resolve(txn, qsys, m) {
            Ok(v) => v,
            Err(e) => return Ok(Err(e)),
        };
        let sm = match db::source::read_member(txn, fid, MemberSpec::Name(name)) {
            Ok(s) => s,
            Err(e) => return Ok(Err(db_failed(&e))),
        };
        let ccsid = match file_ccsid(&desc, false) {
            Ok(c) => c,
            Err(e) => return Ok(Err(e)),
        };
        if other_holds(txn, job, obj, !lock) {
            return Ok(Err(held(m)));
        }
        let records = match records_of(txn, fid, name, ccsid) {
            Ok(r) => r,
            Err(e) => return Ok(Err(e)),
        };
        if lock
            && !holds(txn, job, obj)
            && txn
                .store()
                .locks_mut()
                .lock(job, obj, LockState::SharedNoUpdate)
                .is_err()
        {
            return Ok(Err(held(m)));
        }
        Ok(Ok(MemberData {
            srctype: sm.srctype,
            text: sm.text,
            width: u16::try_from(sm.width).unwrap_or(u16::MAX),
            ccsid,
            records,
        }))
    });
    r.unwrap_or_else(|e| Err(failed("CPF9898", &e)))
}

/// Each record's data in `ccsid`, no longer than `width`: `TCP1A5A`
/// for a character with no encoding, `TCP1A62` for a record too long.
fn encoded(
    records: &[SourceRecord],
    ccsid: u16,
    width: usize,
) -> Result<Vec<(u32, u32, Vec<u8>)>, QapRefused> {
    let codec = ccsid::conv::Codec::of(ccsid).ok().flatten();
    records
        .iter()
        .map(|r| {
            let exact = r.data.chars().all(|c| {
                let mut b = Vec::new();
                codec.as_ref().is_some_and(|k| k.encode(c, &mut b))
            });
            if !exact {
                return Err(refused(
                    ERROR,
                    qapd::develop::NO_CONVERSION,
                    "",
                    &["1208", &ccsid.to_string()],
                ));
            }
            let bytes = ccsid::conv::from_str(ccsid, r.data.trim_end()).unwrap_or_default();
            if bytes.len() > width {
                let seq = format!("{:04}.{:02}", r.seq / 100, r.seq % 100);
                return Err(refused(
                    INVALID_REQUEST,
                    qapd::develop::TOO_LONG,
                    "",
                    &[&seq, &width.to_string()],
                ));
            }
            Ok((r.seq, r.date, bytes))
        })
        .collect()
}

/// `WriteMember`: the member replaced whole; the job's lock on it ends.
pub fn write(
    system: &mut System,
    ctx: &JobCtx,
    m: &MemberName,
    records: Vec<SourceRecord>,
    renumber: bool,
    (srctype, text): (Option<&str>, Option<&str>),
) -> Result<u32, QapRefused> {
    connection_job(ctx)?;
    let job = JobHandle(ctx.job.handle);
    let st = match srctype.map(str::trim) {
        None => None,
        Some("") => Some(None),
        Some(t) => Some(Some(Name::parse(t).map_err(|e| {
            refused(INVALID_REQUEST, "CPD0078", &format!("{t}: {e}"), &[])
        })?)),
    };
    let now = crate::stgsvc::msgs::now_micros() / 1_000_000;
    let today = qapd::develop::yymmdd(now);
    let r = super::super::spool::with_store(system, |txn, qsys| {
        let (fid, name, obj, desc) = match resolve(txn, qsys, m) {
            Ok(v) => v,
            Err(e) => return Ok(Err(e)),
        };
        if other_holds(txn, job, obj, false) {
            return Ok(Err(held(m)));
        }
        let ccsid = match file_ccsid(&desc, true) {
            Ok(c) => c,
            Err(e) => return Ok(Err(e)),
        };
        let width = match db::source::data_width(txn, fid) {
            Ok(w) => w,
            Err(e) => return Ok(Err(db_failed(&e))),
        };
        let old = match records_of(txn, fid, name, ccsid) {
            Ok(r) => r,
            Err(e) => return Ok(Err(e)),
        };
        let new = qapd::develop::dated(&old, records, renumber, today);
        let lines = match encoded(&new, ccsid, width) {
            Ok(l) => l,
            Err(e) => return Ok(Err(e)),
        };
        // The write holds the member alone, as SEU's save does.
        if txn
            .store()
            .locks_mut()
            .lock(job, obj, LockState::Exclusive)
            .is_err()
        {
            return Ok(Err(held(m)));
        }
        let n = db::source::write_records(txn, job, fid, name, &lines, st, text, now);
        let locks = txn.store().locks_mut();
        locks.unlock(job, obj, LockState::Exclusive);
        locks.unlock(job, obj, LockState::SharedNoUpdate);
        Ok(n.map_err(|e| db_failed(&e))
            .map(|n| u32::try_from(n).unwrap_or(u32::MAX)))
    });
    r.unwrap_or_else(|e| Err(failed("CPF9898", &e)))
}

/// `UnlockMember`: `TCP1A61` when the job holds no lock on it.
pub fn unlock(system: &mut System, ctx: &JobCtx, m: &MemberName) -> Result<(), QapRefused> {
    connection_job(ctx)?;
    let job = JobHandle(ctx.job.handle);
    let r = super::super::spool::with_store(system, |txn, qsys| {
        let (_, _, obj, _) = match resolve(txn, qsys, m) {
            Ok(v) => v,
            Err(e) => return Ok(Err(e)),
        };
        if txn
            .store()
            .locks_mut()
            .unlock(job, obj, LockState::SharedNoUpdate)
        {
            Ok(Ok(()))
        } else {
            let what = format!(
                "{}/{} {}",
                m.file.library.trim(),
                m.file.name.trim(),
                m.member.trim()
            );
            Ok(Err(refused(
                NOT_FOUND,
                qapd::develop::NOT_LOCKED,
                "",
                &[&what],
            )))
        }
    });
    r.unwrap_or_else(|e| Err(failed("CPF9898", &e)))
}
