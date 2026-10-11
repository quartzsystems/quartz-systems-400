// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SQL on the console: a line mode standing in for STRSQL, and the
//! statement-file runner.

use std::fmt::Write as _;

use qsname::Name;
use sql::ast::Naming;
use sql::{CommitOpt, Outcome, Prepared, RunOpts, parse_all, runsqlstm};

use super::{Mode, Params, db};
use crate::ipl::system::System;

/// `sql` enters SQL mode; `sql STATEMENT;` runs it at once.
pub fn sql(system: &mut System, line: &str) -> String {
    let text = line.trim_start().strip_prefix("sql").unwrap_or("").trim();
    if text.is_empty() {
        system.svc.mode = Mode::Sql {
            buffer: String::new(),
        };
        return "SQL mode: end each statement with ';', type quit to leave.\n".to_string();
    }
    if text.ends_with(';') {
        return run_text(system, text);
    }
    system.svc.mode = Mode::Sql {
        buffer: format!("{text}\n"),
    };
    String::new()
}

/// Run every statement of `text`, each in its own transaction, and
/// show rows, counts and the SQLCA of anything that is not plain success.
pub fn run_text(system: &mut System, text: &str) -> String {
    let naming = super::naming(system);
    let statements = match parse_all(text, naming) {
        Ok(s) => s,
        Err(e) => return format!("{} {e}\n", e.msgid()),
    };
    let (store, ctx) = match db(system) {
        Ok(x) => x,
        Err(e) => return format!("{e}\n"),
    };
    let mut out = String::new();
    for st in statements {
        let p = Prepared {
            statement: st,
            naming,
        };
        let mut txn = match store.begin() {
            Ok(t) => t,
            Err(e) => return format!("{e}\n"),
        };
        let (ca, outcome) = p.run(&mut txn, ctx, &[]);
        if ca.is_error() {
            txn.abort();
            let _ = writeln!(out, "{}", ca.line());
            continue;
        }
        if let Err(e) = txn.commit() {
            let _ = writeln!(out, "commit failed: {e}");
            continue;
        }
        match outcome {
            Some(Outcome::Rows(rs)) => {
                out.push_str(&rs.render());
                let _ = writeln!(out, "{} row(s)", rs.rows.len());
            }
            Some(Outcome::Count(n)) => {
                let _ = writeln!(out, "{n} row(s) affected");
            }
            Some(Outcome::Done) | None => {
                out.push_str("OK\n");
            }
        }
    }
    out
}

/// `sqlrun NAME [COMMIT(*CHG|*CS|*ALL) ERRLVL(n) NAMING(*SQL) DFTRDBCOL(LIB)]`.
pub fn sqlrun(system: &mut System, p: &Params) -> Result<String, String> {
    let name = p.pos_or(0, "SRCSTMF")?;
    let text = system
        .svc
        .xfer
        .get(&name)
        .cloned()
        .ok_or_else(|| format!("{name}: no such buffer; xfer put it first"))?;
    let opts = RunOpts {
        commit: match p.upper("COMMIT").as_deref() {
            None | Some("*NONE") => CommitOpt::None,
            Some("*CHG") => CommitOpt::Chg,
            Some("*CS") => CommitOpt::Cs,
            Some("*ALL") => CommitOpt::All,
            Some(x) => return Err(format!("COMMIT({x}) is not *NONE, *CHG, *CS or *ALL")),
        },
        naming: match p.upper("NAMING").as_deref() {
            None => super::naming(system),
            Some("*SYS") => Naming::Sys,
            Some("*SQL") => Naming::Sql,
            Some(x) => return Err(format!("NAMING({x}) is not *SYS or *SQL")),
        },
        errlvl: p.num("ERRLVL")?.unwrap_or(10),
        dftrdbcol: p.kw("DFTRDBCOL").map(str::to_string),
        show_rows: 20,
    };
    let (store, ctx) = db(system)?;
    let report = runsqlstm(store, ctx, &text, &opts).map_err(|e| e.to_string())?;
    Ok(format!(
        "{}{} statement(s), {} error(s), severity {}",
        report.listing, report.statements, report.errors, report.severity
    ))
}

/// `naming *SYS|*SQL`.
pub fn naming(system: &mut System, args: &[&str]) -> Result<String, String> {
    let (_, ctx) = db(system)?;
    match args {
        [] => Ok(format!("Naming is {:?}", ctx.naming).to_ascii_uppercase()),
        [m] if m.eq_ignore_ascii_case("*SYS") => {
            ctx.naming = Naming::Sys;
            Ok("Naming *SYS: LIB/FILE, the library list.".into())
        }
        [m] if m.eq_ignore_ascii_case("*SQL") => {
            ctx.naming = Naming::Sql;
            Ok("Naming *SQL: SCHEMA.TABLE, the current schema.".into())
        }
        _ => Err("Usage: naming *SYS|*SQL".into()),
    }
}

/// `curlib LIB`: the current library, also the `*SQL` schema.
pub fn curlib(system: &mut System, args: &[&str]) -> Result<String, String> {
    let [lib] = args else {
        return Err("Usage: curlib LIB".into());
    };
    let name = Name::parse(lib).map_err(|e| format!("{lib}: {e}"))?;
    let (store, ctx) = db(system)?;
    let mut txn = store.begin().map_err(|e| e.to_string())?;
    let found = store::obj::ops::find_library(&mut txn, ctx.qsys, name).map_err(|e| e.to_string());
    txn.abort();
    let id = found?.id;
    ctx.names.curlib = Some(id);
    ctx.names.libl.retain(|l| *l != id);
    ctx.names.libl.insert(0, id);
    ctx.current_schema = Some(name);
    Ok(format!(
        "Current library {name}; it heads the library list."
    ))
}
