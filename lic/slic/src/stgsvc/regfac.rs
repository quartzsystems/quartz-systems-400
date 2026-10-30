// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The registration facility in the service: the repository
//! `QUSRSYS/QUSEXRGOBJ` made at the first IPL, every job's questions
//! (the APIs', the panels', the servers'), and the commands
//! `ADDEXITPGM`, `RMVEXITPGM` and `WRKREGINF` (its text form).
//! Changing the repository takes `*ALLOBJ` and `*SECADM`.

use std::fmt::Write as _;

use ::regfac::{
    AddOpts, Answer, Desc, ExitProgram, MtAction, Query, RegFail, ThreadSafe, store as repo,
};
use cmd::block::Parms;
use sec::auth::principal::Principal;
use sec::profile::SpecialAuth;
use stg::proto::{CmdMsg, CmdReply};
use store::obj::ObjectId;
use store::txn::Txn;

use crate::ipl::system::System;

/// Make the repository, or add the shipped points it lacks.
pub fn install(txn: &mut Txn<'_>, qsys: ObjectId, stamp: &wm::obj::Stamp) -> Result<usize, String> {
    repo::install(txn, qsys, stamp).map_err(|e| e.to_string())
}

fn refused(e: &sec::SecError) -> RegFail {
    RegFail::id(e.msgid().unwrap_or("CPF2222"), e.to_string())
}

/// Answer `q` for `me` (the system when `None`): the repository read,
/// changed and written in one transaction, its audit entries recorded.
pub fn api(system: &mut System, me: Option<&Principal>, q: Query) -> Result<Answer, RegFail> {
    if q.changes()
        && let Some(p) = me
    {
        p.require(
            SpecialAuth::ALLOBJ.union(SpecialAuth::SECADM),
            "change the registration facility",
        )
        .map_err(|e| refused(&e))?;
    }
    let Some(s) = system.storage.as_mut() else {
        return Err(RegFail::unavailable("the object store is not open"));
    };
    let Some(qsys) = s.qsys else {
        return Err(RegFail::unavailable("the object store is not initialized"));
    };
    let mut failed = None;
    let out = crate::svc::db::with_txn(&mut s.store, |txn| {
        let run = || -> Result<::regfac::Outcome, RegFail> {
            let mut reg = repo::load(txn, qsys)?;
            let out = ::regfac::answer(&mut reg, q)?;
            if out.changed {
                repo::save(txn, qsys, &reg)?;
            }
            Ok(out)
        };
        run().map_err(|e| {
            let text = e.to_string();
            failed = Some(e);
            text
        })
    });
    match out {
        Ok(o) => {
            for e in o.audit {
                sec::audit::record(e);
            }
            Ok(o.answer)
        }
        Err(text) => Err(failed.unwrap_or_else(|| RegFail::unavailable(&text))),
    }
}

fn escape(e: &RegFail) -> CmdReply {
    CmdReply {
        text: format!("{} {}\n", e.msgid, e.detail),
        escape: Some(CmdMsg {
            msgid: e.msgid.clone(),
            kind: 7,
            data: e.data.clone(),
        }),
        ..CmdReply::default()
    }
}

fn plain(text: String) -> CmdReply {
    CmdReply {
        text,
        ..CmdReply::default()
    }
}

/// The actor of the command being run.
fn actor() -> Option<Principal> {
    super::security::actor()
}

/// `*LOW`, `*HIGH` or a number.
fn number(parms: &Parms, kwd: &str) -> i32 {
    match parms.name(kwd).as_str() {
        "*LOW" => ::regfac::LOW,
        "*HIGH" => ::regfac::HIGH,
        _ => parms
            .int(kwd)
            .and_then(|n| i32::try_from(n).ok())
            .unwrap_or(0),
    }
}

/// The program data element list: CCSID, length, data.
fn program_data(parms: &Parms) -> (i32, Vec<u8>) {
    let elems = parms.elems("PGMDTA");
    let get = |i: usize| {
        elems
            .get(i)
            .copied()
            .flatten()
            .map(cmd::Value::source)
            .unwrap_or_default()
    };
    let ccsid = match get(0).to_ascii_uppercase().as_str() {
        "*JOB" | "" => 0,
        n => n.parse().unwrap_or(0),
    };
    let text = get(2);
    let text = text.trim_matches('\'');
    if text.eq_ignore_ascii_case("*NONE") {
        return (ccsid, Vec::new());
    }
    let mut data = ::regfac::text::ebcdic(text);
    match get(1).to_ascii_uppercase().as_str() {
        "*CALC" | "" => {}
        n => {
            if let Ok(len) = n.parse::<usize>() {
                data.resize(len, 0x40);
            }
        }
    }
    (ccsid, data)
}

/// A system command of the facility, or `None` for another.
pub fn command(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    Some(match name {
        "ADDEXITPGM" => addexitpgm(system, parms),
        "RMVEXITPGM" => {
            let (point, format) = (parms.name("EXITPNT"), parms.name("FORMAT"));
            let n = number(parms, "PGMNBR");
            let q = Query::Remove {
                name: point.clone(),
                format: format.clone(),
                number: n,
            };
            match api(system, actor().as_ref(), q) {
                Ok(_) => plain(format!(
                    "Exit program {n} removed from exit point {point} format {format}.\n"
                )),
                Err(e) => escape(&e),
            }
        }
        "WRKREGINF" => wrkreginf(system, parms),
        _ => return None,
    })
}

fn addexitpgm(system: &mut System, parms: &Parms) -> CmdReply {
    let (point, format) = (parms.name("EXITPNT"), parms.name("FORMAT"));
    let (pgm, lib) = parms.qual("PGM");
    let lib = match lib.to_ascii_uppercase().as_str() {
        "" | "*CURLIB" => "QGPL".to_string(),
        l => l.to_string(),
    };
    let (ccsid, data) = program_data(parms);
    let text = parms.text("TEXT");
    let program = ExitProgram {
        number: number(parms, "PGMNBR"),
        pgm: pgm.to_ascii_uppercase(),
        lib,
        data,
        ccsid,
        threadsafe: ThreadSafe::of_special(&parms.name("THDSAFE")).unwrap_or_default(),
        mt_action: MtAction::of_special(&parms.name("MLTTHDACN")).unwrap_or_default(),
        desc: Desc::text(if text.eq_ignore_ascii_case("*BLANK") {
            ""
        } else {
            &text
        }),
    };
    let shown = format!("{}/{}", program.lib, program.pgm);
    let q = Query::Add {
        name: point.clone(),
        format: format.clone(),
        program,
        opts: AddOpts {
            replace: parms.yes("REPLACE"),
            create_point: parms.yes("CRTEXITPNT"),
        },
    };
    match api(system, actor().as_ref(), q) {
        Ok(Answer::Number { n }) => plain(format!(
            "Exit program {shown} added to exit point {point} format {format} as number {n}.\n"
        )),
        Ok(_) => plain(String::new()),
        Err(e) => escape(&e),
    }
}

/// `WRKREGINF` without a display: the points as a listing.
fn wrkreginf(system: &mut System, parms: &Parms) -> CmdReply {
    let which = parms.name("EXITPNT");
    let (name, w) = match which.as_str() {
        "*REGISTERED" | "" => ("*ALL".to_string(), 0),
        "*UNREGISTERED" => ("*ALL".to_string(), 1),
        "*ALL" => ("*ALL".to_string(), 2),
        n => (n.to_string(), 2),
    };
    let format = match parms.name("FORMAT") {
        f if f.is_empty() => "*ALL".to_string(),
        f => f,
    };
    let q = Query::List {
        name,
        format,
        which: w,
    };
    match api(system, None, q) {
        Ok(Answer::Points { points }) => {
            if points.is_empty() {
                return escape(&RegFail::id("CPF3CEC", "no exit points match"));
            }
            let mut out =
                String::from("Exit point            Format    Registered  Programs  Text\n");
            for p in &points {
                let _ = writeln!(
                    out,
                    "{:<20}  {:<8}  {:<10}  {:>8}  {}",
                    p.name,
                    p.format,
                    if p.registered { "*YES" } else { "*NO" },
                    p.programs.len(),
                    p.desc.as_text()
                );
                for g in &p.programs {
                    let _ = writeln!(out, "    {:>10}  {}/{}", g.number, g.lib, g.pgm);
                }
            }
            plain(out)
        }
        Ok(_) => plain(String::new()),
        Err(e) => escape(&e),
    }
}
