// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `RUNSQLSTM`: a statement file run statement by statement, each in
//! its own store transaction, with the listing `OUTPUT(*PRINT)` prints.

use std::fmt::Write as _;

use db::cmt::LckLvl;
use store::Store;

use crate::ast::Naming;
use crate::error::Result;
use crate::exec::{Outcome, Prepared, SqlContext};
use crate::parse::parse_all;
use crate::sqlca::Sqlca;

/// `COMMIT`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CommitOpt {
    /// `*NONE`.
    #[default]
    None,
    /// `*CHG`.
    Chg,
    /// `*CS`.
    Cs,
    /// `*ALL`.
    All,
}

/// The parameters.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunOpts {
    /// `COMMIT`.
    pub commit: CommitOpt,
    /// `NAMING`.
    pub naming: Naming,
    /// `ERRLVL`: the severity a failing statement may have without
    /// stopping the run (0, 10, 20, 30, 40).
    pub errlvl: u8,
    /// `DFTRDBCOL`: the default schema.
    pub dftrdbcol: Option<String>,
    /// Row limit for result sets shown in the listing.
    pub show_rows: usize,
}

/// What a run did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunReport {
    /// The listing.
    pub listing: String,
    /// Statements run.
    pub statements: u32,
    /// Statements that failed.
    pub errors: u32,
    /// The highest severity seen: 0, 30 for an error, 40 for a stop.
    pub severity: u8,
}

/// The severity of an SQLCODE: 30 for errors, 0 otherwise.
fn severity(sqlca: &Sqlca) -> u8 {
    if sqlca.is_error() { 30 } else { 0 }
}

/// Run a script. Each statement commits its own store transaction;
/// under `COMMIT(*CHG|*CS|*ALL)` the changes are also under commitment
/// control, committed at the end or rolled back on a stopping error.
pub fn runsqlstm(
    store: &mut Store,
    ctx: &mut SqlContext,
    text: &str,
    opts: &RunOpts,
) -> Result<RunReport> {
    let mut report = RunReport::default();
    ctx.naming = opts.naming;
    let statements = match parse_all(text, opts.naming) {
        Ok(s) => s,
        Err(e) => {
            let ca = Sqlca::from_error(&e);
            report.listing.push_str(&ca.line());
            report.listing.push('\n');
            report.errors = 1;
            report.severity = 40;
            return Ok(report);
        }
    };
    let lcklvl = match opts.commit {
        CommitOpt::None => None,
        CommitOpt::Chg => Some(LckLvl::Chg),
        CommitOpt::Cs => Some(LckLvl::Cs),
        CommitOpt::All => Some(LckLvl::All),
    };
    if let Some(l) = lcklvl
        && ctx.session.commit_def().is_none()
    {
        let mut t = store.begin()?;
        ctx.session.strcmtctl(&mut t, l, None, ctx.clock())?;
        t.commit()?;
    }
    if let Some(schema) = &opts.dftrdbcol {
        ctx.current_schema = qsname::Name::parse(schema).ok();
    }
    let mut stopped = false;
    for st in statements {
        report.statements += 1;
        let p = Prepared {
            statement: st,
            naming: opts.naming,
        };
        let name = crate::exec::statement_name(&p.statement);
        let mut t = store.begin()?;
        let (ca, outcome) = p.run(&mut t, ctx, &[]);
        if ca.is_error() {
            t.abort();
        } else {
            t.commit()?;
        }
        let _ = writeln!(report.listing, "{name}: {}", ca.line());
        if let Some(Outcome::Rows(rs)) = &outcome
            && !rs.rows.is_empty()
            && opts.show_rows > 0
        {
            let shown = crate::exec::ResultSet {
                columns: rs.columns.clone(),
                rows: rs.rows.iter().take(opts.show_rows).cloned().collect(),
            };
            report.listing.push_str(&shown.render());
        }
        let sev = severity(&ca);
        report.severity = report.severity.max(sev);
        if ca.is_error() {
            report.errors += 1;
            if sev > opts.errlvl {
                report
                    .listing
                    .push_str("Run ended: the error exceeds ERRLVL\n");
                report.severity = 40;
                stopped = true;
                break;
            }
        }
    }
    if lcklvl.is_some() {
        let mut t = store.begin()?;
        if stopped {
            ctx.session.rollbck(&mut t, ctx.now)?;
            report.listing.push_str("ROLLBACK\n");
        } else {
            ctx.session.commit(&mut t, ctx.now)?;
            report.listing.push_str("COMMIT\n");
        }
        crate::exec::cursor::end_of_unit(ctx);
        t.commit()?;
    }
    Ok(report)
}
