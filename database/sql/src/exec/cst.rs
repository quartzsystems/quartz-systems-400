// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SQL's constraints over the database's (`db::cst`): a `CONSTRAINT`
//! clause becomes a `NewCst`, a `CHECK` condition a check program the
//! database evaluates on every write, and the database's refusals the
//! SQLCODEs IBM gives (settled on the primary, `tools/conformance/sql/
//! oracle/integrity/`).

use db::DbError;
use db::cst::check::{self, Arith, Cmp, Func};
use db::cst::ops::{self, NewCst, Pick, Refusal};
use db::cst::{Kind, Rule};
use dds::RecordFormat;
use qdecimal::Decimal;
use qsname::Name;
use store::obj::ObjectId;
use store::txn::Txn;

use super::SqlContext;
use crate::ast::{self, BinOp, CmpOp, Constraint, Expr, Ident, Literal, RefRule};
use crate::error::{Result, SqlError};

fn err(sqlcode: i32, sqlstate: &'static str, what: impl Into<String>) -> SqlError {
    SqlError::Integrity {
        sqlcode,
        sqlstate,
        what: what.into(),
    }
}

/// The field a column name names: its system name or its SQL name.
pub fn field_name(rf: &RecordFormat, c: &Ident) -> Result<Name> {
    if let Ok(n) = SqlContext::sys_name(c)
        && rf.field(&n).is_some()
    {
        return Ok(n);
    }
    rf.fields
        .iter()
        .find(|f| f.alias.as_deref() == Some(c.value.as_str()))
        .map(|f| f.name)
        // A key or a change naming a column the table lacks: `SQL0205`.
        .ok_or_else(|| SqlError::Coded {
            code: -205,
            state: "42703",
            what: format!("column {} is not in the table", c.value),
        })
}

fn rule(r: RefRule) -> Rule {
    match r {
        RefRule::NoAction => Rule::NoAction,
        RefRule::Restrict => Rule::Restrict,
        RefRule::Cascade => Rule::Cascade,
        RefRule::SetNull => Rule::SetNull,
        RefRule::SetDefault => Rule::SetDefault,
    }
}

/// What `c` adds to the table whose format is `rf`; `self_id` is the
/// table, for a foreign key that references it.
pub fn new_cst(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    rf: &RecordFormat,
    c: &Constraint,
) -> Result<NewCst> {
    let cols = |cs: &[Ident]| {
        cs.iter()
            .map(|c| field_name(rf, c))
            .collect::<Result<Vec<_>>>()
    };
    let mut n = match c {
        Constraint::PrimaryKey { columns, .. } => NewCst::new(Kind::PrimaryKey, cols(columns)?),
        Constraint::Unique { columns, .. } => NewCst::new(Kind::Unique, cols(columns)?),
        Constraint::Check { cond, text, name } => {
            let cname = name.as_ref().map_or("*N", |n| n.value.as_str());
            let e = compile_check(cond, rf, cname)?;
            NewCst {
                check: Some((text.clone(), e)),
                ..NewCst::new(Kind::Check, Vec::new())
            }
        }
        Constraint::ForeignKey {
            columns,
            parent,
            parent_columns,
            on_delete,
            on_update,
            ..
        } => {
            let p = ctx.resolve_table(txn, parent)?;
            let pcols = parent_columns
                .iter()
                .map(|c| field_name(&p.format.format, c))
                .collect::<Result<Vec<_>>>()?;
            NewCst {
                parent: p.id,
                parent_columns: pcols,
                delete_rule: rule(*on_delete),
                update_rule: rule(*on_update),
                ..NewCst::new(Kind::Referential, cols(columns)?)
            }
        }
    };
    n.name = c.name().map(|i| i.value.clone());
    Ok(n)
}

/// Add `c` to `table`.
pub fn add(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    table: ObjectId,
    rf: &RecordFormat,
    c: &Constraint,
) -> Result<()> {
    let n = new_cst(txn, ctx, rf, c)?;
    ops::add(txn, table, &n, ctx.clock())?;
    Ok(())
}

/// `DROP PRIMARY KEY` and `DROP CONSTRAINT | UNIQUE | FOREIGN KEY | CHECK`.
pub fn drop(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    table: ObjectId,
    name: Option<&Ident>,
    kind: ast::DropKind,
    cascade: bool,
) -> Result<()> {
    let csts = db::cst::list(txn, table)?;
    let found = match name {
        None => csts.iter().find(|c| c.kind == Kind::PrimaryKey),
        Some(n) => csts
            .iter()
            .find(|c| c.kind != Kind::Dependent && c.name == n.value),
    };
    let Some(found) = found else {
        let what = name.map_or_else(|| "the primary key".to_string(), |n| n.value.clone());
        return Err(err(-204, "42704", format!("constraint {what} not found")));
    };
    let fits = match kind {
        ast::DropKind::Any => true,
        ast::DropKind::Unique => matches!(found.kind, Kind::Unique | Kind::PrimaryKey),
        ast::DropKind::ForeignKey => found.kind == Kind::Referential,
        ast::DropKind::Check => found.kind == Kind::Check,
    };
    if !fits {
        return Err(err(
            -204,
            "42704",
            format!("constraint {} is not of the kind named", found.name),
        ));
    }
    let pick = Pick {
        name: Some(found.name.clone()),
        kinds: Vec::new(),
    };
    let r = ops::remove(txn, table, &pick, cascade, ctx.clock())?;
    if !r.refused.is_empty() {
        return Err(err(
            -616,
            "42893",
            format!(
                "constraint {} has dependent foreign keys; drop it with CASCADE",
                found.name
            ),
        ));
    }
    Ok(())
}

/// A database refusal as IBM's SQLCODE.
#[must_use]
pub fn from_db(e: &DbError) -> SqlError {
    let what = e.to_string();
    match e {
        DbError::CheckViolation { .. } => err(-545, "23513", what),
        DbError::RefNoParent { .. } => err(-530, "23503", what),
        DbError::RefRestricted {
            update, no_action, ..
        } => {
            let state = if *no_action { "23504" } else { "23001" };
            if *update {
                err(-531, state, what)
            } else {
                err(-532, state, what)
            }
        }
        DbError::CheckPending { .. } => err(-971, "57011", what),
        DbError::RuleNeedsJournal { .. } => err(-7008, "55019", what),
        DbError::IdentityExhausted { .. } => err(-359, "23522", what),
        DbError::Constraint { reason, .. } => match reason {
            Refusal::Exists => err(-601, "42710", what),
            Refusal::NoColumn => err(-205, "42703", what),
            Refusal::BadColumns => err(-537, "42709", what),
            Refusal::LobKey => err(-350, "42962", what),
            Refusal::SecondPrimary => err(-624, "42889", what),
            Refusal::SameKey => err(-541, "42891", what),
            Refusal::Nullable => err(-542, "42831", what),
            Refusal::DuplicateData => err(-603, "23515", what),
            Refusal::CheckData => err(-544, "23512", what),
            Refusal::RefData => err(-667, "23520", what),
            Refusal::KeyMismatch => err(-538, "42830", what),
            Refusal::NoParentKey => err(-573, "42890", what),
            Refusal::SetNullNotNull => err(-629, "42834", what),
            Refusal::NotFound => err(-204, "42704", what),
            Refusal::HasDependents => err(-616, "42893", what),
            Refusal::NotPhysical => err(-7008, "55019", what),
            Refusal::NotDisableable => SqlError::Unsupported { what },
        },
        _ => SqlError::Database {
            what,
            msgid: e.msgid().map(str::to_string),
        },
    }
}

/// A check condition IBM does not allow (`SQL0546`).
fn invalid(cname: &str, why: &str) -> SqlError {
    err(
        -546,
        "42621",
        format!("the CHECK condition of constraint {cname} is not valid: {why}"),
    )
}

/// `cond` as the database's check program over `rf`'s fields.
#[allow(clippy::too_many_lines)]
pub fn compile_check(cond: &Expr, rf: &RecordFormat, cname: &str) -> Result<check::Expr> {
    let go = |e: &Expr| compile_check(e, rf, cname);
    let b = |e: &Expr| -> Result<Box<check::Expr>> { Ok(Box::new(go(e)?)) };
    Ok(match cond {
        Expr::Literal(l) => match l {
            Literal::Num(n) => {
                if n.contains(['E', 'e']) {
                    check::Expr::Float(n.parse().map_err(|_| invalid(cname, "a number"))?)
                } else {
                    check::Expr::Num(Decimal::parse(n).map_err(|_| invalid(cname, "a number"))?)
                }
            }
            Literal::Str(s) | Literal::Date(s) | Literal::Time(s) | Literal::Timestamp(s) => {
                check::Expr::Str(s.clone())
            }
            Literal::Null => check::Expr::Null,
            Literal::Hex(_) | Literal::Bin(_) | Literal::Bool(_) => {
                return Err(SqlError::Unsupported {
                    what: "a binary or boolean literal in a CHECK condition".into(),
                });
            }
        },
        Expr::Column(c) => {
            if c.table.is_some() {
                return Err(invalid(cname, "a qualified column"));
            }
            // A check's column the table lacks is `SQL0206`, as in a query.
            check::Expr::Col(
                field_name(rf, &c.column).map_err(|_| SqlError::ColumnNotFound {
                    name: c.column.value.clone(),
                })?,
            )
        }
        Expr::Neg(a) => check::Expr::Neg(b(a)?),
        Expr::Binary(x, op, y) => match op {
            BinOp::Concat => check::Expr::Concat(b(x)?, b(y)?),
            BinOp::Add => check::Expr::Arith(Arith::Add, b(x)?, b(y)?),
            BinOp::Sub => check::Expr::Arith(Arith::Sub, b(x)?, b(y)?),
            BinOp::Mul => check::Expr::Arith(Arith::Mul, b(x)?, b(y)?),
            BinOp::Div => check::Expr::Arith(Arith::Div, b(x)?, b(y)?),
        },
        Expr::Compare(x, op, y, None) => {
            let op = match op {
                CmpOp::Eq => Cmp::Eq,
                CmpOp::Ne => Cmp::Ne,
                CmpOp::Lt => Cmp::Lt,
                CmpOp::Le => Cmp::Le,
                CmpOp::Gt => Cmp::Gt,
                CmpOp::Ge => Cmp::Ge,
            };
            check::Expr::Cmp(op, b(x)?, b(y)?)
        }
        Expr::And(x, y) => check::Expr::And(b(x)?, b(y)?),
        Expr::Or(x, y) => check::Expr::Or(b(x)?, b(y)?),
        Expr::Not(x) => check::Expr::Not(b(x)?),
        Expr::IsNull(x, not) => check::Expr::IsNull(b(x)?, *not),
        Expr::Between(x, lo, hi, not) => check::Expr::Between(b(x)?, b(lo)?, b(hi)?, *not),
        Expr::InList(x, list, not) => {
            check::Expr::In(b(x)?, list.iter().map(go).collect::<Result<_>>()?, *not)
        }
        Expr::Like(x, p, esc, not) => {
            check::Expr::Like(b(x)?, b(p)?, esc.as_deref().map(b).transpose()?, *not)
        }
        Expr::Function(f) if !f.distinct && !f.star => {
            let Some(func) = Func::by_name(&f.name) else {
                return Err(SqlError::Unsupported {
                    what: format!("function {} in a CHECK condition", f.name),
                });
            };
            let args = f.args.iter().map(go).collect::<Result<Vec<_>>>()?;
            let ok = match func {
                Func::Substr => (2..=3).contains(&args.len()),
                Func::Mod => args.len() == 2,
                Func::Coalesce => args.len() >= 2,
                _ => args.len() == 1,
            };
            if !ok {
                return Err(err(
                    -170,
                    "42605",
                    format!("wrong number of arguments for {}", f.name),
                ));
            }
            check::Expr::Call(func, args)
        }
        // IBM refuses these in a check: registers, subqueries, host
        // variables and markers, sequences.
        Expr::Register(_) => return Err(invalid(cname, "a special register")),
        Expr::Subquery(_)
        | Expr::InSubquery(..)
        | Expr::Exists(..)
        | Expr::Compare(_, _, _, Some(_)) => {
            return Err(invalid(cname, "a subquery"));
        }
        Expr::Param(_)
        | Expr::Host { .. }
        | Expr::NextValue(_)
        | Expr::PrevValue(_)
        | Expr::Default => {
            return Err(invalid(cname, "a variable"));
        }
        _ => {
            return Err(SqlError::Unsupported {
                what: "that expression in a CHECK condition".into(),
            });
        }
    })
}
