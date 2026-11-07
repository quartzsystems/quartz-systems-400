// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 160: QAP version 8 (`Develop`, `Jobs`, `Sql` 12 and 13) through
//! the real connection jobs as `QAPTEST`, every operation once at least,
//! and the gates: a version 7 connection is not offered services 9 and
//! 10 and its requests for them are `InvalidRequest`.
//!
//! The exit test: `HLINV/QRPGLESRC ASSETS` is written, its records'
//! dates set back to 2026-01-05 with SQL, and copied to `ASSETSCPY`
//! (`CopyMember`, `CPYSRCF`); `ReadMember` with a lock (a second
//! connection is refused `InUse`), `WriteMember` of the same records,
//! then the two members compared record by record as `CMPPFM` compares
//! them: identical, dates unchanged. A changed record then carries
//! today's date and the others keep theirs. QS/400 ships no `CMPPFM`
//! yet, so the step compares the records itself.

use std::time::{SystemTime, UNIX_EPOCH};

use qap_proto::ops::command::{CommandReply, CommandText};
use qap_proto::ops::connection::HelloReply;
use qap_proto::ops::develop::{
    ChangeMember, CompileReply, CopyMember, CreateMember, DescribeObject, Libraries, LibraryKind,
    LibraryList, ListLibraries, ListMembers, ListObjects, MemberData, MemberRef, Members,
    ObjectDescription, Objects, ReadMember, RenameMember, SetLibraryList, WriteMember, Written,
};
use qap_proto::ops::jobs::{EndJob, JobLog, JobLogEntries, JobRef, Jobs, ListJobs};
use qap_proto::ops::sql::{Indexes, ListIndexes, ListRoutines, Routines};
use qap_proto::types::{EndHow, JobId, MemberName, ObjectName, SourceRecord};
use qap_proto::{Envelope, Op, Service, Status};

use super::client::{Conn, brief, msgid};
use super::sql;

/// The date the step sets the member's records back to.
const OLD: u32 = 260_105;

fn file() -> ObjectName {
    ObjectName {
        library: "HLINV".into(),
        name: "QRPGLESRC".into(),
    }
}

fn mbr(name: &str) -> MemberName {
    MemberName {
        file: file(),
        member: name.into(),
    }
}

/// `want` or the reply as an error.
fn status(r: &Envelope, want: Status, what: &str) -> Result<(), String> {
    if r.status == want {
        Ok(())
    } else {
        Err(format!("{what}: wanted {want:?}, got {}", brief(r)))
    }
}

/// `yymmdd` of a Unix time, UTC.
fn yymmdd(unix: u64) -> u32 {
    let z = i64::try_from(unix / 86_400).unwrap_or(0) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    u32::try_from((y % 100) * 10_000 + m * 100 + d).unwrap_or(0)
}

/// The source of `ASSETS`: a small program of Quartz's own.
fn source() -> Vec<SourceRecord> {
    let lines = [
        "**FREE",
        "ctl-opt dftactgrp(*no) actgrp(*caller);",
        "// Count the assets in service.",
        "dcl-s count int(10) inz(0);",
        "dcl-s i int(10);",
        "for i = 1 to 12;",
        "  count += 1;",
        "endfor;",
        "dsply ('Assets: ' + %char(count));",
        "*inlr = *on;",
    ];
    lines
        .iter()
        .zip(1u32..)
        .map(|(l, n)| SourceRecord {
            seq: n * 100,
            date: 0,
            data: (*l).to_string(),
        })
        .collect()
}

fn read(c: &mut Conn, name: &str, lock: bool) -> Result<Envelope, String> {
    c.call(
        Op::ReadMember,
        &ReadMember {
            member: mbr(name),
            lock,
        },
    )
}

fn records(c: &mut Conn, name: &str) -> Result<Vec<SourceRecord>, String> {
    let r = read(c, name, false)?;
    status(&r, Status::Ok, &format!("ReadMember {name}"))?;
    let d: MemberData = r.body_as().map_err(|e| e.to_string())?;
    Ok(d.records)
}

fn write(c: &mut Conn, name: &str, recs: Vec<SourceRecord>) -> Result<Envelope, String> {
    c.call(
        Op::WriteMember,
        &WriteMember {
            member: mbr(name),
            records: recs,
            renumber: false,
            srctype: None,
            text: None,
            first: true,
            last: true,
        },
    )
}

/// A version 7 connection (`v7`, signed on) sees no version 8.
pub fn gates(v7: &mut Conn, hello: &HelloReply) -> Result<String, String> {
    if hello.version != 7
        || hello.services.contains(&Service::Develop)
        || hello.services.contains(&Service::Jobs)
    {
        return Err(format!("a version 7 Hello: {hello:?}"));
    }
    for op in [
        Op::GetLibraryList,
        Op::ListLibraries,
        Op::ReadMember,
        Op::Compile,
        Op::ListJobs,
        Op::ListRoutines,
        Op::ListIndexes,
    ] {
        let r = v7.call(op, &())?;
        status(&r, Status::InvalidRequest, &format!("{op:?} at version 7"))?;
    }
    Ok("version 7: no Develop or Jobs, their requests InvalidRequest".into())
}

/// The library list, libraries, objects and the members' life.
#[allow(clippy::too_many_lines)] // every operation in turn
pub fn develop(a: &mut Conn, b: &mut Conn) -> Result<String, String> {
    let before: LibraryList = a.ok(Op::GetLibraryList, &())?;
    let set: LibraryList = a.ok(
        Op::SetLibraryList,
        &SetLibraryList {
            current: Some("hlinv".into()),
            user: vec!["QGPL".into(), "QTEMP".into()],
        },
    )?;
    if set.current.as_deref() != Some("HLINV") || set.user != ["QGPL", "QTEMP"] {
        return Err(format!("SetLibraryList: {set:?}"));
    }
    let again: LibraryList = a.ok(Op::GetLibraryList, &())?;
    if again != set || again.system.is_empty() {
        return Err(format!("GetLibraryList after a set: {again:?}"));
    }
    // RunCommand sees the list.
    let rc: CommandReply = a.ok(
        Op::RunCommand,
        &CommandText {
            command: "CHKOBJ OBJ(*CURLIB/QRPGLESRC) OBJTYPE(*FILE)".into(),
        },
    )?;
    let _ = rc;
    let bad = a.call(
        Op::SetLibraryList,
        &SetLibraryList {
            current: None,
            user: vec!["QAPNOSUCH".into()],
        },
    )?;
    status(
        &bad,
        Status::NotFound,
        "SetLibraryList of a library not there",
    )?;
    let libs: Libraries = a.ok(
        Op::ListLibraries,
        &ListLibraries {
            pattern: "HLINV".into(),
            start: 0,
            max: 0,
        },
    )?;
    if libs.libraries.len() != 1 || libs.libraries[0].kind != LibraryKind::Test {
        return Err(format!("ListLibraries HLINV: {libs:?}"));
    }
    let objs: Objects = a.ok(
        Op::ListObjects,
        &ListObjects {
            library: "*CURLIB".into(),
            pattern: "QRPG*".into(),
            types: vec!["*FILE".into()],
            attribute: None,
            start: 0,
            max: 0,
        },
    )?;
    if !objs
        .objects
        .iter()
        .any(|o| o.name == "QRPGLESRC" && o.size > 0 && o.changed > 0)
    {
        return Err(format!("ListObjects *CURLIB: {objs:?}"));
    }
    let d: ObjectDescription = a.ok(
        Op::DescribeObject,
        &DescribeObject {
            object: ObjectName {
                library: "*LIBL".into(),
                name: "QRPGLESRC".into(),
            },
            r#type: "*FILE".into(),
        },
    )?;
    if !d
        .attributes
        .iter()
        .any(|x| x.name == "Owner" && !x.value.is_empty())
    {
        return Err(format!("DescribeObject: {d:?}"));
    }
    for m in ["ASSETS", "ASSETSCPY", "ASSETSREN"] {
        let _ = a.call(Op::DeleteMember, &MemberRef { member: mbr(m) })?;
    }
    let created = a.call(
        Op::CreateMember,
        &CreateMember {
            member: mbr("ASSETS"),
            srctype: "RPGLE".into(),
            text: "Asset count".into(),
        },
    )?;
    status(&created, Status::Ok, "CreateMember ASSETS")?;
    let twice = a.call(
        Op::CreateMember,
        &CreateMember {
            member: mbr("ASSETS"),
            srctype: "RPGLE".into(),
            text: String::new(),
        },
    )?;
    status(&twice, Status::Exists, "CreateMember of a member there")?;
    let w = write(a, "ASSETS", source())?;
    status(&w, Status::Ok, "WriteMember ASSETS")?;
    // Old dates, as a member edited long ago has them, through an alias
    // of the member.
    sql::run_or(
        a,
        "CREATE ALIAS QTEMP.QAPASSETS FOR HLINV.QRPGLESRC (ASSETS)",
        &["42710"],
    )?;
    sql::run(a, &format!("UPDATE QTEMP.QAPASSETS SET SRCDAT = {OLD}"))?;
    let copied = a.call(
        Op::CopyMember,
        &CopyMember {
            from: mbr("ASSETS"),
            to: mbr("ASSETSCPY"),
            replace: false,
        },
    )?;
    status(&copied, Status::Ok, "CopyMember to ASSETSCPY")?;
    let exists = a.call(
        Op::CopyMember,
        &CopyMember {
            from: mbr("ASSETS"),
            to: mbr("ASSETSCPY"),
            replace: false,
        },
    )?;
    status(&exists, Status::Exists, "CopyMember without replace")?;
    let exit = exit_test(a, b)?;
    let members: Members = a.ok(
        Op::ListMembers,
        &ListMembers {
            file: file(),
            pattern: "ASSETS*".into(),
            start: 0,
            max: 0,
        },
    )?;
    let names: Vec<&str> = members.members.iter().map(|m| m.name.as_str()).collect();
    if names != ["ASSETS", "ASSETSCPY"] || members.members[0].records != 10 {
        return Err(format!("ListMembers: {:?}", members.members));
    }
    let ren = a.call(
        Op::RenameMember,
        &RenameMember {
            member: mbr("ASSETSCPY"),
            new_name: "ASSETSREN".into(),
        },
    )?;
    status(&ren, Status::Ok, "RenameMember")?;
    let taken = a.call(
        Op::RenameMember,
        &RenameMember {
            member: mbr("ASSETSREN"),
            new_name: "ASSETS".into(),
        },
    )?;
    status(&taken, Status::Exists, "RenameMember to a name taken")?;
    let chg = a.call(
        Op::ChangeMember,
        &ChangeMember {
            member: mbr("ASSETSREN"),
            srctype: Some("SQLRPGLE".into()),
            text: Some("renamed copy".into()),
        },
    )?;
    status(&chg, Status::Ok, "ChangeMember")?;
    let ren: MemberData = read(a, "ASSETSREN", false)?
        .body_as()
        .map_err(|e| e.to_string())?;
    if (ren.srctype.as_str(), ren.text.as_str()) != ("SQLRPGLE", "renamed copy") {
        return Err(format!("after ChangeMember: {} {}", ren.srctype, ren.text));
    }
    let del = a.call(
        Op::DeleteMember,
        &MemberRef {
            member: mbr("ASSETSREN"),
        },
    )?;
    status(&del, Status::Ok, "DeleteMember")?;
    let gone = a.call(
        Op::DeleteMember,
        &MemberRef {
            member: mbr("ASSETSREN"),
        },
    )?;
    status(
        &gone,
        Status::NotFound,
        "DeleteMember of a member not there",
    )?;
    let pieces = pieces(a)?;
    let snow = write(
        a,
        "ASSETS",
        vec![SourceRecord {
            seq: 100,
            date: 0,
            data: "\u{2603}".into(),
        }],
    )?;
    if snow.status != Status::Error || msgid(&snow) != "TCP1A5A" {
        return Err(format!(
            "a character CCSID 37 cannot hold: {}",
            brief(&snow)
        ));
    }
    let compile = compile(a)?;
    let restored = a.call(
        Op::SetLibraryList,
        &SetLibraryList {
            current: before.current.clone(),
            user: before.user.clone(),
        },
    )?;
    status(&restored, Status::Ok, "SetLibraryList back")?;
    Ok(format!(
        "library list set and read back; HLINV *TEST; objects, description, members; {exit}; renamed, changed, deleted; {pieces}; TCP1A5A for a character CCSID 37 lacks; {compile}"
    ))
}

/// The exit test: read, write back, compare with the copy; a change.
#[allow(clippy::many_single_char_names)] // the two connections and replies
fn exit_test(a: &mut Conn, b: &mut Conn) -> Result<String, String> {
    let locked = read(a, "ASSETS", true)?;
    status(&locked, Status::Ok, "ReadMember ASSETS with a lock")?;
    let d: MemberData = locked.body_as().map_err(|e| e.to_string())?;
    if d.records.iter().any(|r| r.date != OLD) || d.records.len() != 10 {
        return Err(format!("ASSETS as read: {:?}", d.records));
    }
    let other = read(b, "ASSETS", true)?;
    if other.status != Status::InUse || msgid(&other) != "CPF1002" {
        return Err(format!("a second connection's lock: {}", brief(&other)));
    }
    let w = write(a, "ASSETS", d.records.clone())?;
    status(&w, Status::Ok, "WriteMember of the same records")?;
    let n: Written = w.body_as().map_err(|e| e.to_string())?;
    let after = records(a, "ASSETS")?;
    let copy = records(a, "ASSETSCPY")?;
    if after != copy || n.records != 10 {
        return Err(format!(
            "ASSETS after the write differs from its copy:\n{after:?}\n{copy:?}"
        ));
    }
    // The write ended the lock: the other connection may take it now.
    let other = read(b, "ASSETS", true)?;
    status(&other, Status::Ok, "a lock after the holder wrote")?;
    let unlock = b.call(
        Op::UnlockMember,
        &MemberRef {
            member: mbr("ASSETS"),
        },
    )?;
    status(&unlock, Status::Ok, "UnlockMember")?;
    let again = b.call(
        Op::UnlockMember,
        &MemberRef {
            member: mbr("ASSETS"),
        },
    )?;
    if again.status != Status::NotFound || msgid(&again) != "TCP1A61" {
        return Err(format!("UnlockMember of nothing held: {}", brief(&again)));
    }
    // One record changed: today's date on it alone.
    let mut changed = after.clone();
    changed[3].data = "dcl-s count int(20) inz(0);".into();
    let w = write(a, "ASSETS", changed)?;
    status(&w, Status::Ok, "WriteMember with a change")?;
    let now = records(a, "ASSETS")?;
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let today = [
        yymmdd(secs),
        yymmdd(secs.saturating_sub(86_400)),
        yymmdd(secs + 86_400),
    ];
    if !today.contains(&now[3].date) {
        return Err(format!("the changed record's date: {}", now[3].date));
    }
    if now.iter().enumerate().any(|(i, r)| i != 3 && r.date != OLD) {
        return Err(format!("unchanged records' dates: {now:?}"));
    }
    Ok(format!(
        "ASSETS read, written back and identical to its CPYSRCF copy, dates {OLD} kept; the changed record dated {}",
        now[3].date
    ))
}

/// A write in two pieces; a piece without a first refused.
fn pieces(a: &mut Conn) -> Result<String, String> {
    let all = records(a, "ASSETS")?;
    let piece = |a: &mut Conn, recs: &[SourceRecord], first, last| {
        a.call(
            Op::WriteMember,
            &WriteMember {
                member: mbr("ASSETS"),
                records: recs.to_vec(),
                renumber: true,
                srctype: None,
                text: Some("in pieces".into()),
                first,
                last,
            },
        )
    };
    let stray = piece(a, &all[..2], false, false)?;
    status(&stray, Status::InvalidRequest, "a piece without a first")?;
    let one = piece(a, &all[..5], true, false)?;
    let w: Written = one.body_as().map_err(|e| e.to_string())?;
    let mid = records(a, "ASSETS")?;
    if w.records != 5 || mid != all {
        return Err("the member changed before the last piece".into());
    }
    let two = piece(a, &all[5..], false, true)?;
    status(&two, Status::Ok, "the last piece")?;
    let back = records(a, "ASSETS")?;
    let seqs_ok = back.iter().zip(1u32..).all(|(r, n)| r.seq == n * 100);
    let same = back
        .iter()
        .zip(&all)
        .all(|(x, y)| x.data == y.data && x.date == y.date);
    if !seqs_ok || !same || back.len() != all.len() {
        return Err(format!("after the pieces: {back:?}"));
    }
    Ok("written in two pieces, renumbered".into())
}

/// `Compile` answers in the lane: until the compilers leave their
/// diagnostics on the job it is `TCP1A60`; once they do, a body.
fn compile(a: &mut Conn) -> Result<String, String> {
    let r = a.call(
        Op::Compile,
        &CommandText {
            command: "CRTBNDRPG PGM(QTEMP/ASSETS) SRCFILE(HLINV/QRPGLESRC) SRCMBR(ASSETS)".into(),
        },
    )?;
    if r.status == Status::Error && msgid(&r) == "TCP1A60" {
        return Ok("Compile refused TCP1A60 (no diagnostics yet)".into());
    }
    let c: CompileReply = r
        .body_as()
        .map_err(|e| format!("Compile's body: {e} ({})", brief(&r)))?;
    Ok(format!(
        "Compile {:?}, {} messages, {} diagnostics",
        r.status,
        c.messages.len(),
        c.diagnostics.len()
    ))
}

/// `Jobs`: a batch job of `QAPTEST`'s listed, held, released, its log
/// read and ended; another user's jobs refused; the connection's own
/// job not ended.
pub fn jobs(a: &mut Conn, own: &JobId) -> Result<String, String> {
    let sbm: CommandReply = a.ok(
        Op::RunCommand,
        &CommandText {
            command: "SBMJOB CMD(DLYJOB DLY(120)) JOB(QAPDEVJOB) JOBQ(QBATCH)".into(),
        },
    )?;
    let _ = sbm;
    let mut found = None;
    for _ in 0..40 {
        let l: Jobs = a.ok(
            Op::ListJobs,
            &ListJobs {
                user: "*CURRENT".into(),
                status: None,
                start: 0,
                max: 0,
            },
        )?;
        if let Some(j) = l.jobs.iter().find(|j| j.id.name == "QAPDEVJOB") {
            found = Some(j.clone());
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    let j = found.ok_or("QAPDEVJOB never listed")?;
    let hold = a.call(Op::HoldJob, &JobRef { job: j.id.clone() })?;
    status(&hold, Status::Ok, "HoldJob")?;
    let rls = a.call(Op::ReleaseJob, &JobRef { job: j.id.clone() })?;
    status(&rls, Status::Ok, "ReleaseJob")?;
    let log = a.call(
        Op::JobLog,
        &JobLog {
            job: j.id.clone(),
            start: 0,
            max: 0,
        },
    )?;
    let entries = match log.status {
        Status::Ok => {
            log.body_as::<JobLogEntries>()
                .map_err(|e| e.to_string())?
                .total
        }
        // Still on the job queue: no log yet.
        Status::NotFound => 0,
        _ => return Err(format!("JobLog: {}", brief(&log))),
    };
    let end = a.call(
        Op::EndJob,
        &EndJob {
            job: j.id.clone(),
            how: EndHow::Immediate,
            delay: None,
        },
    )?;
    status(&end, Status::Ok, "EndJob")?;
    let theirs = a.call(
        Op::ListJobs,
        &ListJobs {
            user: "QSECOFR".into(),
            status: None,
            start: 0,
            max: 0,
        },
    )?;
    status(&theirs, Status::NotAuthorized, "ListJobs of QSECOFR's jobs")?;
    let mine = a.call(
        Op::EndJob,
        &EndJob {
            job: own.clone(),
            how: EndHow::Immediate,
            delay: None,
        },
    )?;
    status(
        &mine,
        Status::InvalidRequest,
        "EndJob of the connection's job",
    )?;
    Ok(format!(
        "QAPDEVJOB {:?} listed, held, released, {entries} log entries, ended; QSECOFR's refused",
        j.state
    ))
}

/// `Sql` 12 and 13 over a table and index of the step's own.
pub fn catalog(a: &mut Conn) -> Result<String, String> {
    sql::run_or(
        a,
        "CREATE TABLE HLINV.QAPDEVT (ID INTEGER NOT NULL, NAME CHAR(10))",
        &["42710"],
    )?;
    sql::run_or(
        a,
        "CREATE UNIQUE INDEX HLINV.QAPDEVTI1 ON HLINV.QAPDEVT (NAME, ID)",
        &["42710"],
    )?;
    let ix: Indexes = a.ok(
        Op::ListIndexes,
        &ListIndexes {
            schema: "HLINV".into(),
            table: "QAPDEVT".into(),
        },
    )?;
    let i = ix
        .indexes
        .iter()
        .find(|i| i.name == "QAPDEVTI1")
        .ok_or_else(|| format!("ListIndexes: {ix:?}"))?;
    if !i.unique || i.columns != ["NAME", "ID"] {
        return Err(format!("QAPDEVTI1: {i:?}"));
    }
    let none = a.call(
        Op::ListIndexes,
        &ListIndexes {
            schema: "HLINV".into(),
            table: "QAPNOSUCH".into(),
        },
    )?;
    status(&none, Status::NotFound, "ListIndexes of a table not there")?;
    let r: Routines = a.ok(
        Op::ListRoutines,
        &ListRoutines {
            schema: "HLINV".into(),
            pattern: "%".into(),
            kinds: Vec::new(),
            start: 0,
            max: 5,
        },
    )?;
    let missing = a.call(
        Op::ListRoutines,
        &ListRoutines {
            schema: "QAPNOSUCH".into(),
            ..ListRoutines::default()
        },
    )?;
    status(
        &missing,
        Status::NotFound,
        "ListRoutines of a schema not there",
    )?;
    Ok(format!(
        "index QAPDEVTI1 (NAME, ID) unique; {} routines in HLINV",
        r.total
    ))
}
