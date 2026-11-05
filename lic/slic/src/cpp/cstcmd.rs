// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The physical file constraint commands on `db::cst::ops`: `ADDPFCST`,
//! `RMVPFCST`, `CHGPFCST`, `DSPCPCST`, and the text forms of
//! `WRKPFCST` and `EDTCPCST` (BLOCKED(sql): their list panels are not
//! built yet; at a display the job shows this text). The messages are
//! the primary's (`tools/conformance/sql/oracle/integrity/primary-facts.md`).

#![allow(clippy::many_single_char_names)]

use std::fmt::Write as _;

use cmd::block::Parms;
use db::cst::ops::{self, NewCst, Pick};
use db::cst::{Constraint, Kind, Rule};
use qsname::Name;
use stg::proto::{CmdMsg, CmdReply};
use store::obj::{ObjectId, ops as objops, types};
use store::txn::Txn;

use super::direct::{in_store, lib_or};
use crate::ipl::system::{System, now_unix};

/// Run `name`, or `None` when it is not one of these.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    Some(match name {
        "ADDPFCST" => add(system, parms),
        "RMVPFCST" => remove(system, parms),
        "CHGPFCST" => change(system, parms),
        "DSPCPCST" => display(system, parms),
        "WRKPFCST" => work(system, parms),
        "EDTCPCST" => pending(system),
        _ => return None,
    })
}

fn msg(id: &str, kind: u8, fields: &[(&str, usize)]) -> CmdMsg {
    CmdMsg { msgid: id.into(), kind, data: cmd::error::data(fields) }
}

/// Diagnostics, then the escape `esc` naming the file.
fn refused(diags: Vec<CmdMsg>, esc: &str, file: &str, text: &str) -> CmdReply {
    CmdReply {
        text: format!("{esc} {text}\n"),
        messages: diags,
        escape: Some(msg(esc, 7, &[(file, 10)])),
        ..CmdReply::default()
    }
}

fn file_of(txn: &mut Txn<'_>, qsys: ObjectId, names: &objops::NameContext, q: &(String, String)) -> Result<ObjectId, String> {
    wm::obj::find(txn, qsys, names, &lib_or(&q.1), &q.0, types::FILE)
        .map_err(|_| format!("CPF9812 file {} in library {} not found", q.0, lib_or(&q.1)))
}

/// A constraint name as given: upper case unless quoted.
fn cst_name(raw: &str) -> String {
    let t = raw.trim();
    match t.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        Some(q) => q.to_string(),
        None => t.to_ascii_uppercase(),
    }
}

fn names(parms: &Parms, kw: &str) -> Result<Vec<Name>, String> {
    parms
        .list(kw)
        .iter()
        .map(|v| Name::parse(&v.source().to_ascii_uppercase()).map_err(|e| format!("CPD0078 {kw}: {e}")))
        .collect()
}

fn kind(word: &str) -> Option<Kind> {
    Some(match word {
        "*UNQCST" => Kind::Unique,
        "*PRIKEY" => Kind::PrimaryKey,
        "*CHKCST" => Kind::Check,
        "*REFCST" => Kind::Referential,
        _ => return None,
    })
}

/// The database's refusal as a diagnostic: the IBM message where the
/// primary gives one, else the error's own.
fn diag(e: &db::DbError, cst: &str, file: &str) -> CmdMsg {
    match e {
        db::DbError::Constraint { reason: ops::Refusal::NotFound, .. } => msg("CPD32B1", 5, &[(cst, 258), (file, 10)]),
        other => msg(other.msgid().unwrap_or("CPF9898"), 5, &[(&other.to_string(), 256)]),
    }
}

/// `ADDPFCST`.
fn add(system: &mut System, parms: &Parms) -> CmdReply {
    let q = parms.qual("FILE");
    let file_name = q.0.clone();
    let ty = parms.name("TYPE");
    let r = in_store(system, |txn, qsys, nm| {
        let id = file_of(txn, qsys, nm, &q)?;
        let k = kind(&ty).ok_or_else(|| format!("CPD0084 TYPE({ty})"))?;
        let mut n = NewCst::new(k, if k == Kind::Check { Vec::new() } else { names(parms, "KEY")? });
        n.pending_ok = true;
        let c = parms.text("CST");
        if !c.is_empty() && !c.eq_ignore_ascii_case("*GEN") {
            n.name = Some(cst_name(&c));
        }
        if k == Kind::Referential {
            let p = parms.qual("PRNFILE");
            n.parent = file_of(txn, qsys, nm, &p)?;
            if parms.name("PRNKEY") != "*PRNFILE" {
                n.parent_columns = names(parms, "PRNKEY")?;
            }
            n.delete_rule = Rule::parse(&parms.name("DLTRULE")).unwrap_or(Rule::NoAction);
            n.update_rule = Rule::parse(&parms.name("UPDRULE")).unwrap_or(Rule::NoAction);
        }
        if k == Kind::Check {
            let text = parms.text("CHKCST");
            let e = sql::parse::parse_expr(&text, sql::Naming::Sys).map_err(|e| format!("{} {e}", e.msgid()))?;
            let rf = db::file::describe(txn, id).map_err(|e| e.to_string())?.formats[0].format.clone();
            let prog = sql::exec::cst::compile_check(&e, &rf, n.name.as_deref().unwrap_or("*N")).map_err(|e| format!("CPF32B0 {e}"))?;
            n.check = Some((text, prog));
        }
        if k.keyed() && n.columns.is_empty() {
            return Err("CPD0071 KEY is needed for this constraint type".into());
        }
        ops::add(txn, id, &n, now_unix()).map_err(|e| {
            let d = diag(&e, n.name.as_deref().unwrap_or("*GEN"), &file_name);
            format!("\u{1}{}\u{1}{}", d.msgid, e)
        })
    });
    match r {
        Ok(report) => {
            let mut messages: Vec<CmdMsg> = report.names.iter().map(|n| msg("CPI32B0", 4, &[(n, 258), (&file_name, 10)])).collect();
            for (id, data) in &report.messages {
                messages.push(match *id {
                    "CPI32B4" | "CPI32B5" => msg(id, 4, &[(data, 10)]),
                    "CPD32C5" => msg(id, 5, &[(data, 258)]),
                    _ => msg(id, 5, &[(data, 10)]),
                });
            }
            messages.push(msg("CPC32B0", 4, &[("1", 4), (&file_name, 10)]));
            let mut text = String::new();
            for m in &messages {
                let _ = writeln!(text, "{}", m.msgid);
            }
            CmdReply { text, messages, ..CmdReply::default() }
        }
        Err(e) => {
            let (id, detail) = e.strip_prefix('\u{1}').and_then(|s| s.split_once('\u{1}')).map_or_else(
                || (super::leading_msgid(&e).unwrap_or("CPF9898").to_string(), e.clone()),
                |(a, b)| (a.to_string(), b.to_string()),
            );
            refused(vec![msg(&id, 5, &[(&detail, 256)])], "CPF32B0", &file_name, &detail)
        }
    }
}

fn pick(parms: &Parms, kw: &str) -> Pick {
    let raw = parms.text(kw);
    let types = parms.name("TYPE");
    Pick {
        name: match raw.to_ascii_uppercase().as_str() {
            "*ALL" | "*CHKPND" => None,
            _ => Some(cst_name(&raw)),
        },
        kinds: kind(&types).into_iter().collect(),
    }
}

/// `RMVPFCST`: what cannot be removed is a diagnostic, not an escape.
fn remove(system: &mut System, parms: &Parms) -> CmdReply {
    let q = parms.qual("FILE");
    let file_name = q.0.clone();
    let p = pick(parms, "CST");
    let pending_only = parms.name("CST") == "*CHKPND";
    let cascade = parms.name("RMVCST") == "*REMOVE";
    let r = in_store(system, |txn, qsys, nm| {
        let id = file_of(txn, qsys, nm, &q)?;
        if pending_only {
            let names: Vec<String> = db::cst::list(txn, id).map_err(|e| e.to_string())?.into_iter().filter(|c| c.check_pending).map(|c| c.name).collect();
            let mut all = ops::Removed::default();
            for n in names {
                let one = Pick { name: Some(n), kinds: Vec::new() };
                let r = ops::remove(txn, id, &one, cascade, now_unix()).map_err(|e| e.to_string())?;
                all.removed.extend(r.removed);
                all.refused.extend(r.refused);
            }
            return Ok(all);
        }
        ops::remove(txn, id, &p, cascade, now_unix()).map_err(|e| e.to_string())
    });
    match r {
        Ok(done) => {
            let mut messages: Vec<CmdMsg> = done.refused.iter().map(|n| msg("CPD32B1", 5, &[(n, 258), (&file_name, 10)])).collect();
            messages.extend(done.removed.iter().map(|n| msg("CPI32B1", 4, &[(n, 258), (&file_name, 10)])));
            let count = done.removed.len().to_string();
            messages.push(msg("CPC32B1", 4, &[(&count, 4), (&file_name, 10)]));
            CmdReply { text: format!("CPC32B1 {count} constraint(s) removed from file {file_name}\n"), messages, ..CmdReply::default() }
        }
        Err(e) => refused(vec![msg(super::leading_msgid(&e).unwrap_or("CPF9898"), 5, &[(&e, 256)])], "CPF32B1", &file_name, &e),
    }
}

/// `CHGPFCST STATE(...)`.
fn change(system: &mut System, parms: &Parms) -> CmdReply {
    let q = parms.qual("FILE");
    let file_name = q.0.clone();
    let state = parms.name("STATE");
    let list: Vec<String> = parms.list("CST").iter().map(|v| v.source()).collect();
    let r = in_store(system, |txn, qsys, nm| {
        let id = file_of(txn, qsys, nm, &q)?;
        let mut out = ops::Changed::default();
        if state == "*SAME" || state.is_empty() {
            return Ok(out);
        }
        let enabled = state == "*ENABLED";
        let picks: Vec<Pick> = if list.iter().any(|c| matches!(c.to_ascii_uppercase().as_str(), "*ALL" | "*CHKPND")) {
            vec![Pick { name: None, kinds: Vec::new() }]
        } else {
            list.iter().map(|c| Pick { name: Some(cst_name(c)), kinds: Vec::new() }).collect()
        };
        for p in picks {
            let r = ops::set_state(txn, id, &p, enabled, now_unix()).map_err(|e| e.to_string())?;
            out.changed.extend(r.changed);
            out.pending.extend(r.pending);
        }
        Ok(out)
    });
    match r {
        Ok(done) => {
            let mut messages = Vec::new();
            for n in &done.pending {
                messages.push(msg("CPD32C5", 5, &[(n, 258)]));
                messages.push(msg("CPD32B7", 5, &[(&file_name, 10)]));
            }
            messages.extend(done.changed.iter().map(|n| msg("CPI32B2", 4, &[(n, 258), (&file_name, 10)])));
            let count = done.changed.len().to_string();
            messages.push(msg("CPC32B2", 4, &[(&count, 4), (&file_name, 10)]));
            CmdReply { text: format!("CPC32B2 {count} constraint(s) changed for file {file_name}\n"), messages, ..CmdReply::default() }
        }
        Err(e) => refused(vec![msg(super::leading_msgid(&e).unwrap_or("CPF9898"), 5, &[(&e, 256)])], "CPF32B2", &file_name, &e),
    }
}

/// `DSPCPCST`: the relative record numbers of the records that keep the
/// constraint in check pending.
fn display(system: &mut System, parms: &Parms) -> CmdReply {
    let q = parms.qual("FILE");
    let file_name = q.0.clone();
    let cst = cst_name(&parms.text("CST"));
    let r = in_store(system, |txn, qsys, nm| {
        let id = file_of(txn, qsys, nm, &q)?;
        let csts = db::cst::list(txn, id).map_err(|e| e.to_string())?;
        let Some(c) = csts.iter().find(|c| c.name == cst && c.check_pending) else {
            return Ok(None);
        };
        let rrns = ops::pending_records(txn, id, &c.name).map_err(|e| e.to_string())?;
        Ok(Some(rrns))
    });
    match r {
        Ok(Some(rrns)) => {
            let mut text = format!("Constraint {cst} of file {file_name}: records in check pending\n");
            for n in &rrns {
                let _ = writeln!(text, "  RRN {n}");
            }
            CmdReply { text, ..CmdReply::default() }
        }
        Ok(None) => refused(vec![msg("CPD32B6", 5, &[(&cst, 258), (&file_name, 10)])], "CPF32B6", &file_name, "not in check pending"),
        Err(e) => refused(vec![msg(super::leading_msgid(&e).unwrap_or("CPF9898"), 5, &[(&e, 256)])], "CPF32B6", &file_name, &e),
    }
}

/// One row of the constraint lists.
fn row(text: &mut String, lib: &str, file: &str, c: &Constraint) {
    let state = if !c.enabled {
        "DISABLED"
    } else if c.check_pending {
        "CHECK PENDING"
    } else {
        "ESTABLISHED"
    };
    let _ = writeln!(text, "  {:<10} {:<10} {:<30} {:<7} {state}", file, lib, c.name, c.kind.word());
}

/// `WRKPFCST`'s list as text.
fn work(system: &mut System, parms: &Parms) -> CmdReply {
    let (fname, lname) = parms.qual("FILE");
    let p = pick(parms, "CST");
    let pending_only = parms.name("CST") == "*CHKPND";
    let r = in_store(system, |txn, qsys, nm| {
        let mut text = String::from("  File       Library    Constraint                     Type    State\n");
        for (lib, file, c) in every(txn, qsys, nm, &fname, &lname)? {
            if (pending_only && !c.check_pending) || !(p.name.is_none() || p.name.as_deref() == Some(c.name.as_str())) {
                continue;
            }
            if !p.kinds.is_empty() && !p.kinds.contains(&c.kind) {
                continue;
            }
            row(&mut text, &lib, &file, &c);
        }
        Ok(text)
    });
    match r {
        Ok(text) => CmdReply { text, ..CmdReply::default() },
        Err(e) => super::direct::failed("WRKPFCST", &e),
    }
}

/// `EDTCPCST`'s list as text: the constraints in check pending.
fn pending(system: &mut System) -> CmdReply {
    let r = in_store(system, |txn, qsys, nm| {
        let mut text = String::from("  File       Library    Constraint                     Type    State\n");
        for (lib, file, c) in every(txn, qsys, nm, "*ALL", "*ALL")? {
            if c.check_pending {
                row(&mut text, &lib, &file, &c);
            }
        }
        Ok(text)
    });
    match r {
        Ok(text) => CmdReply { text, ..CmdReply::default() },
        Err(e) => super::direct::failed("EDTCPCST", &e),
    }
}

/// The constraints of the files a `FILE(lib/file)` names, generic names
/// and `*ALL` included.
fn every(txn: &mut Txn<'_>, qsys: ObjectId, nm: &objops::NameContext, file: &str, lib: &str) -> Result<Vec<(String, String, Constraint)>, String> {
    let libs: Vec<ObjectId> = match lib_or(lib).as_str() {
        "*ALL" | "*ALLUSR" => objops::list(txn, qsys, Some(types::LIB)).map_err(|e| e.to_string())?.into_iter().map(|(_, id)| id).collect(),
        "*LIBL" | "*USRLIBL" => nm.libl.clone(),
        "*CURLIB" => nm.curlib.into_iter().collect(),
        l => vec![wm::obj::find(txn, qsys, nm, "*LIBL", l, types::LIB).map_err(|_| format!("CPF2110 library {l} not found"))?],
    };
    let generic = file.strip_suffix('*');
    let mut out = Vec::new();
    for l in libs {
        let Ok((_, lh)) = objops::read_header(txn, l) else { continue };
        let Ok(list) = objops::list(txn, l, Some(types::FILE)) else { continue };
        for (k, id) in list {
            let n = k.name.to_string();
            let take = file == "*ALL" || generic.map_or(n == file, |g| n.starts_with(g));
            if !take {
                continue;
            }
            for c in db::cst::list(txn, id).unwrap_or_default().into_iter().filter(|c| c.kind != Kind::Dependent) {
                out.push((lh.name.to_string(), n.clone(), c));
            }
        }
    }
    Ok(out)
}
