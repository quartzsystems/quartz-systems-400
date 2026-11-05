// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `OBJECT_LOCK_INFO`: the store's lock table, as `WRKOBJLCK` shows
//! it. Every lock QS/400 keeps is held (none waits in the table) and
//! scoped to the job; the program that took it is not recorded.

use store::obj::{JobHandle, ObjectId, ops, types};
use store::txn::Txn;

use super::super::super::SqlContext;
use super::{Args, Out};
use crate::error::Result;

/// One lock: library, object, member, type, state, job, count.
struct Lock {
    lib: String,
    name: String,
    member: Option<String>,
    type_name: &'static str,
    state: &'static str,
    job: JobHandle,
    count: usize,
}

/// Every lock, a member's under its file, the same state of one job
/// counted once.
fn locks(txn: &mut Txn<'_>) -> Vec<Lock> {
    let ids: Vec<ObjectId> = txn.store().locks().locked_objects();
    let mut out: Vec<Lock> = Vec::new();
    for id in ids {
        let holders = txn.store().locks().holders(id);
        let Ok((_, h)) = ops::read_header(txn, id) else { continue };
        let (lib, name, member, code) = if id.type_code == types::MBR {
            let Ok((_, f)) = ops::read_header(txn, h.library) else { continue };
            let Ok((_, l)) = ops::read_header(txn, f.library) else { continue };
            (l.name.to_string(), f.name.to_string(), Some(h.name.to_string()), types::FILE)
        } else if id.type_code == types::LIB {
            ("QSYS".to_string(), h.name.to_string(), None, types::LIB)
        } else {
            let Ok((_, l)) = ops::read_header(txn, h.library) else { continue };
            (l.name.to_string(), h.name.to_string(), None, id.type_code)
        };
        let type_name = types::type_by_code(code).map_or("", |t| t.name);
        for held in holders {
            let state = held.state.name();
            if let Some(l) = out.iter_mut().find(|l| l.job == held.job && l.state == state && l.lib == lib && l.name == name && l.member == member) {
                l.count += 1;
                continue;
            }
            out.push(Lock { lib: lib.clone(), name: name.clone(), member: member.clone(), type_name, state, job: held.job, count: 1 });
        }
    }
    out.sort_by(|a, b| (&a.lib, &a.name, &a.member, a.job).cmp(&(&b.lib, &b.name, &b.member, b.job)));
    out
}

/// The job columns of a lock.
fn job(out: &mut Out, h: JobHandle) {
    let j = jrn::JobInfo::from_handle(h);
    out.text("JOB_NAME", j.qualified());
    out.text("JOB_NAME_SHORT", j.name.as_str());
    out.text("JOB_USER", j.user.as_str());
    out.text("JOB_NUMBER", format!("{:06}", j.number));
}

/// The columns both forms have.
fn lock_columns(out: &mut Out, l: &Lock) {
    if l.member.is_some() {
        out.text("MEMBER_LOCK_TYPE", "MEMBER");
    }
    out.text("LOCK_STATE", l.state);
    out.text("LOCK_STATUS", "HELD");
    out.text("LOCK_SCOPE", "JOB");
    out.int("LOCK_COUNT", l.count);
    job(out, l.job);
}

/// The view.
pub fn view(txn: &mut Txn<'_>, _ctx: &SqlContext, out: &mut Out) -> Result<()> {
    for l in locks(txn) {
        out.row();
        out.text("OBJECT_SCHEMA", l.lib.as_str());
        out.text("OBJECT_NAME", l.name.as_str());
        out.text("SYSTEM_OBJECT_SCHEMA", l.lib.as_str());
        out.text("SYSTEM_OBJECT_NAME", l.name.as_str());
        if let Some(m) = &l.member {
            out.text("SYSTEM_TABLE_MEMBER", m.as_str());
        }
        out.text("OBJECT_TYPE", l.type_name);
        out.int("ASP_NUMBER", 1);
        out.text("ASPGRP", "*SYSBAS");
        lock_columns(out, &l);
    }
    Ok(())
}

/// The table function: one object's locks.
pub fn function(txn: &mut Txn<'_>, _ctx: &SqlContext, args: &Args, out: &mut Out) -> Result<()> {
    let lib = args.word("LIBRARY_NAME").unwrap_or_default();
    let name = args.word("OBJECT_NAME").unwrap_or_default();
    let ty = args.word("OBJECT_TYPE").unwrap_or_default();
    for l in locks(txn) {
        if l.lib != lib || l.name != name || (!ty.is_empty() && l.type_name != ty) {
            continue;
        }
        out.row();
        if let Some(m) = &l.member {
            out.text("MEMBER_NAME", m.as_str());
        }
        out.int("IASP_NUMBER", 1);
        out.text("ASPGRP", "*SYSBAS");
        lock_columns(out, &l);
    }
    Ok(())
}
