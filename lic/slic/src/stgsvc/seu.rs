// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SEU's source members (requests 300-302): a member read whole with a
//! lock on it (`*SHRRD` to browse, `*EXCL` to edit, refused while
//! another job holds it the other way), written back, released.

use qsname::{MemberSpec, Name};
use store::obj::{JobHandle, LockState, ObjectId, types};
use store::txn::Txn;
use stg::proto::SrcMember;

use crate::ipl::system::System;

/// The source file and its real library.
fn source_file(txn: &mut Txn<'_>, qsys: ObjectId, lib: &str, file: &str) -> Result<(ObjectId, String), String> {
    let names = crate::svc::ile::name_context(txn, qsys);
    let id = wm::obj::find(txn, qsys, &names, lib, file, types::FILE)
        .map_err(|_| format!("CPF9812 File {file} in library {lib} not found."))?;
    let real = ile::store::library_name(txn, id).map_err(|e| e.to_string())?;
    Ok((id, real))
}

fn member_name(member: &str) -> Result<Name, String> {
    Name::parse(member).map_err(|e| format!("CPD0078 {member}: {e}"))
}

fn db_err(e: &db::DbError) -> String {
    format!("{} {e}", e.msgid().unwrap_or("CPF9898"))
}

/// Open a member for browsing or editing. A member that does not exist
/// opens empty for editing (it is added when saved).
pub fn open(
    system: &mut System,
    job: JobHandle,
    (lib, file, member): (&str, &str, &str),
    edit: bool,
) -> Result<SrcMember, String> {
    let m = member_name(member)?;
    crate::stgsvc::spool::with_store(system, |txn, qsys| {
        let (fid, real) = source_file(txn, qsys, lib, file)?;
        if edit {
            db::auth::check(txn, "file.open-output", fid).map_err(|e| db_err(&e))?;
        }
        let found = match db::source::read_member(txn, fid, MemberSpec::Name(m)) {
            Ok(sm) => sm,
            Err(db::DbError::MemberNotFound { .. }) if edit => {
                let width = db::source::data_width(txn, fid).map_err(|e| db_err(&e))?;
                return Ok(SrcMember {
                    lib: real,
                    file: file.to_ascii_uppercase(),
                    member: m.to_string(),
                    srctype: String::new(),
                    text: String::new(),
                    width: u32::try_from(width).unwrap_or(80),
                    exists: false,
                    records: Vec::new(),
                });
            }
            Err(e) => return Err(db_err(&e)),
        };
        let state = if edit {
            LockState::Exclusive
        } else {
            LockState::SharedRead
        };
        if txn.store().locks_mut().lock(job, found.object, state).is_err() {
            return Err(format!(
                "CPF3330 Member {m} in {real}/{} is held by another job.",
                file.to_ascii_uppercase()
            ));
        }
        Ok(SrcMember {
            lib: real,
            file: file.to_ascii_uppercase(),
            member: m.to_string(),
            srctype: found.srctype,
            text: found.text,
            width: u32::try_from(found.width).unwrap_or(80),
            exists: true,
            records: found.records,
        })
    })
}

/// Write a member's records back, adding it when new.
pub fn save(
    system: &mut System,
    job: JobHandle,
    (lib, file, member): (&str, &str, &str),
    records: &[(u32, u32, String)],
    srctype: &str,
    text: &str,
) -> Result<(), String> {
    let m = member_name(member)?;
    let st = if srctype.trim().is_empty() {
        None
    } else {
        Some(Name::parse(srctype.trim()).map_err(|e| format!("CPD0078 {srctype}: {e}"))?)
    };
    let now = crate::stgsvc::msgs::now_micros() / 1_000_000;
    crate::stgsvc::spool::with_store(system, |txn, qsys| {
        let (fid, _) = source_file(txn, qsys, lib, file)?;
        db::source::write_member(txn, job, fid, m, records, st, text, now).map_err(|e| db_err(&e))?;
        Ok(())
    })
}

/// Release what `open` took on a member.
pub fn close(system: &mut System, job: JobHandle, (lib, file, member): (&str, &str, &str)) {
    let Ok(m) = member_name(member) else {
        return;
    };
    let _ = crate::stgsvc::spool::with_store(system, |txn, qsys| {
        let (fid, _) = source_file(txn, qsys, lib, file)?;
        let desc = db::file::describe(txn, fid).map_err(|e| db_err(&e))?;
        if let Some(e) = desc.member(MemberSpec::Name(m)).copied() {
            let locks = txn.store().locks_mut();
            locks.unlock(job, e.object, LockState::Exclusive);
            locks.unlock(job, e.object, LockState::SharedRead);
        }
        Ok(())
    });
}
