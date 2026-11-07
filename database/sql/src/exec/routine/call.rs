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
    /// The procedure, `schema.name` (the plan's `routine`).
    pub routine: String,
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
            Value::Str(self.routine.clone()),
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
            routine: v.get(8).map(Value::as_text).unwrap_or_default(),
        })
    }
}

/// `CALL`.
pub fn call(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    c: &Call,
    params: &[Value],
) -> Result<Outcome> {
    // `SYSTOOLS` need not exist as a library for its procedures.
    let engine = c
        .name
        .schema
        .as_ref()
        .is_some_and(|s| matches!(s.value.as_str(), "QSYS2" | "SYSTOOLS"))
        && crate::exec::catalog::services::procs::is_procedure(
            c.name.schema.as_ref().map(|s| s.value.as_str()),
            &c.name.name.value,
        );
    let all = match find::named(txn, ctx, &c.name, 1) {
        Err(_) if engine => Vec::new(),
        r => r?,
    };
    if let Some(done) = service_call(txn, ctx, c, params, all.is_empty())? {
        return Ok(done);
    }
    let plan = if all.is_empty() {
        program_call(txn, ctx, c, params)?
    } else {
        // The procedure the arguments fit: by position, then by name,
        // the parameters left out taking their defaults.
        let mut bound = None;
        for f in &all {
            if let Some(b) = bind(f, &c.args)? {
                bound = Some((f, b));
                break;
            }
        }
        let Some((f, chosen)) = bound else {
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
        for (p, a) in f.routine.parms.iter().filter(|p| p.role == 0).zip(&chosen) {
            let ty = parse_type(&p.ty)?;
            let default;
            let a = match a {
                Some(a) if !matches!(a, Expr::Default) => *a,
                // DEFAULT, or left out: the parameter's default, else null.
                _ => {
                    default = match &p.default {
                        Some(t) => crate::parse::Parser::new(t, ctx.naming)?.expr()?,
                        None => Expr::Literal(Literal::Null),
                    };
                    &default
                }
            };
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
        // BLOCKED(rel): this interface runs statements in the database
        // only; calling a program needs the job's runtime (embedded SQL
        // and SQL routines have it).
        return Err(SqlError::Unsupported {
            what: format!(
                "CALL {} from this interface: it cannot run the program",
                c.name
            ),
        });
    }
    ctx.routine.pending = Some(Box::new(plan));
    Ok(Outcome::Done)
}

/// The arguments of a procedure's parameters in order (`None` for one
/// left out, which has a default), or `None` when they do not fit it.
/// A name no parameter has is `SQ20483`.
fn bind<'a>(f: &find::Found, args: &'a [Expr]) -> Result<Option<Vec<Option<&'a Expr>>>> {
    let parms: Vec<_> = f.routine.parms.iter().filter(|p| p.role == 0).collect();
    let mut out: Vec<Option<&Expr>> = vec![None; parms.len()];
    let mut at = 0;
    for a in args {
        match a {
            Expr::Function(n) if n.name.starts_with("=>") && n.args.len() == 1 => {
                let name = &n.name[2..];
                let Some(k) = parms.iter().position(|p| p.name == name) else {
                    return Err(SqlError::Coded {
                        code: -20_483,
                        state: "4274K",
                        what: format!("procedure {} has no parameter {name}", f.routine.def.name),
                    });
                };
                if out[k].is_some() {
                    return Ok(None);
                }
                out[k] = Some(&n.args[0]);
            }
            e => {
                if at >= parms.len() {
                    return Ok(None);
                }
                out[at] = Some(e);
                at += 1;
            }
        }
    }
    let fits = parms
        .iter()
        .zip(&out)
        .all(|(p, a)| a.is_some() || p.default.is_some());
    Ok(fits.then_some(out))
}

/// `PREPARE` of a `CALL` with markers: a procedure of the name must
/// exist, or be one the engine runs; a program no procedure names takes
/// no markers in a dynamic statement (SQL0204, as the primary answers).
/// A wrong number of arguments is found as it runs (SQL0440).
pub fn check_prepare(txn: &mut Txn<'_>, ctx: &SqlContext, c: &Call) -> Result<()> {
    use crate::exec::catalog::services::procs;
    if !c.args.iter().any(|a| matches!(a, Expr::Param(_)))
        || !find::named(txn, ctx, &c.name, 1)?.is_empty()
    {
        return Ok(());
    }
    let schema = c.name.schema.as_ref().map(|s| s.value.as_str());
    if matches!(schema, None | Some("QSYS2" | "SYSTOOLS"))
        && procs::is_procedure(schema, &c.name.name.value)
    {
        return Ok(());
    }
    Err(SqlError::ObjectNotFound {
        name: c.name.to_string(),
        kind: "procedure",
    })
}

/// `CALL` of one of the procedures the engine runs itself
/// (`QSYS2.QCMDEXC`, `IFS_WRITE` ...): qualified with its schema, or
/// unqualified when no procedure of the name is found. Their arguments
/// may be named (`PATH_NAME => ...`).
fn service_call(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    c: &Call,
    params: &[Value],
    none_found: bool,
) -> Result<Option<Outcome>> {
    use crate::exec::catalog::services::procs;
    let schema = c.name.schema.as_ref().map(|s| s.value.as_str());
    let ours = match schema {
        Some("QSYS2" | "SYSTOOLS") => true,
        None => none_found,
        _ => false,
    };
    if !ours || !procs::is_procedure(schema, &c.name.name.value) {
        return Ok(None);
    }
    let mut args = Vec::new();
    for a in &c.args {
        let (name, e) = match a {
            Expr::Function(f) if f.name.starts_with("=>") && f.args.len() == 1 => {
                (Some(f.name[2..].to_string()), &f.args[0])
            }
            e => (None, e),
        };
        let v = Evaluator { ctx, params }.eval(txn, e, &Env::default())?;
        args.push((name, v));
    }
    match procs::call(txn, ctx, schema, &c.name.name.value, &args) {
        Some(r) => r.map(|_| Some(Outcome::Done)),
        None => Ok(None),
    }
}

/// One argument for a parameter of `ty`.
fn argument(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    mode: ParmMode,
    ty: &TypeSpec,
    a: &Expr,
) -> Result<PlanParm> {
    let h = host_type(ty).map_err(|_| SqlError::Unsupported {
        what: format!(
            "CALL with a parameter of type {}",
            crate::pl::types::type_text(ty)
        ),
    })?;
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
fn program_call(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    c: &Call,
    params: &[Value],
) -> Result<CallPlan> {
    let not_found = || SqlError::ObjectNotFound {
        name: c.name.to_string(),
        kind: "procedure",
    };
    let Ok(n) = Name::parse(&c.name.name.value) else {
        return Err(not_found());
    };
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
        let ty = TypeSpec {
            data_type: own_type(a, &v),
            ccsid: None,
            bit_data: false,
            udt: None,
        };
        let mode = if matches!(a, Expr::Param(_)) {
            ParmMode::InOut
        } else {
            ParmMode::In
        };
        let mut p = argument(
            txn,
            ctx,
            params,
            ParmMode::In,
            &ty,
            &Expr::Literal(Literal::Null),
        )?;
        p.mode = model::mode_code(mode);
        p.value = cast(&v, &ty)?;
        p.marker = if mode == ParmMode::InOut {
            if let Expr::Param(i) = a {
                Some(*i)
            } else {
                None
            }
        } else {
            None
        };
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
        other => DataType::Char(
            u32::try_from(other.as_text().chars().count())
                .unwrap_or(1)
                .max(1),
        ),
    }
}
