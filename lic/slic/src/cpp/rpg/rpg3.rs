// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CRTRPGPGM` and `CVTRPGSRC` (licensed program `QS00RPG`): an RPG III
//! member compiled through its conversion (`rpg3`) into a program, its
//! RPG/400 listing spooled; or converted into an ILE RPG member with
//! its conversion report.

use std::cell::RefCell;
use std::fmt::Write as _;

use cmd::block::Parms;
use ile::CreateData;
use qsname::{MemberSpec, Name};
use stg::proto::{CmdMsg, CmdReply};
use store::obj::JobHandle;

use super::super::direct::{in_store, lib_or};
use super::object::{self, Made, Shape};
use super::source;
use crate::ipl::system::{System, now_unix};

/// The product both commands belong to.
const PRODUCT: &str = "QS00RPG";

/// Refused unless `QS00RPG` is installed.
fn installed(system: &mut System, cmdname: &str) -> Option<CmdReply> {
    (!crate::stgsvc::licpgm::installed(system, PRODUCT, licpgm::OptionNo::BASE)).then(|| {
        super::super::clpgm::failed(
            cmdname,
            &format!("{} product {PRODUCT} option *BASE is not installed", super::super::lpcmd::NOT_INSTALLED),
        )
    })
}

/// `CRTRPGPGM`.
pub fn crtrpgpgm(system: &mut System, parms: &Parms) -> CmdReply {
    if let Some(r) = installed(system, "CRTRPGPGM") {
        return r;
    }
    let (name, lib) = parms.qual("PGM");
    let (sf, sfl) = parms.qual("SRCFILE");
    let srcfile = if sf.is_empty() { "QRPGSRC".to_string() } else { sf.to_ascii_uppercase() };
    let mbr = match parms.name("SRCMBR").as_str() {
        "" | "*PGM" => name.to_ascii_uppercase(),
        m => m.to_string(),
    };
    let lib_name = match super::super::clpgm::target_library(system, &lib.to_ascii_uppercase()) {
        Ok(l) => l,
        Err(e) => return super::super::clpgm::failed("CRTRPGPGM", &e),
    };
    let name = if name.is_empty() { mbr.clone() } else { name.to_ascii_uppercase() };
    let src = match in_store(system, |txn, qsys, names| {
        source::read(txn, qsys, names, &srcfile, &lib_or(&sfl.to_ascii_uppercase()), Some(&mbr))
    }) {
        Ok(s) => s,
        Err(e) => return super::super::clpgm::failed("CRTRPGPGM", &e),
    };
    let mut o = rpg3::Options {
        pgm: (name.clone(), lib_name.clone()),
        src: (src.member.lib.clone(), srcfile, mbr),
        ..rpg3::Options::default()
    };
    if let Some(g) = parms.int("GENLVL").and_then(|n| u8::try_from(n).ok()) {
        o.genlvl = g;
    }
    if parms.specified("OPTION") {
        o.option = parms.list("OPTION").iter().map(|v| v.source().to_ascii_uppercase()).collect();
    }
    if parms.specified("TEXT") {
        o.text = parms.text("TEXT");
    }
    o.replace = !parms.specified("REPLACE") || parms.yes("REPLACE");
    let text = if o.text == "*SRCMBRTXT" { src.text.clone() } else { o.text.clone() };
    let compiled = {
        let cell = RefCell::new(&mut *system);
        let mut includes = source::StoreIncludes { system: &cell, incdir: Vec::new() };
        let mut files = source::StoreFiles { system: &cell };
        rpg3::compile(&src.member, &mut includes, &mut files, &o)
    };
    let mut out = String::new();
    for d in &compiled.diags {
        let _ = writeln!(out, "{} {:02} statement {}: {}", d.msgid, d.severity, d.stmt, rpg3::crt::text(&d.msgid));
    }
    let _ = match crate::svc::security::print_as(system, &src.member.name, &name, &compiled.listing, true) {
        Ok(w) => writeln!(out, "Listing: {w}"),
        Err(e) => writeln!(out, "The listing was not spooled: {e}"),
    };
    let refused = |out: &str| CmdReply {
        text: format!("{out}QRG9001 Program {name} was not created in library {lib_name}.\n"),
        escape: Some(CmdMsg {
            msgid: "QRG9001".into(),
            kind: 7,
            data: cmd::error::data(&[(name.as_str(), 10), (lib_name.as_str(), 10)]),
        }),
        ..CmdReply::default()
    };
    if !compiled.created {
        return refused(&out);
    }
    let made = Made {
        shape: Shape::Program,
        name: name.clone(),
        text,
        replace: o.replace,
        usrprf_owner: parms.name("USRPRF") == "*OWNER",
        bnddir: Vec::new(),
        source: None,
    };
    let create = CreateData {
        compiler: "RPG".into(),
        options: o.option.join(" "),
        src_file: src.member.file.clone(),
        src_lib: src.member.lib.clone(),
        src_mbr: src.member.name.clone(),
    };
    let stored = crate::svc::ile::commands::in_store(system, &lib_name, |txn, c, lib_id| {
        object::make(txn, c, lib_id, &made, &compiled.inner.qir, create, now_unix())
    });
    if let Err(e) = stored {
        return refused(&format!("{out}{e}\n"));
    }
    let sev = format!("{:02}", compiled.max_severity);
    CmdReply {
        text: format!("{out}Program {name} created in library {lib_name}. Highest severity: {sev}.\n"),
        messages: vec![CmdMsg {
            msgid: "QRG0003".into(),
            kind: 4,
            data: cmd::error::data(&[(name.as_str(), 10), (lib_name.as_str(), 10), (sev.as_str(), 2)]),
        }],
        ..CmdReply::default()
    }
}

/// The job's lock owner handle.
fn job_handle(system: &System) -> JobHandle {
    JobHandle(system.jobs.get(system.current_job).map_or(u64::from(system.current_job), |j| j.handle))
}

/// `CVTRPGSRC`: one member, or every member (`FROMMBR(*ALL)`).
pub fn cvtrpgsrc(system: &mut System, parms: &Parms) -> CmdReply {
    if let Some(r) = installed(system, "CVTRPGSRC") {
        return r;
    }
    let (ff, ffl) = parms.qual("FROMFILE");
    let (tf, tfl) = parms.qual("TOFILE");
    let from = (lib_or(&ffl.to_ascii_uppercase()), ff.to_ascii_uppercase());
    let to = (
        lib_or(&tfl.to_ascii_uppercase()),
        if tf.is_empty() { "QRPGLESRC".to_string() } else { tf.to_ascii_uppercase() },
    );
    let from_mbr = parms.name("FROMMBR");
    let to_mbr = parms.name("TOMBR");
    let to_mbr = if to_mbr.is_empty() { "*FROMMBR".to_string() } else { to_mbr };
    let cvtrpt = !parms.specified("CVTRPT") || parms.yes("CVTRPT");
    let user = system.jobs.get(system.current_job).map(|j| j.user.clone()).unwrap_or_default();
    let job = job_handle(system);
    let now = now_unix();
    // The members to convert, then each converted.
    let members = in_store(system, |txn, qsys, names| {
        let f = wm::obj::find(txn, qsys, names, &from.0, &from.1, store::obj::types::FILE)
            .map_err(|_| format!("CPF9812 file {} in library {} not found", from.1, from.0))?;
        let d = db::file::describe(txn, f).map_err(|e| e.to_string())?;
        let all: Vec<String> = d.members.iter().map(|m| m.name.to_string()).collect();
        Ok(match from_mbr.as_str() {
            "*ALL" => all,
            "" => return Err("CPD0071 FROMMBR is required".into()),
            m if all.iter().any(|a| a == m) => vec![m.to_string()],
            m => return Err(format!("CPF9815 member {m} not found in file {}", from.1)),
        })
    });
    let members = match members {
        Ok(m) => m,
        Err(e) => return super::super::clpgm::failed("CVTRPGSRC", &e),
    };
    let mut out = String::new();
    let mut converted = 0;
    for m in &members {
        let target = if to_mbr == "*FROMMBR" { m.clone() } else { to_mbr.clone() };
        let src = match in_store(system, |txn, qsys, names| source::read(txn, qsys, names, &from.1, &from.0, Some(m))) {
            Ok(s) => s,
            Err(e) => return super::super::clpgm::failed("CVTRPGSRC", &e),
        };
        let lines: Vec<String> = src.member.records.iter().map(|r| r.text.clone()).collect();
        let c = rpg3::convert(&lines);
        let text = c.text().join("\n");
        let srctext = src.text.clone();
        let put = in_store(system, |txn, qsys, names| {
            let t = wm::obj::find(txn, qsys, names, &to.0, &to.1, store::obj::types::FILE)
                .map_err(|_| format!("CPF9812 file {} in library {} not found", to.1, to.0))?;
            let tname = Name::parse(&target).map_err(|e| format!("CPD0078 TOMBR({target}): {e}"))?;
            let tdesc = db::file::describe(txn, t).map_err(|e| e.to_string())?;
            let rpgle = Name::parse("RPGLE").ok();
            if tdesc.members.iter().any(|x| x.name == tname) {
                return Err(format!("RNM9002 member {target} already exists in {}/{}", to.0, to.1));
            }
            db::file::add_member(txn, t, tname, &srctext, rpgle, now)
                .map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPF3204")))?;
            db::source::srcput(txn, job, t, MemberSpec::Name(tname), &text, rpgle, now)
                .map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPF3204")))
        });
        match put {
            Ok(n) => {
                converted += 1;
                let _ = writeln!(out, "Member {m} converted: {n} records into {}/{}({target}).", to.0, to.1);
            }
            Err(e) => return super::super::clpgm::failed("CVTRPGSRC", &format!("{out}{e}")),
        }
        if cvtrpt {
            let p = rpg3::report::Params {
                user: user.clone(),
                from: (from.0.clone(), from.1.clone(), m.clone()),
                to: (to.0.clone(), to.1.clone(), to_mbr.clone()),
                ..rpg3::report::Params::default()
            };
            let rpt = rpg3::report::report(&c, &p);
            let _ = match crate::svc::security::print_as(system, "QSYSPRT", m, &rpt, true) {
                Ok(w) => writeln!(out, "Report: {w}"),
                Err(e) => writeln!(out, "The report was not spooled: {e}"),
            };
        }
    }
    CmdReply {
        text: out,
        messages: vec![CmdMsg {
            msgid: "RNM9001".into(),
            kind: 4,
            data: cmd::error::data(&[(&converted.to_string(), 10), (&to.0, 10), (&to.1, 10), (&to_mbr, 10)]),
        }],
        ..CmdReply::default()
    }
}
