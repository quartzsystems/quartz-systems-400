// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `RUNSQLSTM`: a statement file run statement by statement, each in
//! its own store transaction, with the listing `OUTPUT(*PRINT)` prints.

use std::fmt::Write as _;

use std::cell::{RefCell, RefMut};

use db::cmt::LckLvl;
use store::Store;
use store::txn::Txn;

use crate::ast::Naming;
use crate::embed::StoreLend;
use crate::error::{Result, SqlError};
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
    /// The run is the `RUNSQLSTM` or `RUNSQL` command's: a query is
    /// refused SQL0084 (severity 30), as IBM i refuses it there.
    pub command: bool,
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
    /// Each statement's outcome, for the printed listing.
    pub entries: Vec<RunEntry>,
}

/// One statement's outcome.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunEntry {
    /// The statement's place in the script, from 0.
    pub index: usize,
    /// Its name (`INSERT`, `DROP TABLE`, ...).
    pub name: String,
    /// `SQLCODE`.
    pub sqlcode: i32,
    /// The message identifier of a failure.
    pub msgid: String,
    /// The failure's text.
    pub text: String,
    /// Rows changed.
    pub count: u64,
    /// The object a change or a `DROP` names, as written: schema (when
    /// given) and name.
    pub target: Option<(Option<String>, String)>,
}

/// A query's first `n` rows into the listing.
fn show_rows(listing: &mut String, outcome: Option<&Outcome>, n: usize) {
    if let Some(Outcome::Rows(rs)) = outcome
        && !rs.rows.is_empty()
        && n > 0
    {
        let shown = crate::exec::ResultSet {
            columns: rs.columns.clone(),
            rows: rs.rows.iter().take(n).cloned().collect(),
        };
        listing.push_str(&shown.render());
    }
}

/// A statement's entry.
fn entry(index: usize, name: &str, ca: &Sqlca, outcome: Option<&Outcome>, target: Option<(Option<String>, String)>) -> RunEntry {
    let failed = ca.is_error();
    RunEntry {
        index,
        name: name.to_string(),
        sqlcode: ca.sqlcode,
        msgid: if failed { ca.msgid() } else { String::new() },
        text: if failed { ca.sqlerrmc.clone() } else { String::new() },
        count: match outcome {
            Some(Outcome::Count(c)) => *c,
            _ => 0,
        },
        target,
    }
}

/// The object a data change or a `DROP` names.
fn target_of(s: &crate::ast::Statement) -> Option<(Option<String>, String)> {
    use crate::ast::Statement as S;
    let o = match s {
        S::Insert(i) => &i.table,
        S::Update(u) => &u.table,
        S::Delete(d) => &d.table,
        S::DropTable { name, .. } | S::DropView { name, .. } | S::DropIndex(name) => name,
        _ => return None,
    };
    Some((o.schema.as_ref().map(ToString::to_string), o.name.to_string()))
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
    runsqlstm_lent(&Owned(RefCell::new(store)), ctx, text, opts)
}

/// A store borrowed for the run: lent to each statement in turn.
struct Owned<'s>(RefCell<&'s mut Store>);

impl StoreLend for Owned<'_> {
    fn store(&self) -> RefMut<'_, Store> {
        RefMut::map(self.0.borrow_mut(), |s| &mut **s)
    }
}

/// [`runsqlstm`] over a store lent for each statement. Under a yielder
/// ([`SqlContext::yielder`]) each statement takes turns as an embedded
/// one does, and the run gives a turn between statements; a statement
/// that failed after it committed parts keeps them, as a change ended
/// part way does.
pub fn runsqlstm_lent(
    lender: &dyn StoreLend,
    ctx: &mut SqlContext,
    text: &str,
    opts: &RunOpts,
) -> Result<RunReport> {
    let begin = || Txn::lent(Box::new(|| lender.store()));
    let mut report = RunReport::default();
    ctx.naming = opts.naming;
    let statements = match parse_all(text, opts.naming) {
        Ok(s) => s,
        Err(e) => {
            let ca = Sqlca::from_error(&e);
            report.listing.push_str(&ca.line());
            report.listing.push('\n');
            report.entries.push(RunEntry {
                sqlcode: ca.sqlcode,
                msgid: ca.msgid(),
                text: ca.sqlerrmc,
                ..RunEntry::default()
            });
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
        let mut t = begin()?;
        ctx.session.strcmtctl(&mut t, l, None, ctx.clock())?;
        t.commit()?;
    }
    if let Some(schema) = &opts.dftrdbcol {
        ctx.current_schema = qsname::Name::parse(schema).ok();
    }
    let mut stopped = false;
    // `CALL` is planned for the requester when it makes calls mid-run.
    let could_call = ctx.routine.can_call;
    ctx.routine.can_call = could_call || ctx.midcall.is_some();
    let r = run_statements(&begin, ctx, statements, opts, &mut report, &mut stopped);
    ctx.routine.can_call = could_call;
    r?;
    if lcklvl.is_some() {
        let mut t = begin()?;
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

/// The outcome of a `CALL` whose program the requester made: its
/// answer (`routine::Called`) as an SQLCA.
fn call_made(ctx: &SqlContext, plan: &crate::exec::routine::CallPlan) -> Sqlca {
    let Some(answer) = ctx.midcall.as_ref().and_then(|m| m(plan.clone())) else {
        return Sqlca::from_error(&crate::error::SqlError::Cancelled);
    };
    let Some(c) = crate::exec::routine::Called::from_inputs(&answer) else {
        return Sqlca::from_error(&crate::error::SqlError::Unsupported { what: format!("CALL {}: the answer was not understood", plan.routine) });
    };
    let mut ca = Sqlca::default();
    if c.sqlcode != 0 && c.sqlcode != 100 {
        ca = Sqlca { sqlcode: c.sqlcode, sqlstate: c.sqlstate, sqlerrmc: c.message, ..Sqlca::default() };
        if c.sqlcode > 0 {
            ca.sqlwarn[0] = 'W';
        }
    }
    ca.sqlerrd[0] = c.status;
    ca
}

fn run_statements<'l>(
    begin: &dyn Fn() -> store::Result<Txn<'l>>,
    ctx: &mut SqlContext,
    statements: Vec<crate::ast::Statement>,
    opts: &RunOpts,
    report: &mut RunReport,
    stopped: &mut bool,
) -> Result<()> {
    for (n, st) in statements.into_iter().enumerate() {
        // A turn for the others between statements.
        if n > 0
            && let Err(e) = ctx.between()
        {
            let _ = writeln!(report.listing, "{}", Sqlca::from_error(&e).line());
            report.errors += 1;
            report.severity = 40;
            *stopped = true;
            break;
        }
        report.statements += 1;
        let p = Prepared {
            statement: st,
            naming: opts.naming,
        };
        let name = crate::exec::statement_name(&p.statement);
        let target = target_of(&p.statement);
        let mut t = begin()?;
        let yields = ctx.yields();
        let checked = if opts.command && matches!(p.statement, crate::ast::Statement::Select(_)) {
            Err(SqlError::Unsupported { what: "a SELECT statement in RUNSQLSTM or RUNSQL".into() })
        } else {
            crate::exec::dynamic::check_statement(&p.statement)
        };
        let (ca, outcome) = match checked {
            Ok(()) => p.run(&mut t, ctx, &[]),
            Err(e) => (Sqlca::from_error(&e), None),
        };
        ctx.release_held(&mut t);
        if ca.is_error() && ctx.yields() == yields {
            t.abort();
        } else {
            t.commit()?;
        }
        // A `CALL`'s program, made by the requester through the mid-call
        // path, the statement's outcome its answer.
        let ca = match ctx.routine.pending.take() {
            Some(plan) if !ca.is_error() => call_made(ctx, &plan),
            _ => ca,
        };
        let _ = writeln!(report.listing, "{name}: {}", ca.line());
        report.entries.push(entry(n, name, &ca, outcome.as_ref(), target));
        show_rows(&mut report.listing, outcome.as_ref(), opts.show_rows);
        let sev = severity(&ca);
        report.severity = report.severity.max(sev);
        if ca.is_error() {
            report.errors += 1;
            if sev > opts.errlvl || ca.sqlcode == -952 {
                report
                    .listing
                    .push_str("Run ended: the error exceeds ERRLVL\n");
                report.severity = 40;
                *stopped = true;
                break;
            }
        }
    }
    Ok(())
}
