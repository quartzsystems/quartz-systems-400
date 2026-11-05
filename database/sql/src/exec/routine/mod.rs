// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Routines: `CREATE PROCEDURE` and `CREATE FUNCTION` (an SQL
//! procedure's body compiled to a `*PGM` in its schema), `DROP`, and
//! `CALL`. The database cannot run a program, so `CALL` resolves the
//! procedure, evaluates the arguments and leaves a [`CallPlan`] for the
//! requester (the job's runtime) to carry out; when the procedure has
//! returned, the requester says so ([`crate::embed::Op::Called`]) and the
//! procedure's result sets become cursors of the caller.

mod call;
mod create;
pub mod find;
pub mod udf;
pub mod model;

use std::fmt::Write as _;

use store::txn::Txn;

use super::{Outcome, SqlContext};
use crate::ast::Statement;
use crate::error::{Result, SqlError};
use crate::value::Value;

pub use call::Called;

/// What a context keeps of routines between statements.
#[derive(Default)]
pub struct State {
    /// The requester carries out the calls `CALL` plans.
    pub can_call: bool,
    /// The plan the last `CALL` left.
    pub pending: Option<Box<CallPlan>>,
}

/// How the program is called.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CallStyle {
    /// The arguments only (`GENERAL`).
    #[default]
    General,
    /// The arguments and an array of indicators (`GENERAL WITH NULLS`).
    GeneralNulls,
    /// `SQL`: arguments, indicators, SQLSTATE, the names, the message.
    Sql,
    /// An SQL routine of QS/400's: `SQL` and its SQLCA after them.
    SqlRoutine,
}

impl CallStyle {
    fn letter(self) -> char {
        match self {
            Self::General => 'G',
            Self::GeneralNulls => 'N',
            Self::Sql => 'S',
            Self::SqlRoutine => 'Q',
        }
    }

    fn of(c: char) -> Self {
        match c {
            'N' => Self::GeneralNulls,
            'S' => Self::Sql,
            'Q' => Self::SqlRoutine,
            _ => Self::General,
        }
    }
}

/// One argument of a call.
#[derive(Clone, Debug, PartialEq)]
pub struct PlanParm {
    /// 1 IN, 2 OUT, 3 INOUT.
    pub mode: u8,
    /// The parameter's storage: the runtime's descriptor word
    /// ([`crate::pl::types::HostType`]).
    pub desc: i64,
    /// A date's or time's format word.
    pub extra: i64,
    /// The value passed in (`Null` for `OUT`).
    pub value: Value,
    /// For `OUT` and `INOUT`: the input marker of the statement whose
    /// variable receives the value back.
    pub marker: Option<usize>,
}

/// A call for the requester to make.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct CallPlan {
    /// The program, or the service program of `entry`.
    pub program: String,
    /// Its library, or `*LIBL`.
    pub library: String,
    /// The service program procedure; empty for a program.
    pub entry: String,
    /// How it is called.
    pub style: CallStyle,
    /// `SCHEMA.NAME` of the procedure (`SQL` style's routine name).
    pub routine: String,
    /// The specific name.
    pub specific: String,
    /// `DYNAMIC RESULT SETS`.
    pub max_results: u16,
    /// The cursor count when the call began: cursors opened after it
    /// and left open `WITH RETURN` are its result sets.
    pub seq: u64,
    /// `COMMIT ON RETURN YES`.
    pub commit_on_return: bool,
    /// The arguments.
    pub parms: Vec<PlanParm>,
}

const SEP: char = '\u{1f}';
const ROW: char = '\u{1e}';

impl CallPlan {
    /// The plan as text, to cross to another process.
    #[must_use]
    pub fn encode(&self) -> String {
        let mut s = format!(
            "{}{SEP}{}{SEP}{}{SEP}{}{SEP}{}{SEP}{}{SEP}{}{SEP}{}{SEP}{}",
            self.program,
            self.library,
            self.entry,
            self.style.letter(),
            self.routine,
            self.specific,
            self.max_results,
            self.seq,
            u8::from(self.commit_on_return)
        );
        for p in &self.parms {
            let (tag, text) = crate::embed::to_wire(&p.value);
            let _ = write!(
                s,
                "{ROW}{}{SEP}{}{SEP}{}{SEP}{}{SEP}{tag}{SEP}{text}",
                p.mode,
                p.desc,
                p.extra,
                p.marker.map_or_else(String::new, |m| m.to_string())
            );
        }
        s
    }

    /// From its text; `None` for an empty or damaged text.
    #[must_use]
    pub fn decode(s: &str) -> Option<Self> {
        let mut rows = s.split(ROW);
        let head: Vec<&str> = rows.next()?.split(SEP).collect();
        if head.len() != 9 {
            return None;
        }
        let mut plan = Self {
            program: head[0].into(),
            library: head[1].into(),
            entry: head[2].into(),
            style: CallStyle::of(head[3].chars().next()?),
            routine: head[4].into(),
            specific: head[5].into(),
            max_results: head[6].parse().ok()?,
            seq: head[7].parse().ok()?,
            commit_on_return: head[8] == "1",
            parms: Vec::new(),
        };
        for r in rows {
            let f: Vec<&str> = r.splitn(6, SEP).collect();
            if f.len() != 6 {
                return None;
            }
            plan.parms.push(PlanParm {
                mode: f[0].parse().ok()?,
                desc: f[1].parse().ok()?,
                extra: f[2].parse().ok()?,
                marker: if f[3].is_empty() { None } else { Some(f[3].parse().ok()?) },
                value: crate::embed::from_wire(&(f[4].parse().ok()?, f[5].to_string())),
            });
        }
        Some(plan)
    }
}

/// Run a routine statement.
pub fn run(txn: &mut Txn<'_>, ctx: &mut SqlContext, st: &Statement, params: &[Value]) -> Result<Outcome> {
    match st {
        Statement::CreateRoutine(r) => create::create(txn, ctx, r).map(|()| Outcome::Done),
        Statement::DropRoutine(d) => create::drop(txn, ctx, d).map(|()| Outcome::Done),
        Statement::Call(c) => call::call(txn, ctx, c, params),
        Statement::Signal(s) => signal(txn, ctx, s, params),
        Statement::SetResultSets(names) => {
            for c in ctx.cursors.values_mut() {
                c.returns = None;
            }
            for n in names {
                let c = ctx.cursors.get_mut(&n.value).ok_or_else(|| SqlError::Coded {
                    code: -504,
                    state: "34000",
                    what: format!("cursor {} is not declared", n.value),
                })?;
                c.returns = Some(crate::ast::ReturnTo::Caller);
            }
            Ok(Outcome::Done)
        }
        _ => Err(SqlError::Unsupported { what: "that routine statement".into() }),
    }
}

/// `SIGNAL`: the condition as the statement's outcome, `SQL0438`.
fn signal(txn: &mut Txn<'_>, ctx: &mut SqlContext, s: &crate::ast::Signal, params: &[Value]) -> Result<Outcome> {
    let env = super::eval::Env::default();
    let mut eval = |ctx: &mut SqlContext, e: &crate::ast::Expr| super::eval::Evaluator { ctx, params }.eval(txn, e, &env);
    let state = match &s.state {
        Some(crate::ast::SignalValue::State(st)) => st.clone(),
        Some(crate::ast::SignalValue::Variable(e)) => eval(ctx, &e.expr)?.as_text().trim_end().to_string(),
        Some(crate::ast::SignalValue::Condition(c)) => {
            return Err(SqlError::Coded { code: -312, state: "42618", what: format!("condition {} is not declared", c.value) });
        }
        None => return Err(SqlError::Coded { code: -1067, state: "0K000", what: "RESIGNAL outside a handler".into() }),
    };
    if state.len() != 5 || state.starts_with("00") {
        return Err(SqlError::Coded { code: -435, state: "428B3", what: format!("SQLSTATE {state} is not one a SIGNAL may raise") });
    }
    let text = match &s.message {
        Some(m) => eval(ctx, &m.expr)?.as_text().trim_end().to_string(),
        None => format!("SQLSTATE {state} was signalled"),
    };
    let text: String = text.chars().take(70).collect();
    if state.starts_with("01") {
        ctx.warning = Some(crate::sqlca::Warning::coded(438, &state, &text));
        return Ok(Outcome::Done);
    }
    if state.starts_with("02") {
        ctx.warning = Some(crate::sqlca::Warning::coded(100, &state, &text));
        return Ok(Outcome::Done);
    }
    Err(SqlError::Signalled { code: -438, state, what: text })
}
