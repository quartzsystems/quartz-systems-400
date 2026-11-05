// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CALL`: the procedure resolved by name and number of arguments (a
//! program of that name when no procedure has it, as IBM i does), the
//! arguments evaluated and converted to the parameters' types, and the
//! [`CallPlan`] left for the requester.

use qsname::Name;
use store::obj::library::Library;
use store::obj::types;
use store::txn::Txn;

use super::find;
use super::model;
use super::{CallPlan, CallStyle, PlanParm};
use crate::ast::{Call, DataType, Expr, Literal, ParmMode, TypeSpec};
use crate::error::{Result, SqlError};
use crate::exec::eval::{Env, Evaluator, cast};
use crate::exec::{Outcome, SqlContext};
use crate::pl::types::{host_type, parse_type};
use crate::value::Value;

/// What the requester tells the database when the procedure returned:
/// the inputs of [`crate::embed::Op::Called`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Called {
    /// The plan's `seq`.
    pub seq: u64,
    /// The plan's `max_results`.
    pub max: u16,
    /// The call was made inside another procedure: result sets `TO
    /// CLIENT` wait for the outermost call.
    pub nested: bool,
    /// The return status (`SQLERRD(1)`).
    pub status: i32,
    /// The procedure's SQLCODE: negative when it failed.
    pub sqlcode: i32,
    /// Its SQLSTATE.
    pub sqlstate: String,
    /// Its message text.
    pub message: String,
    /// The scope of an SQL procedure's own statements, whose cursors
    /// close as it returns (empty for an external procedure).
    pub scope: String,
}

impl Called {
    /// As the statement's inputs.
    #[must_use]
    pub fn inputs(&self) -> Vec<Value> {
        vec![
            Value::Str(self.seq.to_string()),
            Value::int(self.max.into()),
            Value::int(i64::from(self.nested)),
            Value::int(self.status.into()),
            Value::int(self.sqlcode.into()),
            Value::Str(self.sqlstate.clone()),
            Value::Str(self.message.clone()),
            Value::Str(self.scope.clone()),
        ]
    }

    /// From the statement's inputs.
    #[must_use]
    pub fn from_inputs(v: &[Value]) -> Option<Self> {
        let int = |i: usize| v.get(i).and_then(|x| x.as_i64().ok());
        Some(Self {
            seq: v.first()?.as_text().trim().parse().ok()?,
            max: u16::try_from(int(1)?).ok()?,
            nested: int(2)? != 0,
            status: i32::try_from(int(3)?).ok()?,
            sqlcode: i32::try_from(int(4)?).ok()?,
            sqlstate: v.get(5)?.as_text(),
            message: v.get(6)?.as_text(),
            scope: v.get(7)?.as_text(),
        })
    }
}

/// `CALL`.
pub fn call(txn: &mut Txn<'_>, ctx: &mut SqlContext, c: &Call, params: &[Value]) -> Result<Outcome> {
    let all = find::named(txn, ctx, &c.name, 1)?;
    let plan = if all.is_empty() {
        program_call(txn, ctx, c, params)?
    } else {
        let Some(f) = all.iter().find(|f| usize::from(f.routine.def.parm_count) == c.args.len()) else {
            return Err(SqlError::Coded {
                code: -440,
                state: "42884",
                what: format!("no procedure {} takes {} arguments", c.name, c.args.len()),
            });
        };
        let d = &f.routine.def;
        let style = match (d.body, d.style) {
            (1, _) => CallStyle::SqlRoutine,
            (_, 2) => CallStyle::General,
            (_, 3) => CallStyle::GeneralNulls,
            _ => CallStyle::Sql,
        };
        let mut args = Vec::new();
        for (p, a) in f.routine.parms.iter().filter(|p| p.role == 0).zip(&c.args) {
            let ty = parse_type(&p.ty)?;
            args.push(argument(txn, ctx, params, model::mode_of(p.mode), &ty, a)?);
        }
        CallPlan {
            program: d.program.clone(),
            library: d.program_library.clone(),
            entry: d.entry.clone(),
            style,
            routine: format!("{}.{}", f.schema, d.name),
            specific: d.specific.clone(),
            max_results: d.result_sets,
            seq: crate::exec::cursor::current_seq(),
            commit_on_return: d.has(db::sqlinf::routine::flag::COMMIT_ON_RETURN),
            parms: args,
        }
    };
    if !ctx.routine.can_call {
        // BLOCKED(sql): this interface runs statements in the database
        // only; calling a program needs the job's runtime (embedded SQL
        // and SQL routines have it).
        return Err(SqlError::Unsupported { what: format!("CALL {} from this interface: it cannot run the program", c.name) });
    }
    ctx.routine.pending = Some(Box::new(plan));
    Ok(Outcome::Done)
}

/// One argument for a parameter of `ty`.
fn argument(txn: &mut Txn<'_>, ctx: &mut SqlContext, params: &[Value], mode: ParmMode, ty: &TypeSpec, a: &Expr) -> Result<PlanParm> {
    let h = host_type(ty).map_err(|_| SqlError::Unsupported { what: format!("CALL with a parameter of type {}", crate::pl::types::type_text(ty)) })?;
    let marker = match a {
        Expr::Param(i) => Some(*i),
        _ => None,
    };
    let value = if mode == ParmMode::Out {
        Value::Null
    } else {
        let v = Evaluator { ctx, params }.eval(txn, a, &Env::default())?;
        cast(&v, ty)?
    };
    if mode != ParmMode::In && marker.is_none() {
        return Err(SqlError::Coded {
            code: -469,
            state: "42886",
            what: "an OUT or INOUT argument must be a variable or a parameter marker".into(),
        });
    }
    Ok(PlanParm {
        mode: model::mode_code(mode),
        desc: h.desc,
        extra: h.extra,
        value,
        marker: if mode == ParmMode::In { None } else { marker },
    })
}

/// `CALL` of a program no procedure names: found in the schema or the
/// library list, called `GENERAL`, each argument of its own type and
/// given back (`INOUT`).
fn program_call(txn: &mut Txn<'_>, ctx: &mut SqlContext, c: &Call, params: &[Value]) -> Result<CallPlan> {
    let not_found = || SqlError::ObjectNotFound { name: c.name.to_string(), kind: "procedure" };
    let Ok(n) = Name::parse(&c.name.name.value) else { return Err(not_found()) };
    let libs = if let Some(s) = &c.name.schema {
        vec![ctx.schema_library(txn, s)?]
    } else {
        let mut v: Vec<_> = ctx.names.curlib.into_iter().collect();
        v.extend(ctx.names.libl.iter().copied());
        v
    };
    let mut lib = None;
    for l in libs {
        if Library::load(txn, l)?.lookup(txn, types::PGM, n)?.is_some() {
            lib = Some(store::obj::ops::read_header(txn, l)?.1.name.to_string());
            break;
        }
    }
    let library = lib.ok_or_else(not_found)?;
    let mut args = Vec::new();
    for a in &c.args {
        let v = Evaluator { ctx, params }.eval(txn, a, &Env::default())?;
        let ty = TypeSpec { data_type: own_type(a, &v), ccsid: None, bit_data: false };
        let mode = if matches!(a, Expr::Param(_)) { ParmMode::InOut } else { ParmMode::In };
        let mut p = argument(txn, ctx, params, ParmMode::In, &ty, &Expr::Literal(Literal::Null))?;
        p.mode = model::mode_code(mode);
        p.value = cast(&v, &ty)?;
        p.marker = if mode == ParmMode::InOut { if let Expr::Param(i) = a { Some(*i) } else { None } } else { None };
        args.push(p);
    }
    Ok(CallPlan {
        program: n.to_string(),
        library,
        style: CallStyle::General,
        routine: c.name.to_string(),
        specific: c.name.name.value.clone(),
        seq: crate::exec::cursor::current_seq(),
        parms: args,
        ..CallPlan::default()
    })
}

/// The type an argument passes as when no procedure gives one: an
/// integer constant `INTEGER`, a decimal one `DECIMAL(p,s)`, a string
/// `CHAR(n)`; a marker's value by its kind.
fn own_type(arg: &Expr, v: &Value) -> DataType {
    if let Expr::Literal(Literal::Num(n)) = arg
        && !n.contains(['.', 'E', 'e'])
        && n.parse::<i32>().is_ok()
    {
        return DataType::Integer;
    }
    match v {
        Value::Num(num) => {
            let scale = num.scale();
            DataType::Decimal(num.digits_needed().max(scale).max(1), scale)
        }
        Value::Double(_) => DataType::Double,
        Value::Real(_) => DataType::Real,
        Value::Date(_) => DataType::Date,
        Value::Time(_) => DataType::Time,
        Value::Timestamp(_) => DataType::Timestamp(6),
        other => DataType::Char(u32::try_from(other.as_text().chars().count()).unwrap_or(1).max(1)),
    }
}
