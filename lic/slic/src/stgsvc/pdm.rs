// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! PDM's storage requests (320-339): a source member read or written
//! whole (find string, the user-defined options file) and each user's
//! session defaults, kept as a member named for the user in
//! `QUSRSYS/QAUODFT`.

use db::file::{self, CreateFileOpts};
use qsname::{MemberSpec, Name};

use crate::ipl::system::System;
use crate::svc::db::{JOB, db, db_err, file_id, now, object_name, with_txn};

/// The file holding every user's defaults.
const DEFAULTS: (&str, &str) = ("QUSRSYS", "QAUODFT");

/// The option file every user starts with; written by the system for
/// anyone, as the shared file of a shipped system is.
const SHARED_OPTIONS: (&str, &str) = ("QGPL", "QAUOOPT");

/// Record length of the files PDM creates: room for a 255-character
/// command after the option code.
const RCDLEN: u32 = 280;

fn member_name(m: &str) -> Result<Name, String> {
    Name::parse(m.trim()).map_err(|e| format!("CPD0078 {m}: {e}"))
}

/// A member's lines; `None` when the file or the member does not exist.
pub fn read(
    system: &mut System,
    lib: &str,
    file: &str,
    member: &str,
) -> Result<Option<Vec<String>>, String> {
    let m = member_name(member)?;
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let Ok(id) = file_id(txn, ctx, &format!("{lib}/{file}")) else {
            return Ok(None);
        };
        let desc = file::describe(txn, id).map_err(|e| db_err(&e))?;
        if desc.member(MemberSpec::Name(m)).is_none() {
            return Ok(None);
        }
        db::source::srcget(txn, id, MemberSpec::Name(m))
            .map(|t| Some(t.lines().map(str::to_string).collect()))
            .map_err(|e| db_err(&e))
    })
}

/// Replace a member's lines, the source file and the member made when
/// missing.
pub fn write(
    system: &mut System,
    lib: &str,
    file: &str,
    member: &str,
    lines: &[String],
    srctype: &str,
) -> Result<(), String> {
    let m = member_name(member)?;
    let ty = if srctype.trim().is_empty() {
        None
    } else {
        Some(member_name(srctype)?)
    };
    let qual = format!("{lib}/{file}");
    let name = object_name(&qual)?;
    let text: String = lines.iter().flat_map(|l| [l.as_str(), "\n"]).collect();
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let id = if let Ok(id) = file_id(txn, ctx, &qual) {
            id
        } else {
            let l = ctx.library_for(txn, &name).map_err(|e| e.to_string())?;
            let f = member_name(file)?;
            let opts = CreateFileOpts {
                now: now(),
                text: "PDM".into(),
                max_members: 0,
                member: file::MemberOpt::None,
                ..CreateFileOpts::default()
            };
            file::create_srcpf(txn, l, f, RCDLEN, &opts).map_err(|e| db_err(&e))?
        };
        let desc = file::describe(txn, id).map_err(|e| db_err(&e))?;
        if desc.member(MemberSpec::Name(m)).is_none() {
            file::add_member(txn, id, m, "", ty, now()).map_err(|e| db_err(&e))?;
        }
        db::source::srcput(txn, JOB, id, MemberSpec::Name(m), &text, ty, now())
            .map(|_| ())
            .map_err(|e| db_err(&e))
    })
}

/// Whether the file is the shared option file.
fn shared(lib: &str, file: &str) -> bool {
    lib.eq_ignore_ascii_case(SHARED_OPTIONS.0) && file.eq_ignore_ascii_case(SHARED_OPTIONS.1)
}

/// An option file's member: the shared one as the system reads it.
pub fn read_options(
    system: &mut System,
    lib: &str,
    file: &str,
    member: &str,
) -> Result<Option<Vec<String>>, String> {
    if shared(lib, file) {
        sec::guard::as_system(|| read(system, lib, file, member))
    } else {
        read(system, lib, file, member)
    }
}

/// An option file's member written: the shared one by the system.
pub fn write_options(
    system: &mut System,
    lib: &str,
    file: &str,
    member: &str,
    lines: &[String],
) -> Result<(), String> {
    if shared(lib, file) {
        sec::guard::as_system(|| write(system, lib, file, member, lines, "TXT"))
    } else {
        write(system, lib, file, member, lines, "TXT")
    }
}

/// The user the request runs for.
fn user() -> Result<String, String> {
    sec::guard::current()
        .map(|s| s.borrow().principal.user.clone())
        .ok_or_else(|| "no user".to_string())
}

/// The job's user's defaults as stored, blank when none.
pub fn defaults(system: &mut System) -> String {
    let Ok(u) = user() else {
        return String::new();
    };
    sec::guard::as_system(|| read(system, DEFAULTS.0, DEFAULTS.1, &u))
        .ok()
        .flatten()
        .map(|l| l.join("\n"))
        .unwrap_or_default()
}

/// Store the job's user's defaults.
pub fn put_defaults(system: &mut System, text: &str) -> Result<(), String> {
    let u = user()?;
    let lines: Vec<String> = text.lines().map(str::to_string).collect();
    sec::guard::as_system(|| write(system, DEFAULTS.0, DEFAULTS.1, &u, &lines, "TXT"))
}
