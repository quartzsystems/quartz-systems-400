// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The physical file constraint commands on `db::cst::ops`: `ADDPFCST`,
//! `RMVPFCST`, `CHGPFCST`, `DSPCPCST`, and the data of the `WRKPFCST`
//! and `EDTCPCST` panels (`QSDBCST`, driven by the job). Every message
//! is a `QCPFMSG` identifier with its data, sent as the primary sends
//! them (`tools/conformance/sql/oracle/integrity/primary-facts.md`).

#![allow(clippy::many_single_char_names)]

use std::fmt::Write as _;

use cmd::block::Parms;
use db::cst::ops::{self, NewCst, Pick};
use db::cst::{Constraint, Kind, Rule};
use qsname::Name;
use stg::proto::{CmdMsg, CmdReply, PanelData};
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
        _ => return None,
    })
}

/// A message with character data.
fn msg(id: &str, kind: u8, fields: &[(&str, usize)]) -> CmdMsg {
    CmdMsg {
        msgid: id.into(),
        kind,
        data: cmd::error::data(fields),
    }
}

/// A completion message of a count (`*BIN 4`) and a file.
fn counted(id: &str, n: usize, file: &str) -> CmdMsg {
    let mut data = i32::try_from(n).unwrap_or(i32::MAX).to_be_bytes().to_vec();
    data.extend(cmd::error::data(&[(file, 10)]));
    CmdMsg {
        msgid: id.into(),
        kind: 4,
        data,
    }
}

/// Diagnostics, then the escape `esc` naming the file.
fn refused(diags: Vec<CmdMsg>, esc: &str, file: &str) -> CmdReply {
    CmdReply {
        messages: diags,
        escape: Some(msg(esc, 7, &[(file, 10)])),
        ..CmdReply::default()
    }
}

/// A store failure (`e`, the error's text) as its diagnostic.
fn failure(e: &str) -> CmdMsg {
    msg(super::leading_msgid(e).unwrap_or("CPF9898"), 5, &[(e, 256)])
}

/// What a command's work gives: its result, or the diagnostic that
/// refused it.
type Step<T> = Result<T, CmdMsg>;

fn file_of(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    names: &objops::NameContext,
    q: &(String, String),
) -> Step<ObjectId> {
    let lib = lib_or(&q.1);
    wm::obj::find(txn, qsys, names, &lib, &q.0, types::FILE)
        .map_err(|_| msg("CPF9812", 5, &[(&q.0, 10), (&lib, 10)]))
}

/// A constraint name as given: upper case unless quoted.
fn cst_name(raw: &str) -> String {
    let t = raw.trim();
    match t.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        Some(q) => q.to_string(),
        None => t.to_ascii_uppercase(),
    }
}

fn names(parms: &Parms, kw: &str) -> Step<Vec<Name>> {
    parms
        .list(kw)
        .iter()
        .map(|v| {
            let s = v.source().to_ascii_uppercase();
            Name::parse(&s).map_err(|_| msg("CPD0078", 5, &[(&s, 10), (kw, 10)]))
        })
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
        db::DbError::Constraint {
            reason: ops::Refusal::NotFound,
            ..
        } => msg("CPD32B1", 5, &[(cst, 258), (file, 10)]),
        other => msg(
            other.msgid().unwrap_or("CPF9898"),
            5,
            &[(&other.to_string(), 256)],
        ),
    }
}

/// The work of a command in the store: `Ok(Err(m))` is a refusal with
/// its diagnostic, `Err` a store failure.
fn work<T>(
    system: &mut System,
    f: impl FnOnce(&mut Txn<'_>, ObjectId, &objops::NameContext) -> Step<T>,
) -> Step<T> {
    match in_store(system, |txn, qsys, nm| Ok(f(txn, qsys, nm))) {
        Ok(r) => r,
        Err(e) => Err(failure(&e)),
    }
}

/// `ADDPFCST`.
fn add(system: &mut System, parms: &Parms) -> CmdReply {
    let q = parms.qual("FILE");
    let file_name = q.0.clone();
    let ty = parms.name("TYPE");
    let r = work(system, |txn, qsys, nm| {
        let id = file_of(txn, qsys, nm, &q)?;
        let k = kind(&ty).ok_or_else(|| msg("CPD0084", 5, &[(&ty, 10)]))?;
        let mut n = NewCst::new(
            k,
            if k == Kind::Check {
                Vec::new()
            } else {
                names(parms, "KEY")?
            },
        );
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
            let e = sql::parse::parse_expr(&text, sql::Naming::Sys)
                .map_err(|e| failure(&e.to_string()))?;
            let rf = db::file::describe(txn, id)
                .map_err(|e| failure(&e.to_string()))?
                .formats[0]
                .format
                .clone();
            let prog = sql::exec::cst::compile_check(&e, &rf, n.name.as_deref().unwrap_or("*N"))
                .map_err(|e| failure(&e.to_string()))?;
            n.check = Some((text, prog));
        }
        if k.keyed() && n.columns.is_empty() {
            return Err(msg("CPD0071", 5, &[("KEY", 10)]));
        }
        ops::add(txn, id, &n, now_unix())
            .map_err(|e| diag(&e, n.name.as_deref().unwrap_or("*GEN"), &file_name))
    });
    match r {
        Ok(report) => {
            let mut messages: Vec<CmdMsg> = report
                .names
                .iter()
                .map(|n| msg("CPI32B0", 4, &[(n, 258), (&file_name, 10)]))
                .collect();
            for (id, data) in &report.messages {
                messages.push(match *id {
                    "CPI32B4" | "CPI32B5" => msg(id, 4, &[(data, 10)]),
                    "CPD32C5" => msg(id, 5, &[(data, 258)]),
                    _ => msg(id, 5, &[(data, 10)]),
                });
            }
            messages.push(counted("CPC32B0", 1, &file_name));
            CmdReply {
                messages,
                ..CmdReply::default()
            }
        }
        Err(m) => refused(vec![m], "CPF32B0", &file_name),
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
    let r = work(system, |txn, qsys, nm| {
        let id = file_of(txn, qsys, nm, &q)?;
        let store = |e: db::DbError| failure(&e.to_string());
        if pending_only {
            let names: Vec<String> = db::cst::list(txn, id)
                .map_err(store)?
                .into_iter()
                .filter(|c| c.check_pending)
                .map(|c| c.name)
                .collect();
            let mut all = ops::Removed::default();
            for n in names {
                let one = Pick {
                    name: Some(n),
                    kinds: Vec::new(),
                };
                let r = ops::remove(txn, id, &one, cascade, now_unix()).map_err(store)?;
                all.removed.extend(r.removed);
                all.refused.extend(r.refused);
            }
            return Ok(all);
        }
        ops::remove(txn, id, &p, cascade, now_unix()).map_err(store)
    });
    match r {
        Ok(done) => {
            let mut messages: Vec<CmdMsg> = done
                .refused
                .iter()
                .map(|n| msg("CPD32B1", 5, &[(n, 258), (&file_name, 10)]))
                .collect();
            messages.extend(
                done.removed
                    .iter()
                    .map(|n| msg("CPI32B1", 4, &[(n, 258), (&file_name, 10)])),
            );
            messages.push(counted("CPC32B1", done.removed.len(), &file_name));
            CmdReply {
                messages,
                ..CmdReply::default()
            }
        }
        Err(m) => refused(vec![m], "CPF32B1", &file_name),
    }
}

/// `CHGPFCST STATE(...)`.
fn change(system: &mut System, parms: &Parms) -> CmdReply {
    let q = parms.qual("FILE");
    let file_name = q.0.clone();
    let state = parms.name("STATE");
    let list: Vec<String> = parms.list("CST").iter().map(|v| v.source()).collect();
    let r = work(system, |txn, qsys, nm| {
        let id = file_of(txn, qsys, nm, &q)?;
        let mut out = ops::Changed::default();
        if state == "*SAME" || state.is_empty() {
            return Ok(out);
        }
        let enabled = state == "*ENABLED";
        let picks: Vec<Pick> = if list
            .iter()
            .any(|c| matches!(c.to_ascii_uppercase().as_str(), "*ALL" | "*CHKPND"))
        {
            vec![Pick {
                name: None,
                kinds: Vec::new(),
            }]
        } else {
            list.iter()
                .map(|c| Pick {
                    name: Some(cst_name(c)),
                    kinds: Vec::new(),
                })
                .collect()
        };
        for p in picks {
            let r = ops::set_state(txn, id, &p, enabled, now_unix())
                .map_err(|e| failure(&e.to_string()))?;
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
            messages.extend(
                done.changed
                    .iter()
                    .map(|n| msg("CPI32B2", 4, &[(n, 258), (&file_name, 10)])),
            );
            messages.push(counted("CPC32B2", done.changed.len(), &file_name));
            CmdReply {
                messages,
                ..CmdReply::default()
            }
        }
        Err(m) => refused(vec![m], "CPF32B2", &file_name),
    }
}

/// `DSPCPCST`: the relative record numbers of the records that keep the
/// constraint in check pending, under a heading from `QSTXTMSG`.
fn display(system: &mut System, parms: &Parms) -> CmdReply {
    let q = parms.qual("FILE");
    let file_name = q.0.clone();
    let cst = cst_name(&parms.text("CST"));
    let r = work(system, |txn, qsys, nm| {
        let id = file_of(txn, qsys, nm, &q)?;
        let csts = db::cst::list(txn, id).map_err(|e| failure(&e.to_string()))?;
        let Some(c) = csts.iter().find(|c| c.name == cst && c.check_pending) else {
            return Err(msg("CPD32B6", 5, &[(&cst, 258), (&file_name, 10)]));
        };
        ops::pending_records(txn, id, &c.name).map_err(|e| failure(&e.to_string()))
    });
    match r {
        Ok(rrns) => {
            let mut text = crate::stgsvc::msgs::text(system, "TXT0312", &[&cst, &file_name]);
            text.push('\n');
            for n in &rrns {
                let _ = writeln!(text, "{n:>12}");
            }
            CmdReply {
                text,
                ..CmdReply::default()
            }
        }
        Err(m) => refused(vec![m], "CPF32B6", &file_name),
    }
}

/// The values of a constraint's row in `WRKPFCST`: its state and check
/// pending, blank for a key, which is always in force. Check pending
/// is set two columns into its field, where the 7.1 list shows it.
fn state_of(c: &Constraint) -> (&'static str, &'static str) {
    match c.kind {
        Kind::Unique | Kind::PrimaryKey => ("", ""),
        _ => (
            if c.enabled { "EST/ENB" } else { "EST/DSB" },
            if c.check_pending && c.enabled {
                "  YES"
            } else {
                "  NO"
            },
        ),
    }
}

/// `WRKPFCST`'s list (`QSDBCST`'s `CSTS`): `lib` and `file` as the
/// command names them, generic names included; `cst` a name, `*ALL` or
/// `*CHKPND`; `ty` a type or `*ALL`.
pub fn wrkpfcst_panel(
    system: &mut System,
    lib: &str,
    file: &str,
    cst: &str,
    ty: &str,
) -> Result<PanelData, String> {
    let pending_only = cst == "*CHKPND";
    let name = (!matches!(cst, "*ALL" | "*CHKPND" | "")).then(|| cst_name(cst));
    let want = kind(ty);
    let file = if file.is_empty() { "*ALL" } else { file };
    in_store(system, |txn, qsys, nm| {
        let mut rows = Vec::new();
        for (l, f, c) in every(txn, qsys, nm, file, lib)? {
            if (pending_only && !c.check_pending)
                || name.as_ref().is_some_and(|n| *n != c.name)
                || want.is_some_and(|k| k != c.kind)
            {
                continue;
            }
            let (state, pending) = state_of(&c);
            rows.push(vec![
                ("CST".to_string(), c.name.clone()),
                ("FILE".to_string(), f),
                ("FILELIB".to_string(), l),
                ("CSTTYPE".to_string(), c.kind.word().to_string()),
                ("CSTSTATE".to_string(), state.to_string()),
                ("CHKPND".to_string(), pending.to_string()),
            ]);
        }
        Ok((Vec::new(), rows))
    })
}

/// `EDTCPCST`'s list (`QSDBCST`'s `CPCSTS`): the enabled constraints in
/// check pending, each held until a sequence is given.
pub fn edtcpcst_panel(system: &mut System) -> Result<PanelData, String> {
    in_store(system, |txn, qsys, nm| {
        let mut rows = Vec::new();
        for (l, f, c) in every(txn, qsys, nm, "*ALL", "*ALL")? {
            if !(c.check_pending && c.enabled) {
                continue;
            }
            rows.push(vec![
                ("SEQ".to_string(), "*HLD".to_string()),
                ("CPSTS".to_string(), "CHKPND".to_string()),
                ("CPCST".to_string(), c.name.clone()),
                ("CPFILE".to_string(), f),
                ("CPLIB".to_string(), l),
                ("VFYTIME".to_string(), "00:00:00".to_string()),
                ("ELAPSED".to_string(), "00:00:00".to_string()),
            ]);
        }
        Ok((Vec::new(), rows))
    })
}

/// The constraints of the files a `FILE(lib/file)` names, generic names
/// and `*ALL` included.
fn every(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    nm: &objops::NameContext,
    file: &str,
    lib: &str,
) -> Result<Vec<(String, String, Constraint)>, String> {
    let libs: Vec<ObjectId> = match lib_or(lib).as_str() {
        "*ALL" | "*ALLUSR" => objops::list(txn, qsys, Some(types::LIB))
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|(_, id)| id)
            .collect(),
        "*LIBL" | "*USRLIBL" => nm.libl.clone(),
        "*CURLIB" => nm.curlib.into_iter().collect(),
        l => vec![
            wm::obj::find(txn, qsys, nm, "*LIBL", l, types::LIB)
                .map_err(|_| l.to_string())
                .map_err(|l| format!("CPF2110 {l}"))?,
        ],
    };
    let generic = file.strip_suffix('*');
    let mut out = Vec::new();
    for l in libs {
        let Ok((_, lh)) = objops::read_header(txn, l) else {
            continue;
        };
        let Ok(list) = objops::list(txn, l, Some(types::FILE)) else {
            continue;
        };
        for (k, id) in list {
            let n = k.name.to_string();
            let take = file == "*ALL" || generic.map_or(n == file, |g| n.starts_with(g));
            if !take {
                continue;
            }
            for c in db::cst::list(txn, id)
                .unwrap_or_default()
                .into_iter()
                .filter(|c| c.kind != Kind::Dependent)
            {
                out.push((lh.name.to_string(), n.clone(), c));
            }
        }
    }
    Ok(out)
}
