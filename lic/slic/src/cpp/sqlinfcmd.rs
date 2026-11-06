// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The SQL information of programs made with embedded SQL: kept as
//! `CRTSQLRPGI` makes the program, printed by `PRTSQLINF`
//! (`sql::prtsqlinf`).

use cmd::block::Parms;
use db::sqlinf::{Kind, SqlInfo, tag};
use qsname::Name;
use sql::prtsqlinf::{Heading, Precompile};
use stg::proto::{CmdMsg, CmdReply};
use store::obj::library::Library;
use store::obj::types;

use super::direct::in_store;
use crate::ipl::system::{System, now_unix};

/// Run `name`, or `None` when it is not one of these.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    (name == "PRTSQLINF").then(|| prtsqlinf(system, parms))
}

/// Keep `pre`'s statements and options with the object `lib/name` just
/// made (`ty` its type).
pub fn keep(system: &mut System, lib: &str, name: &str, ty: u16, pre: &rpgc::sqlpre::Precompiled, p: &Precompile) -> Result<(), String> {
    let mut info = SqlInfo::new(Kind::Program);
    info.set(tag::OPTIONS, db::sqlinf::routine::encode_options(&sql::prtsqlinf::options(p)));
    for s in sql::prtsqlinf::entries(&pre.texts) {
        info.push(tag::STATEMENT, s.encode());
    }
    let (lib, name) = (lib.to_string(), name.to_string());
    in_store(system, |txn, qsys, names| {
        let lid = super::direct::library(txn, qsys, names, &lib)?;
        let n = Name::parse(&name).map_err(|e| e.to_string())?;
        let id = Library::load(txn, lid)
            .and_then(|l| l.lookup(txn, ty, n))
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("CPF9801 {}", sql::prtsqlinf::not_found(&name, &lib)))?;
        db::sqlinf::save(txn, id, &info, now_unix()).map_err(|e| e.to_string())
    })
}

fn msg(id: &str, kind: u8, fields: &[(&str, usize)]) -> CmdMsg {
    CmdMsg { msgid: id.into(), kind, data: cmd::error::data(fields) }
}

/// The diagnostic `id` with its data, then `SQL9011`.
fn failed(id: &str, text: &str, fields: &[(&str, usize)]) -> CmdReply {
    CmdReply {
        text: format!("{id} {text}\nSQL9011\n"),
        messages: vec![msg(id, 2, fields)],
        escape: Some(msg("SQL9011", 7, &[])),
        ..CmdReply::default()
    }
}

fn prtsqlinf(system: &mut System, parms: &Parms) -> CmdReply {
    let (obj, lib) = parms.qual("OBJ");
    let obj = obj.to_ascii_uppercase();
    let lib = if lib.is_empty() { "*LIBL".to_string() } else { lib.to_ascii_uppercase() };
    let objtype = {
        let t = parms.name("OBJTYPE");
        if t.is_empty() { "*PGM".to_string() } else { t }
    };
    if obj == "*JOB" {
        // BLOCKED(sql): the plan cache (`sql::exec::plan::cache`) keeps
        // statements by text for every job, none as one job's to print.
        return super::escape("SQL9011", "PRTSQLINF", &sql::prtsqlinf::refusal("OBJ(*JOB)"));
    }
    let ty = match objtype.as_str() {
        "*PGM" => types::PGM,
        "*SRVPGM" => types::SRVPGM,
        // BLOCKED(sql): a package's statements are kept (`db::sqlpkg`)
        // but not yet printed.
        _ => return super::escape("SQL9011", "PRTSQLINF", &sql::prtsqlinf::refusal("OBJTYPE(*SQLPKG)")),
    };
    let found = in_store(system, |txn, qsys, names| {
        let lid = match lib.as_str() {
            "*LIBL" => None,
            l => Some(super::direct::library(txn, qsys, names, l).map_err(|_| "LIB".to_string())?),
        };
        let n = Name::parse(&obj).map_err(|_| "OBJ".to_string())?;
        let libs: Vec<_> = match lid {
            Some(l) => vec![l],
            None => names.curlib.into_iter().chain(names.libl.iter().copied()).collect(),
        };
        for l in libs {
            let lt = Library::load(txn, l).map_err(|e| e.to_string())?;
            if let Some(id) = lt.lookup(txn, ty, n).map_err(|e| e.to_string())? {
                let lname = store::obj::ops::describe(txn, l).map_err(|e| e.to_string())?.name.to_string();
                return Ok(Some((lname, db::sqlinf::load(txn, id).map_err(|e| e.to_string())?)));
            }
        }
        Ok(None)
    });
    let (lname, info) = match found {
        Err(e) if e == "LIB" => return failed("SQL0204", &sql::prtsqlinf::not_found(&lib, "QSYS"), &[(&lib, 30), ("QSYS", 10)]),
        Err(e) => return super::direct::failed("PRTSQLINF", &e),
        Ok(None) => return failed("SQL0204", &sql::prtsqlinf::not_found(&obj, &lib), &[(&obj, 30), (&lib, 10)]),
        Ok(Some(f)) => f,
    };
    let Some(info) = info.filter(|i| i.kind == Kind::Program) else {
        return failed("SQL5062", &sql::prtsqlinf::no_statements(&obj, &lname, &objtype), &[(&obj, 10), (&lname, 10), (&objtype, 7)]);
    };
    let options = info.first(tag::OPTIONS).and_then(|b| db::sqlinf::routine::decode_options(b).ok()).unwrap_or_default();
    let stmts: Vec<_> = info.all(tag::STATEMENT).filter_map(|b| db::sqlinf::program::StmtDef::decode(b).ok()).collect();
    let (date, time) = super::rpg::stamp_dashed(crate::stgsvc::perfcol::local_now(system));
    let h = Heading {
        product: release::PRODUCT_ID.into(),
        release: release::RELEASE.to_string(),
        object: format!("{lname}/{obj}"),
        objtype: objtype.clone(),
        date,
        time,
        command: "CRTSQLRPGI".into(),
        ccsid: 37,
    };
    let lines = sql::prtsqlinf::render(&h, &options, &stmts);
    match crate::svc::security::print_as(system, &obj, "PRTSQLINF", &lines, true) {
        Ok(w) => super::direct::plain(format!("{w}\n")),
        Err(e) => super::direct::failed("PRTSQLINF", &e),
    }
}
