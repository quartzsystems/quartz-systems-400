// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The database monitor and query attribute commands: `STRDBMON`,
//! `ENDDBMON` (`sql::dbmon`) and `CHGQRYA`.

use cmd::Value;
use cmd::block::Parms;
use sql::dbmon::cmd::{Why, generic_job, job_not_found, refused, started};
use sql::dbmon::{Detail, JobIdent, Scope, StartSpec};
use stg::proto::{CmdMsg, CmdReply};

use super::direct::failed;
use crate::ipl::system::System;

/// Run `name`, or `None` when it is not one of these.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    Some(match name {
        "STRDBMON" => strdbmon(system, parms),
        "ENDDBMON" => enddbmon(system, parms),
        "CHGQRYA" => chgqrya(system, parms),
        _ => return None,
    })
}

/// A message with data: each field padded to its length.
fn msg(id: &str, kind: u8, fields: &[(&str, usize)]) -> CmdMsg {
    CmdMsg { msgid: id.into(), kind, data: cmd::error::data(fields) }
}

fn escape_with(id: &str, text: &str, fields: &[(&str, usize)]) -> CmdReply {
    CmdReply { text: format!("{id} {text}\n"), escape: Some(msg(id, 7, fields)), ..CmdReply::default() }
}

fn ident(system: &System, number: u32) -> JobIdent {
    system.jobs.get(number).map_or_else(
        || JobIdent { name: "QSYSJOB".into(), user: "QSYS".into(), number },
        |j| JobIdent { name: j.name.clone(), user: j.user.clone(), number },
    )
}

/// `JOB`'s job name: the first element of STRDBMON's (the second is
/// the filter operator), ENDDBMON's whole value.
fn job_of(parms: &Parms) -> Option<&Value> {
    match parms.get("JOB") {
        Some(Value::Elems(items)) => items.first().and_then(Option::as_ref),
        other => other,
    }
}

/// The `JOB` parameter: the scope and how messages name it.
fn scope(system: &System, parms: &Parms) -> Result<(Scope, String), String> {
    let job = job_of(parms);
    let quals: Vec<String> = match job {
        Some(Value::Qual(p)) => p.iter().map(|v| v.source().trim().to_ascii_uppercase()).collect(),
        Some(v) => vec![v.source().trim().to_ascii_uppercase()],
        None => vec!["*".into()],
    };
    let part = |i: usize| quals.get(i).map_or("", String::as_str).trim_matches('\'');
    if part(0) == "*ALL" && matches!(part(1), "" | "*ALL") && matches!(part(2), "" | "*ALL") {
        return Ok((Scope::All, "*ALL".into()));
    }
    if quals.iter().any(|q| q.ends_with('*') && q != "*") || part(1) == "*ALL" || part(2) == "*ALL" {
        // BLOCKED(rel): a monitor watches one job or every job.
        return Err(generic_job());
    }
    let n = super::svccmd::job_number_of(system, job)?.unwrap_or(system.current_job);
    let text = if part(0) == "*" || part(0).is_empty() { "*".to_string() } else { ident(system, n).qualified() };
    Ok((Scope::Job(n), text))
}

/// `kwd(value)` refused: what QS/400 does not do.
fn not_kept<T>(kwd: &str, value: &str, why: Why) -> Result<T, String> {
    Err(refused(kwd, value, why))
}

/// A parameter that must keep its default: what QS/400 does not do.
fn only_default(parms: &Parms, kwd: &str, dft: &[&str], why: Why) -> Result<(), String> {
    if !parms.specified(kwd) {
        return Ok(());
    }
    let v = parms.name(kwd);
    if dft.iter().any(|d| v.eq_ignore_ascii_case(d)) {
        return Ok(());
    }
    Err(refused(kwd, &v, why))
}

fn strdbmon(system: &mut System, parms: &Parms) -> CmdReply {
    match start(system, parms) {
        Ok((id, job)) => CmdReply {
            text: started(&id, &job),
            messages: vec![msg("CPI436A", 4, &[(&job, 28), (&id, 10)])],
            ..CmdReply::default()
        },
        Err(e) => monitor_error("STRDBMON", &e),
    }
}

/// A monitor error as an escape with its data.
fn monitor_error(name: &str, e: &str) -> CmdReply {
    let id = super::leading_msgid(e).unwrap_or("CPF0006");
    let rest = e.strip_prefix(id).unwrap_or(e).trim();
    match id {
        "CPF436C" | "CPF436D" | "CPF43A1" => {
            let job = rest.split_whitespace().nth(1).unwrap_or("*");
            escape_with(id, rest, &[(job, 28)])
        }
        _ => failed(name, e),
    }
}

fn start(system: &mut System, parms: &Parms) -> Result<(String, String), String> {
    for kwd in ["FTRUSER", "FTRINTNETA", "FTRCLTACG", "FTRCLTAPP", "FTRCLTPGM", "FTRCLTUSR", "FTRCLTWS", "FTRQRYGOVR", "FTRFILE"] {
        only_default(parms, kwd, &["*NONE", ""], Why::Filter)?;
    }
    for kwd in ["RUNTHLD", "STGTHLD", "FTRLCLPORT"] {
        only_default(parms, kwd, &["*NONE", "-1", ""], Why::Filter)?;
    }
    only_default(parms, "FTRSQLCODE", &["*NONE", "-1000000", ""], Why::Filter)?;
    only_default(parms, "INCSYSSQL", &["*NO", ""], Why::SystemSql)?;
    only_default(parms, "HOSTVAR", &["*BASIC", ""], Why::HostVariables)?;
    if let Some(Value::Elems(items)) = parms.get("JOB")
        && let Some(Some(op)) = items.get(1)
        && !op.source().trim().eq_ignore_ascii_case("*EQ")
    {
        return not_kept("JOB", &op.source(), Why::Filter);
    }
    // TYPE(*DETAIL), the default since IBM i 7.2, writes the same records
    // as *SUMMARY: its 3019 records are among those not written yet
    // (docs/SQL.md lists the record types kept).
    let typ = parms.name("TYPE");
    if !matches!(typ.as_str(), "*BASIC" | "*SUMMARY" | "*DETAIL" | "") {
        return not_kept("TYPE", &typ, Why::Detail);
    }
    let (file, lib) = parms.qual("OUTFILE");
    let lib = if lib.is_empty() { "*LIBL".to_string() } else { lib.to_ascii_uppercase() };
    let file = file.to_ascii_uppercase();
    let e = parms.elems("OUTMBR");
    let word = |i: usize| e.get(i).copied().flatten().map(|v| v.source().trim().to_ascii_uppercase());
    let member = word(0).filter(|m| !m.is_empty()).unwrap_or_else(|| "*FIRST".into());
    let add = word(1).is_some_and(|w| w == "*ADD");
    let frcrcd = match parms.name("FRCRCD").as_str() {
        "" | "*CALC" => 0,
        _ => u16::try_from(parms.int("FRCRCD").unwrap_or(0).max(0)).unwrap_or(u16::MAX),
    };
    let (scope, job_text) = scope(system, parms)?;
    let comment = {
        let c = parms.text("COMMENT");
        if c == "*BLANK" { String::new() } else { c }
    };
    let typ = match parms.name("TYPE").as_str() {
        "*SUMMARY" => "*SUMMARY",
        _ => "*BASIC",
    };
    let shown_member = if member == "*FIRST" { file.clone() } else { member.clone() };
    let job_shown = if scope == Scope::All { "*ALL/*ALL/*ALL".to_string() } else { job_text.clone() };
    let command = format!(
        "STRDBMON  OUTFILE({lib}/{file})  OUTMBR({shown_member} {})  JOB({job_shown})  TYPE({typ})  FRCRCD({frcrcd})  INCSYSSQL(*NO)  COMMENT('{comment}')",
        if add { "*ADD" } else { "*REPLACE" }
    );
    let starter = ident(system, system.current_job);
    let spec = StartSpec {
        lib,
        file,
        member,
        add,
        scope,
        job_text: job_shown.clone(),
        detail: Detail::Basic,
        frcrcd,
        incsyssql: false,
        comment,
        command,
        starter,
    };
    let sysname = system.system_name();
    let (store, ctx) = crate::svc::db::db(system)?;
    ctx.dbmon.system = sysname;
    let mut txn = store.begin().map_err(|e| e.to_string())?;
    match sql::dbmon::start(&mut txn, ctx, &spec) {
        Ok(id) => {
            txn.commit().map_err(|e| e.to_string())?;
            Ok((id, job_shown))
        }
        Err(e) => {
            txn.abort();
            Err(e.to_string())
        }
    }
}

fn enddbmon(system: &mut System, parms: &Parms) -> CmdReply {
    let r = (|| -> Result<(), String> {
        let (scope, job_text) = scope(system, parms)?;
        let monid = parms.specified("MONID").then(|| parms.text("MONID").trim().to_ascii_uppercase()).filter(|m| !m.is_empty());
        let comment = {
            let c = parms.text("COMMENT");
            if c == "*BLANK" { String::new() } else { c }
        };
        let shown = if scope == Scope::All {
            "*ALL".to_string()
        } else if job_text == "*" {
            ident(system, system.current_job).qualified()
        } else {
            job_text
        };
        let sysname = system.system_name();
        let (store, ctx) = crate::svc::db::db(system)?;
        ctx.dbmon.system = sysname;
        let mut txn = store.begin().map_err(|e| e.to_string())?;
        match sql::dbmon::end(&mut txn, ctx, &scope, &shown, monid.as_deref(), &comment) {
            Ok(_) => txn.commit().map_err(|e| e.to_string()),
            Err(e) => {
                txn.abort();
                Err(e.to_string())
            }
        }
    })();
    match r {
        Ok(()) => CmdReply::default(),
        Err(e) => monitor_error("ENDDBMON", &e),
    }
}

/// `CHGQRYA`: the attributes that are hints on IBM i (`DEGREE`,
/// `ASYNCJ`, `APYRMT`) are taken without effect; the governors and the
/// options library, which change what a query may do, keep `*SAME` or
/// `*NOMAX` only.
fn chgqrya(system: &mut System, parms: &Parms) -> CmdReply {
    let r = (|| -> Result<(), String> {
        // The job must exist (CPF1321 when it does not).
        if let Some(Value::Qual(p)) = parms.get("JOB") {
            let name = p.first().map(|v| v.source().trim().to_ascii_uppercase()).unwrap_or_default();
            if name != "*" && super::svccmd::job_number(system, parms, "JOB").is_err() {
                let user = p.get(1).map(|v| v.source().trim().to_ascii_uppercase()).unwrap_or_default();
                let number = p.get(2).map(|v| v.source().trim().to_string()).unwrap_or_default();
                return Err(job_not_found(&name, &user, &number));
            }
        }
        // BLOCKED(rel): the planner makes no time or storage estimate a
        // governor could hold a query to.
        only_default(parms, "QRYTIMLMT", &["*SAME", "*NOMAX", "*SYSVAL", "-1", "-2", "-3", ""], Why::TimeLimit)?;
        only_default(parms, "QRYSTGLMT", &["*SAME", "*NOMAX", "-1", "-2", ""], Why::StorageLimit)?;
        // BLOCKED(rel): query options come from OVERRIDE_QAQQINI, not a
        // QAQQINI file in a library.
        only_default(parms, "QRYOPTLIB", &["*SAME", ""], Why::OptionsLibrary)?;
        let degree = parms.name("DEGREE");
        if degree == "*NBRTASKS" || degree.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            only_default(parms, "DEGREE", &[], Why::Tasks)?;
        }
        Ok(())
    })();
    match r {
        Ok(()) => CmdReply::default(),
        Err(e) => failed("CHGQRYA", &e),
    }
}
