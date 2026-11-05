// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Expression evaluation over rows: columns, arithmetic, predicates with
//! three-valued logic, `CASE`, `CAST`, functions, aggregates over a group
//! and subqueries run against the outer rows.

use std::cmp::Ordering;
use std::rc::Rc;

use db::record::datetime::{Date, Time, days_in_month};
use dds::FieldDef;
use qdecimal::Decimal;
use store::txn::Txn;

use super::func;
use super::query::run_query;
use super::{ColumnDesc, SqlContext};
use crate::ast::{
    BinOp, CmpOp, DurationUnit, Expr, Function, Literal, Quantifier, Query, Register, TypeSpec,
};
use crate::error::{Result, SqlError};
use crate::value::{Timestamp, Value};

/// A row of values.
pub type Row = Vec<Value>;

/// One column of a row.
#[derive(Clone, Debug)]
pub struct ColInfo {
    /// Column name, upper case.
    pub name: String,
    /// Table or correlation name, upper case.
    pub table: Option<String>,
    /// The field, when the column is one.
    pub field: Option<FieldDef>,
    /// Nullable.
    pub nullable: bool,
    /// Not shown by `*`: the relative record number.
    pub hidden: bool,
}

/// The columns of a row.
#[derive(Clone, Debug, Default)]
pub struct Schema {
    /// Columns, in row order.
    pub cols: Vec<ColInfo>,
}

impl Schema {
    /// The column `table.name` (or `name`), or an ambiguity error.
    pub fn find(&self, table: Option<&str>, name: &str) -> Result<Option<usize>> {
        let mut found = None;
        let mut hidden = None;
        for (i, c) in self.cols.iter().enumerate() {
            // A column by its system name or its SQL name.
            let named = c.name == name || c.field.as_ref().is_some_and(|f| f.alias.as_deref() == Some(name) || f.name.to_string() == name);
            if !named || table.is_some_and(|t| c.table.as_deref() != Some(t)) {
                continue;
            }
            // Unqualified, a hidden column (a column of JOIN USING
            // behind its coalesced one) yields to a visible one.
            if c.hidden && table.is_none() {
                if hidden.is_some() && found.is_none() {
                    hidden = Some(None);
                } else if hidden.is_none() {
                    hidden = Some(Some(i));
                }
                continue;
            }
            if found.is_some() {
                return Err(SqlError::AmbiguousColumn { name: name.into() });
            }
            found = Some(i);
        }
        match (found, hidden) {
            (Some(i), _) | (None, Some(Some(i))) => Ok(Some(i)),
            (None, Some(None)) => Err(SqlError::AmbiguousColumn { name: name.into() }),
            (None, None) => Ok(None),
        }
    }

    /// Whether a table name is one of ours.
    #[must_use]
    pub fn has_table(&self, table: &str) -> bool {
        self.cols.iter().any(|c| c.table.as_deref() == Some(table))
    }

    /// The result-set description of the visible columns.
    #[must_use]
    pub fn describe(&self) -> Vec<ColumnDesc> {
        self.cols
            .iter()
            .filter(|c| !c.hidden)
            .map(|c| ColumnDesc {
                name: c.name.clone(),
                data_type: c.field.as_ref().map_or("VARCHAR(32767)".into(), sql_type),
                nullable: c.nullable,
            })
            .collect()
    }
}

/// The SQL type of a field, as the catalog spells it.
#[must_use]
pub fn sql_type(f: &FieldDef) -> String {
    use dds::DataType as D;
    match f.data_type {
        D::Char | D::Hex => {
            if f.varlen.is_some() {
                format!("VARCHAR({})", f.length)
            } else {
                format!("CHAR({})", f.length)
            }
        }
        D::Graphic | D::DbcsOpen => {
            if f.varlen.is_some() {
                format!("VARGRAPHIC({})", f.length)
            } else {
                format!("GRAPHIC({})", f.length)
            }
        }
        D::Zoned => format!("NUMERIC({}, {})", f.length, f.decimals),
        D::Packed => format!("DECIMAL({}, {})", f.length, f.decimals),
        D::Binary => match f.length {
            0..=4 => "SMALLINT".into(),
            5..=9 => "INTEGER".into(),
            _ => "BIGINT".into(),
        },
        D::Float => match f.precision {
            dds::FloatPrecision::Single => "REAL".into(),
            dds::FloatPrecision::Double => "DOUBLE".into(),
        },
        D::Date => "DATE".into(),
        D::Time => "TIME".into(),
        D::Timestamp if f.length == 26 => "TIMESTAMP".into(),
        D::Timestamp => format!("TIMESTAMP({})", D::timestamp_precision(f.length)),
        D::DecFloat => format!("DECFLOAT({})", if f.length <= 16 { 16 } else { 34 }),
        D::Boolean => "BOOLEAN".into(),
        t => format!("{}({})", t.sql_name(f.varlen.is_some()), f.length),
    }
}

/// The rows in scope: the innermost frame last, plus the rows of the
/// current group when aggregates are being evaluated.
#[derive(Clone, Default)]
pub struct Env {
    /// Frames, outermost first.
    pub frames: Vec<(Rc<Schema>, Row)>,
    /// Rows of the group the innermost frame stands for.
    pub group: Option<Rc<Vec<Row>>>,
}

impl Env {
    /// With another frame inside.
    #[must_use]
    pub fn with(&self, schema: Rc<Schema>, row: Row) -> Self {
        let mut e = self.clone();
        e.frames.push((schema, row));
        e.group = None;
        e
    }

    /// The innermost frame's row replaced.
    #[must_use]
    pub fn replaced(&self, row: Row) -> Self {
        let mut e = self.clone();
        if let Some(last) = e.frames.last_mut() {
            last.1 = row;
        }
        e.group = None;
        e
    }
}

/// Aggregate function names.
pub const AGGREGATES: &[&str] = &["COUNT", "SUM", "AVG", "MIN", "MAX", "COUNT_BIG"];

/// Whether an expression holds an aggregate call outside a subquery.
#[must_use]
pub fn has_aggregate(e: &Expr) -> bool {
    match e {
        Expr::Function(f) => super::agg::is_aggregate(f) || f.args.iter().any(has_aggregate),
        Expr::WithinGroup(_) => true,
        // A window function is not the group's aggregate; its arguments
        // may hold one.
        Expr::Over(o) => {
            let inner = match &o.func {
                Expr::Function(f) => f.args.iter().any(has_aggregate),
                Expr::WithinGroup(w) => w.func.args.iter().any(has_aggregate),
                e => has_aggregate(e),
            };
            inner
                || o.window.partition.iter().any(has_aggregate)
                || o.window.order.iter().any(|i| has_aggregate(&i.expr))
        }
        Expr::Neg(a)
        | Expr::Not(a)
        | Expr::IsNull(a, _)
        | Expr::Cast(a, _)
        | Expr::Duration(a, _) => has_aggregate(a),
        Expr::Binary(a, _, b) | Expr::And(a, b) | Expr::Or(a, b) | Expr::Compare(a, _, b, _) => {
            has_aggregate(a) || has_aggregate(b)
        }
        Expr::Between(a, b, c, _) => has_aggregate(a) || has_aggregate(b) || has_aggregate(c),
        Expr::InList(a, list, _) => has_aggregate(a) || list.iter().any(has_aggregate),
        Expr::Like(a, p, _, _) => has_aggregate(a) || has_aggregate(p),
        Expr::Case {
            operand,
            whens,
            otherwise,
        } => {
            operand.as_deref().is_some_and(has_aggregate)
                || whens
                    .iter()
                    .any(|(w, t)| has_aggregate(w) || has_aggregate(t))
                || otherwise.as_deref().is_some_and(has_aggregate)
        }
        Expr::Row(items) => items.iter().any(has_aggregate),
        _ => false,
    }
}

/// Evaluates expressions for one statement.
pub struct Evaluator<'a> {
    /// The job's context.
    pub ctx: &'a mut SqlContext,
    /// Values for the parameter markers.
    pub params: &'a [Value],
}

fn overflow(e: impl std::fmt::Display) -> SqlError {
    SqlError::Overflow {
        what: e.to_string(),
    }
}

/// Add months to a date, clamping the day to the month.
#[must_use]
pub fn add_months(d: Date, months: i64) -> Option<Date> {
    let total = i64::from(d.year) * 12 + i64::from(d.month) - 1 + months;
    if total < 0 {
        return None;
    }
    let year = u16::try_from(total / 12).ok()?;
    let month = u8::try_from(total % 12 + 1).ok()?;
    let day = d.day.min(days_in_month(year, month));
    Date::new(year, month, day)
}

/// Days since 1970-01-01 of a civil date.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Days since 0001-01-01 (day 1), as `DAYS` counts.
#[must_use]
pub fn days_of(d: Date) -> i64 {
    days_from_civil(i64::from(d.year), i64::from(d.month), i64::from(d.day)) + 719_163
}

/// The date `days` after day 1.
#[must_use]
pub fn date_from_days(days: i64) -> Option<Date> {
    let z = days - 719_163 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = u8::try_from(doy - (153 * mp + 2) / 5 + 1).ok()?;
    let month = u8::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).ok()?;
    let year = u16::try_from(yoe + era * 400 + i64::from(month <= 2)).ok()?;
    Date::new(year, month, day)
}

impl Evaluator<'_> {
    /// A three-valued truth: `None` is unknown.
    #[allow(clippy::too_many_lines, clippy::many_single_char_names)]
    pub fn truth(&mut self, txn: &mut Txn<'_>, e: &Expr, env: &Env) -> Result<Option<bool>> {
        match e {
            Expr::And(a, b) => {
                let x = self.truth(txn, a, env)?;
                if x == Some(false) {
                    return Ok(Some(false));
                }
                let y = self.truth(txn, b, env)?;
                Ok(match (x, y) {
                    (_, Some(false)) => Some(false),
                    (Some(true), Some(true)) => Some(true),
                    _ => None,
                })
            }
            Expr::Or(a, b) => {
                let x = self.truth(txn, a, env)?;
                if x == Some(true) {
                    return Ok(Some(true));
                }
                let y = self.truth(txn, b, env)?;
                Ok(match (x, y) {
                    (_, Some(true)) => Some(true),
                    (Some(false), Some(false)) => Some(false),
                    _ => None,
                })
            }
            Expr::Not(a) => Ok(self.truth(txn, a, env)?.map(|b| !b)),
            Expr::IsNull(a, not) => {
                let v = self.eval(txn, a, env)?;
                Ok(Some(v.is_null() != *not))
            }
            Expr::Compare(a, op, b, None) if is_row(a) || is_row(b) => {
                if !matches!(op, CmpOp::Eq | CmpOp::Ne) {
                    return Err(SqlError::RowComparison { op: cmp_text(*op).into() });
                }
                let x = self.row_values(txn, a, env)?;
                let y = self.row_values(txn, b, env)?;
                let eq = row_eq(&x, &y)?;
                Ok(if *op == CmpOp::Eq { eq } else { eq.map(|b| !b) })
            }
            Expr::InList(a, list, not) if is_row(a) => {
                let x = self.row_values(txn, a, env)?;
                let mut unknown = false;
                for item in list {
                    let y = self.row_values(txn, item, env)?;
                    match row_eq(&x, &y)? {
                        Some(true) => return Ok(Some(!*not)),
                        None => unknown = true,
                        Some(false) => {}
                    }
                }
                Ok(if unknown { None } else { Some(*not) })
            }
            Expr::InSubquery(a, q, not) if is_row(a) => {
                let x = self.row_values(txn, a, env)?;
                let rows = self.subquery(txn, q, env)?;
                let mut unknown = false;
                for r in rows.iter() {
                    match row_eq(&x, r)? {
                        Some(true) => return Ok(Some(!*not)),
                        None => unknown = true,
                        Some(false) => {}
                    }
                }
                Ok(if unknown { None } else { Some(*not) })
            }
            Expr::Compare(a, op, b, None) => {
                let x = self.eval(txn, a, env)?;
                let y = self.eval(txn, b, env)?;
                compare(&x, *op, &y)
            }
            Expr::Compare(a, op, b, Some(q)) => {
                let x = self.eval(txn, a, env)?;
                let rows = self.subquery_rows(txn, b, env)?;
                let mut unknown = false;
                for r in rows.iter() {
                    match compare(&x, *op, &r[0])? {
                        Some(true) if *q == Quantifier::Any => return Ok(Some(true)),
                        Some(false) if *q == Quantifier::All => return Ok(Some(false)),
                        None => unknown = true,
                        _ => {}
                    }
                }
                Ok(if unknown {
                    None
                } else {
                    Some(*q == Quantifier::All)
                })
            }
            Expr::Between(a, lo, hi, not) => {
                let x = self.eval(txn, a, env)?;
                let l = self.eval(txn, lo, env)?;
                let h = self.eval(txn, hi, env)?;
                let r = match (compare(&x, CmpOp::Ge, &l)?, compare(&x, CmpOp::Le, &h)?) {
                    (Some(false), _) | (_, Some(false)) => Some(false),
                    (Some(true), Some(true)) => Some(true),
                    _ => None,
                };
                Ok(r.map(|b| b != *not))
            }
            Expr::InList(a, list, not) => {
                let x = self.eval(txn, a, env)?;
                let mut unknown = false;
                for item in list {
                    let v = self.eval(txn, item, env)?;
                    match compare(&x, CmpOp::Eq, &v)? {
                        Some(true) => return Ok(Some(!*not)),
                        None => unknown = true,
                        Some(false) => {}
                    }
                }
                Ok(if unknown { None } else { Some(*not) })
            }
            Expr::InSubquery(a, q, not) => {
                let x = self.eval(txn, a, env)?;
                let rows = self.subquery(txn, q, env)?;
                let mut unknown = false;
                for r in rows.iter() {
                    match compare(&x, CmpOp::Eq, &r[0])? {
                        Some(true) => return Ok(Some(!*not)),
                        None => unknown = true,
                        Some(false) => {}
                    }
                }
                Ok(if unknown { None } else { Some(*not) })
            }
            Expr::Like(a, p, esc, not) => {
                let x = self.eval(txn, a, env)?;
                let pat = self.eval(txn, p, env)?;
                let escape = match esc {
                    Some(e) => Some(self.eval(txn, e, env)?),
                    None => None,
                };
                if x.is_null() || pat.is_null() {
                    return Ok(None);
                }
                let esc_char = escape.and_then(|e| e.as_text().chars().next());
                Ok(Some(like(&x.as_text(), &pat.as_text(), esc_char) != *not))
            }
            Expr::Exists(q, not) => {
                let rows = self.subquery(txn, q, env)?;
                Ok(Some(rows.is_empty() == *not))
            }
            // REGEXP_LIKE is a predicate only.
            Expr::Function(f) if f.name == "REGEXP_LIKE" => {
                let mut args = Vec::with_capacity(f.args.len());
                for a in &f.args {
                    args.push(self.eval(txn, a, env)?);
                }
                Ok(match func::call(&f.name, &args, self.ctx)? {
                    Value::Bool(b) => Some(b),
                    _ => None,
                })
            }
            other => {
                let v = self.eval(txn, other, env)?;
                Ok(match v {
                    Value::Null => None,
                    Value::Num(d) => Some(!d.is_zero()),
                    Value::Double(_) | Value::Real(_) => Some(v.as_bool()?),
                    Value::Bool(b) => Some(b),
                    v => {
                        return Err(SqlError::TypeMismatch {
                            what: format!("{} where a condition is needed", v.type_name()),
                        });
                    }
                })
            }
        }
    }

    /// A subquery's rows: the statement's kept answer when it has one
    /// (`SqlContext::answers`), else run against the outer rows.
    pub(crate) fn subquery(&mut self, txn: &mut Txn<'_>, q: &Query, env: &Env) -> Result<Rc<Vec<Row>>> {
        let Some(a) = self.ctx.answers.as_mut() else {
            return Ok(Rc::new(run_query(txn, self.ctx, self.params, q, env)?.rows));
        };
        if let Some(rows) = a.lookup(q) {
            return Ok(rows);
        }
        let entered = a.enter();
        let r = run_query(txn, self.ctx, self.params, q, env).map(|rs| Rc::new(rs.rows));
        if let Some(a) = self.ctx.answers.as_mut() {
            a.leave(entered, q, env.frames.len(), r.as_ref().ok());
        }
        r
    }

    fn subquery_rows(&mut self, txn: &mut Txn<'_>, e: &Expr, env: &Env) -> Result<Rc<Vec<Row>>> {
        match e {
            Expr::Subquery(q) => self.subquery(txn, q, env),
            _ => Err(SqlError::Unsupported {
                what: "a quantified comparison without a subquery".into(),
            }),
        }
    }

    /// The value of an expression bound for a `DECFLOAT`. An exponent
    /// literal is a double there too, its exact binary value converted,
    /// as the primary converts it (`DECFLOAT(1.5E-30)` is
    /// `1.499999999999999949842322870776844E-30`).
    pub fn eval_decfloat(&mut self, txn: &mut Txn<'_>, e: &Expr, env: &Env) -> Result<Value> {
        self.eval(txn, e, env)
    }

    /// The value of an expression.
    #[allow(clippy::too_many_lines, clippy::many_single_char_names)]
    pub fn eval(&mut self, txn: &mut Txn<'_>, e: &Expr, env: &Env) -> Result<Value> {
        match e {
            Expr::Literal(l) => literal(l),
            Expr::Column(c) => {
                let name = c.column.value.as_str();
                let table = c.table.as_ref().map(|t| t.value.as_str());
                for (fi, (schema, row)) in env.frames.iter().enumerate().rev() {
                    if let Some(i) = schema.find(table, name)? {
                        if let Some(a) = self.ctx.answers.as_mut() {
                            a.touch(fi);
                        }
                        return Ok(row[i].clone());
                    }
                    if table.is_some_and(|t| schema.has_table(t)) {
                        break;
                    }
                }
                // Not a column: a built-in global variable of the path's QSYS2.
                if table.is_none()
                    && !c.column.delimited
                    && let Some(n) = super::registers::global(Some("QSYS2"), name)
                {
                    return Ok(self.ctx.registers.value(n));
                }
                // Not a column: a global variable.
                if let Some(v) = super::var::reference(txn, self.ctx, table, name, c.column.delimited)? {
                    return Ok(v);
                }
                Err(SqlError::ColumnNotFound {
                    name: match table {
                        Some(t) => format!("{t}.{name}"),
                        None => name.to_string(),
                    },
                })
            }
            Expr::Param(i) => self
                .params
                .get(*i)
                .cloned()
                .ok_or_else(|| SqlError::ValueCount {
                    what: format!("parameter {} has no value", i + 1),
                }),
            Expr::Host { name, .. } => Err(SqlError::Unsupported {
                what: format!("host variable :{name} outside a program"),
            }),
            Expr::Register(r) => Ok(self.register(*r)),
            Expr::Neg(a) => match self.eval(txn, a, env)? {
                Value::Null => Ok(Value::Null),
                Value::DecFloat(d) => Ok(Value::DecFloat(d.negated())),
                Value::Double(x) => Ok(Value::Double(-x)),
                Value::Real(x) => Ok(Value::Real(-x)),
                v => Ok(Value::Num(v.as_decimal()?.negated())),
            },
            Expr::Binary(a, op, b) => {
                // A labeled duration on either side of + or -.
                if let Expr::Duration(inner, unit) = &**b
                    && matches!(op, BinOp::Add | BinOp::Sub)
                {
                    let x = self.eval(txn, a, env)?;
                    let n = self.eval(txn, inner, env)?;
                    if x.is_null() || n.is_null() {
                        return Ok(Value::Null);
                    }
                    return super::datetime::labeled(&x, &n, *op == BinOp::Sub, *unit);
                }
                if let Expr::Duration(inner, unit) = &**a
                    && *op == BinOp::Add
                {
                    let x = self.eval(txn, b, env)?;
                    let n = self.eval(txn, inner, env)?;
                    if x.is_null() || n.is_null() {
                        return Ok(Value::Null);
                    }
                    return super::datetime::labeled(&x, &n, false, *unit);
                }
                let x = self.eval(txn, a, env)?;
                let y = self.eval(txn, b, env)?;
                // A date, time or timestamp duration: a decimal of its shape.
                if matches!(op, BinOp::Add | BinOp::Sub)
                    && let Some(r) = self.unlabeled(&x, *op, &y, a, b, env)
                {
                    return r;
                }
                super::cast::arith::binary(&x, *op, &y, self.ctx.decfloat_rounding)
            }
            Expr::Duration(a, unit) => {
                // A labeled duration stands alone only inside arithmetic;
                // elsewhere it is its number.
                let _ = unit;
                self.eval(txn, a, env)
            }
            Expr::Case {
                operand,
                whens,
                otherwise,
            } => {
                let op = match operand {
                    Some(o) => Some(self.eval(txn, o, env)?),
                    None => None,
                };
                for (w, t) in whens {
                    let hit = match &op {
                        Some(v) => {
                            let wv = self.eval(txn, w, env)?;
                            compare(v, CmpOp::Eq, &wv)? == Some(true)
                        }
                        None => self.truth(txn, w, env)? == Some(true),
                    };
                    if hit {
                        return self.eval(txn, t, env);
                    }
                }
                match otherwise {
                    Some(o) => self.eval(txn, o, env),
                    None => Ok(Value::Null),
                }
            }
            Expr::Cast(a, t) => {
                let v = self.eval(txn, a, env)?;
                super::cast::cast(&v, t, self.ctx.decfloat_rounding)
            }
            Expr::Function(f) => self.function(txn, f, env),
            Expr::Subquery(q) => {
                let rows = self.subquery(txn, q, env)?;
                match rows.len() {
                    0 => Ok(Value::Null),
                    1 => Ok(rows[0][0].clone()),
                    _ => Err(SqlError::TooManyRows),
                }
            }
            Expr::Row(items) if items.len() == 1 => self.eval(txn, &items[0], env),
            Expr::Row(_) => Err(SqlError::Unsupported {
                what: "a row value where a single value is needed".into(),
            }),
            Expr::NextValue(n) => super::seq::next_value(txn, self.ctx, n),
            Expr::PrevValue(n) => super::seq::previous_value(txn, self.ctx, n),
            // A column named DEFAULT (SYSVARIABLES has one), else refused.
            Expr::Default => match env.frames.iter().rev().find_map(|(schema, row)| schema.find(None, "DEFAULT").ok().flatten().map(|i| row[i].clone())) {
                Some(v) => Ok(v),
                None => Err(SqlError::Unsupported {
                    what: "DEFAULT outside an INSERT or UPDATE value".into(),
                }),
            },
            Expr::Over(o) => self.ctx.windows.get(&o.slot).cloned().ok_or_else(|| SqlError::Misplaced {
                what: "an OLAP specification outside the select list and ORDER BY".into(),
            }),
            Expr::WithinGroup(w) => {
                if w.func.name == "ARRAY_AGG" && !self.ctx.arrays {
                    return Err(super::array::misplaced());
                }
                let Some(group) = env.group.clone() else {
                    return Err(SqlError::Misplaced { what: format!("{} where no group is being formed", w.func.name) });
                };
                self.aggregate_call(txn, &w.func, &w.order, w.overflow.as_ref(), env, &group)
            }
            Expr::And(..)
            | Expr::Or(..)
            | Expr::Not(_)
            | Expr::IsNull(..)
            | Expr::Compare(..)
            | Expr::Between(..)
            | Expr::InList(..)
            | Expr::InSubquery(..)
            | Expr::Like(..)
            // A predicate as a value is a BOOLEAN, as on IBM i 7.5.
            | Expr::Exists(..) => Ok(match self.truth(txn, e, env)? {
                None => Value::Null,
                Some(b) => Value::Bool(b),
            }),
        }
    }

    fn register(&self, r: Register) -> Value {
        let now = self.ctx.clock();
        match r {
            Register::Date => Value::Date(Date::from_unix(now)),
            Register::Time => Value::Time(Time::from_unix(now)),
            Register::Timestamp(p) => {
                let (secs, picos) = self.ctx.clock_picos();
                Value::Timestamp(Timestamp::new(Date::from_unix(secs), Time::from_unix(secs), picos).with_precision(p))
            }
            Register::User => Value::Str(self.ctx.user.clone()),
            Register::Server => Value::Str(self.ctx.registers.server.clone()),
            Register::Named(n) => self.ctx.registers.value(n),
            Register::Schema => Value::Str(
                self.ctx
                    .current_schema
                    .map_or_else(|| "*LIBL".to_string(), |n| n.to_string()),
            ),
            Register::Path => Value::Str(self.ctx.registers.path.clone().unwrap_or_else(|| match self.ctx.naming {
                crate::ast::Naming::Sys => "*LIBL".into(),
                crate::ast::Naming::Sql => format!("\"QSYS\",\"QSYS2\",\"SYSPROC\",\"SYSIBMADM\",\"{}\"", self.ctx.user),
            })),
            Register::Timezone => {
                // hhmmss of the offset from UTC, signed.
                let s = self.ctx.registers.timezone_secs;
                let a = s.abs();
                let n = a / 3600 * 10_000 + a % 3600 / 60 * 100 + a % 60;
                Value::int(if s < 0 { -n } else { n })
            }
            Register::DecfloatRounding => Value::Str(self.ctx.decfloat_rounding.name().into()),
        }
    }

    /// A function call: aggregates over the group, else a scalar.
    #[allow(clippy::too_many_lines)]
    fn function(&mut self, txn: &mut Txn<'_>, f: &Function, env: &Env) -> Result<Value> {
        if let Some(r) = super::json::call(self, txn, f, env) {
            return r;
        }
        let name = f.name.as_str();
        if name == "ARRAY_AGG" && !self.ctx.arrays {
            return Err(super::array::misplaced());
        }
        if name == "*ARRAYQ" {
            if !self.ctx.arrays {
                return Err(super::array::misplaced());
            }
            let Some(Expr::Subquery(q)) = f.args.first() else { unreachable!("ARRAY[fullselect]") };
            let rows = self.subquery(txn, q, env)?;
            return Ok(super::array::from_rows(&rows));
        }
        if super::agg::is_aggregate(f) {
            let Some(group) = env.group.clone() else {
                return Err(SqlError::Misplaced {
                    what: format!("{name} where no group is being formed"),
                });
            };
            return self.aggregate_call(txn, f, &[], None, env, &group);
        }
        if name == "REGEXP_LIKE" {
            return Err(SqlError::Syntax {
                line: 0,
                col: 0,
                found: "REGEXP_LIKE".into(),
                expected: "REGEXP_LIKE as a predicate, not a value".into(),
            });
        }
        if name == "INTERPRET"
            && let [e, Expr::Cast(_, spec)] = f.args.as_slice()
        {
            let v = self.eval(txn, e, env)?;
            return super::func::misc::interpret(&v, spec);
        }
        if (name == "*DISTINCT" || name == "*NOTDISTINCT") && f.args.len() == 2 {
            let a = self.eval(txn, &f.args[0], env)?;
            let b = self.eval(txn, &f.args[1], env)?;
            let same = not_distinct(&a, &b)?;
            return Ok(Value::Bool(if name == "*DISTINCT" { !same } else { same }));
        }
        // GROUPING of a query that is not grouped by sets.
        if name == "GROUPING" && f.args.len() == 1 {
            return Ok(Value::int(0));
        }
        if name == "RRN" {
            let Some(Expr::Column(c)) = f.args.first() else {
                return Err(SqlError::Unsupported {
                    what: "RRN needs a table name".into(),
                });
            };
            let alias = c.column.value.as_str();
            for (fi, (schema, row)) in env.frames.iter().enumerate().rev() {
                if let Some(i) = schema
                    .cols
                    .iter()
                    .position(|col| col.hidden && col.table.as_deref() == Some(alias))
                {
                    if let Some(a) = self.ctx.answers.as_mut() {
                        a.touch(fi);
                    }
                    return Ok(row[i].clone());
                }
            }
            return Err(SqlError::ColumnNotFound {
                name: format!("RRN({alias})"),
            });
        }
        let mut args = Vec::with_capacity(f.args.len());
        for (i, a) in f.args.iter().enumerate() {
            // CHAR(date, USA): the format word is a keyword, not a column.
            if i == 1
                && matches!(name, "CHAR" | "VARCHAR")
                && let Expr::Column(c) = a
                && c.table.is_none()
                && matches!(
                    c.column.value.as_str(),
                    "ISO" | "USA" | "EUR" | "JIS" | "LOCAL"
                )
            {
                args.push(Value::Str(c.column.value.clone()));
                continue;
            }
            // EXTRACT(YEAR FROM x), STRIP(s, B, c), SUBSTRING(s, n, m, OCTETS):
            // the word is a keyword.
            if let Expr::Column(c) = a
                && c.table.is_none()
                && !c.column.delimited
                && ((i == 0 && name == "EXTRACT")
                    || (i == 1 && name == "STRIP" && matches!(c.column.value.as_str(), "B" | "L" | "T" | "BOTH" | "LEADING" | "TRAILING"))
                    || (i == 3 && matches!(name, "SUBSTRING" | "SUBSTR") && matches!(c.column.value.as_str(), "OCTETS" | "CODEUNITS16" | "CODEUNITS32")))
            {
                args.push(Value::Str(c.column.value.clone()));
                continue;
            }
            if i == 0 && matches!(name, "DECFLOAT" | "NORMALIZE_DECFLOAT") {
                args.push(self.eval_decfloat(txn, a, env)?);
                continue;
            }
            args.push(self.eval(txn, a, env)?);
        }
        let v = match func::call(name, &args, self.ctx) {
            // Not built in: a user-defined function, when there is one.
            Err(SqlError::Unsupported { what }) if what == format!("function {name}") => {
                let literal: Vec<bool> = f.args.iter().map(|a| matches!(a, Expr::Literal(crate::ast::Literal::Str(_)))).collect();
                match super::routine::udf::call(txn, self.ctx, name, &args, &literal) {
                    Some(r) => r?,
                    // A schema's function that is not there.
                    None if name.contains('.') => return Err(SqlError::ObjectNotFound { name: name.to_string(), kind: "function" }),
                    None => return Err(SqlError::Unsupported { what }),
                }
            }
            r => r?,
        };
        // DIGITS of a column fills to the column's precision.
        if name == "DIGITS"
            && let Some(Expr::Column(c)) = f.args.first()
            && let Some(len) = column_length(env, c)
            && let Value::Str(s) = &v
        {
            return Ok(Value::Str(format!("{s:0>len$}")));
        }
        // CHAR of a number fills to its type's width, as a CHAR(n) of
        // that length: SMALLINT 6, INTEGER 11, BIGINT 20, DECIMAL(p, s) p + 2.
        if name == "CHAR"
            && let [a] = f.args.as_slice()
            && matches!(args.first(), Some(Value::Num(_)))
            && let Value::Str(s) = &v
        {
            use super::restype::Kind;
            let t = super::restype::infer(a, &super::restype::Scope::of_env(env));
            let width = match t.kind() {
                Kind::SmallInt => Some(6),
                Kind::Integer => Some(11),
                Kind::BigInt => Some(20),
                Kind::Decimal | Kind::Numeric => usize::try_from(t.len()).ok().map(|p| p + 2),
                _ => None,
            };
            if let Some(w) = width {
                return Ok(Value::Str(format!("{s:<w$}")));
            }
        }
        // DIGITS of an expression fills to its type's precision.
        if name == "DIGITS"
            && let Some(a) = f.args.first()
            && let Value::Str(s) = &v
        {
            let scope = super::restype::Scope::of_env(env);
            if let Some((p, _)) = super::restype::infer(a, &scope).digits() {
                let len = usize::try_from(if p == 11 { 10 } else { p }).expect("small");
                return Ok(Value::Str(format!("{s:0>len$}")));
            }
        }
        Ok(v)
    }

    /// An aggregate over the rows of a group: each row's arguments and
    /// `WITHIN GROUP` keys gathered, then folded (`agg`).
    pub(crate) fn aggregate_call(
        &mut self,
        txn: &mut Txn<'_>,
        f: &Function,
        order: &[crate::ast::OrderItem],
        overflow: Option<&crate::ast::Overflow>,
        env: &Env,
        group: &Rc<Vec<Row>>,
    ) -> Result<Value> {
        let mut rows = Vec::with_capacity(group.len());
        for row in group.iter() {
            self.ctx.tick(txn)?;
            let e = env.replaced(row.clone());
            let mut args = Vec::with_capacity(f.args.len());
            if !f.star {
                for a in &f.args {
                    args.push(self.eval(txn, a, &e)?);
                }
            }
            let mut keys = Vec::with_capacity(order.len());
            for o in order {
                keys.push(self.eval(txn, &o.expr, &e)?);
            }
            rows.push(super::agg::Input { args, keys });
        }
        let call = super::agg::Call { name: &f.name, distinct: f.distinct, star: f.star, order, overflow, exprs: &f.args };
        let mut nulls = false;
        let v = super::agg::finish(&call, rows, &mut nulls)?;
        if nulls {
            self.ctx.nulls_eliminated = true;
        }
        Ok(v)
    }
}

/// Whether `e` is a row of more than one value.
fn is_row(e: &Expr) -> bool {
    matches!(e, Expr::Row(items) if items.len() > 1)
}

fn cmp_text(op: CmpOp) -> &'static str {
    match op {
        CmpOp::Eq => "=",
        CmpOp::Ne => "<>",
        CmpOp::Lt => "<",
        CmpOp::Le => "<=",
        CmpOp::Gt => ">",
        CmpOp::Ge => ">=",
    }
}

/// Two rows equal: every pair equal; unknown when no pair differs and a
/// pair is unknown.
fn row_eq(x: &[Value], y: &[Value]) -> Result<Option<bool>> {
    if x.is_empty() || y.is_empty() {
        return Ok(None);
    }
    if x.len() != y.len() {
        return Err(SqlError::ValueCount { what: format!("a row of {} values beside one of {}", x.len(), y.len()) });
    }
    let mut unknown = false;
    for (a, b) in x.iter().zip(y) {
        match compare(a, CmpOp::Eq, b)? {
            Some(false) => return Ok(Some(false)),
            None => unknown = true,
            Some(true) => {}
        }
    }
    Ok(if unknown { None } else { Some(true) })
}

/// `a IS [NOT] DISTINCT FROM b`: nulls are equal to each other.
pub fn not_distinct(a: &Value, b: &Value) -> Result<bool> {
    Ok(match (a.is_null(), b.is_null()) {
        (true, true) => true,
        (true, false) | (false, true) => false,
        _ => compare(a, CmpOp::Eq, b)? == Some(true),
    })
}

impl Evaluator<'_> {
    /// The values of a row, a scalar, or a subquery's one row.
    fn row_values(&mut self, txn: &mut Txn<'_>, e: &Expr, env: &Env) -> Result<Vec<Value>> {
        match e {
            Expr::Row(items) => {
                let mut v = Vec::with_capacity(items.len());
                for i in items {
                    v.push(self.eval(txn, i, env)?);
                }
                Ok(v)
            }
            Expr::Subquery(q) => {
                let rows = self.subquery(txn, q, env)?;
                match rows.len() {
                    0 => Ok(Vec::new()),
                    1 => Ok(rows[0].clone()),
                    _ => Err(SqlError::TooManyRows),
                }
            }
            e => Ok(vec![self.eval(txn, e, env)?]),
        }
    }
}

/// The declared digits of a column in scope.
fn column_length(env: &Env, c: &crate::ast::ColumnRef) -> Option<usize> {
    let table = c.table.as_ref().map(|t| t.value.as_str());
    for (schema, _) in env.frames.iter().rev() {
        if let Ok(Some(i)) = schema.find(table, &c.column.value) {
            return schema.cols[i]
                .field
                .as_ref()
                .map(|f| usize::try_from(f.length).expect("fits"));
        }
    }
    None
}

/// A literal's value.
pub fn literal(l: &Literal) -> Result<Value> {
    Ok(match l {
        Literal::Str(s) => Value::Str(s.clone()),
        Literal::Hex(h) => Value::Str(ccsid::from_ebcdic(h)),
        Literal::Bin(b) => Value::Bin(b.clone()),
        Literal::Bool(b) => Value::Bool(*b),
        // A literal with an exponent is a double.
        Literal::Num(n) if n.contains(['E', 'e']) => Value::Double(
            n.parse::<f64>().ok().filter(|x| x.is_finite()).ok_or_else(|| SqlError::Literal {
                text: n.clone(),
                what: "out of range for a double".into(),
            })?,
        ),
        Literal::Num(n) => Value::Num(Decimal::parse(n).map_err(|e| SqlError::Literal {
            text: n.clone(),
            what: e.to_string(),
        })?),
        Literal::Null => Value::Null,
        Literal::Date(s) => {
            Value::Date(
                crate::value::parse_date(s).ok_or_else(|| SqlError::Literal {
                    text: s.clone(),
                    what: "not a date".into(),
                })?,
            )
        }
        Literal::Time(s) => {
            Value::Time(
                crate::value::parse_time(s).ok_or_else(|| SqlError::Literal {
                    text: s.clone(),
                    what: "not a time".into(),
                })?,
            )
        }
        Literal::Timestamp(s) => {
            Value::Timestamp(Timestamp::parse(s).ok_or_else(|| SqlError::Literal {
                text: s.clone(),
                what: "not a timestamp".into(),
            })?)
        }
    })
}

/// Compare with an operator; unknown when either side is null.
pub fn compare(a: &Value, op: CmpOp, b: &Value) -> Result<Option<bool>> {
    Ok(a.compare(b)?.map(|o| match op {
        CmpOp::Eq => o == Ordering::Equal,
        CmpOp::Ne => o != Ordering::Equal,
        CmpOp::Lt => o == Ordering::Less,
        CmpOp::Le => o != Ordering::Greater,
        CmpOp::Gt => o == Ordering::Greater,
        CmpOp::Ge => o != Ordering::Less,
    }))
}

/// Arithmetic and concatenation.
#[allow(clippy::many_single_char_names)]
pub fn binary(a: &Value, op: BinOp, b: &Value) -> Result<Value> {
    if a.is_null() || b.is_null() {
        return Ok(Value::Null);
    }
    if let Some(r) = super::datetime::binary(a, op, b) {
        return r;
    }
    if op != BinOp::Concat && (matches!(a, Value::DecFloat(_)) || matches!(b, Value::DecFloat(_))) {
        return decfloat_binary(a, op, b);
    }
    // A float with a number other than a `DECFLOAT`: a double.
    if op != BinOp::Concat && (a.is_float() || b.is_float()) {
        return float_binary(a, op, b);
    }
    match op {
        BinOp::Concat => match (a, b) {
            (Value::Bin(x), Value::Bin(y)) => Ok(Value::Bin([x.as_slice(), y].concat())),
            _ => Ok(Value::Str(format!("{}{}", a.as_text(), b.as_text()))),
        },
        BinOp::Add | BinOp::Sub => {
            let x = a.as_decimal()?;
            let y = b.as_decimal()?;
            let r = if op == BinOp::Add {
                x.add_exact(&y)
            } else {
                x.sub_exact(&y)
            };
            Ok(Value::Num(r.map_err(overflow)?))
        }
        BinOp::Mul => Ok(Value::Num(
            a.as_decimal()?
                .mul_exact(&b.as_decimal()?)
                .map_err(overflow)?,
        )),
        BinOp::Div => {
            let x = a.as_decimal()?;
            let y = b.as_decimal()?;
            if y.is_zero() {
                return Err(SqlError::DivideByZero);
            }
            let scale = (x.scale().max(y.scale()) + 6).min(31);
            Ok(Value::Num(x.div_to_scale(&y, scale).map_err(overflow)?))
        }
    }
}

/// Arithmetic with a float operand, in doubles; a result past the
/// double's range is an overflow.
fn float_binary(lhs: &Value, op: BinOp, rhs: &Value) -> Result<Value> {
    let (x, y) = (lhs.as_f64()?, rhs.as_f64()?);
    let out = match op {
        BinOp::Add => x + y,
        BinOp::Sub => x - y,
        BinOp::Mul => x * y,
        BinOp::Div if y == 0.0 => return Err(SqlError::DivideByZero),
        BinOp::Div => x / y,
        BinOp::Concat => unreachable!("not arithmetic"),
    };
    if out.is_infinite() && x.is_finite() && y.is_finite() {
        return Err(overflow("the result is past a double's range"));
    }
    Ok(Value::Double(out))
}

/// Arithmetic with a `DECFLOAT` operand: the other converted, the
/// result in the wider format.
fn decfloat_binary(a: &Value, op: BinOp, b: &Value) -> Result<Value> {
    let lhs = a.as_decfloat(a.decfloat_format())?;
    let rhs = b.as_decfloat(b.decfloat_format())?;
    let out = match op {
        BinOp::Add => qdecimal::decfloat::add(&lhs, &rhs),
        BinOp::Sub => qdecimal::decfloat::sub(&lhs, &rhs),
        BinOp::Mul => qdecimal::decfloat::mul(&lhs, &rhs),
        BinOp::Div => qdecimal::decfloat::div(&lhs, &rhs),
        BinOp::Concat => unreachable!("not arithmetic"),
    };
    out.map(Value::DecFloat).map_err(crate::value::decfloat_error)
}

/// Add a labeled duration to a date or timestamp.
pub fn add_duration(v: &Value, n: i64, unit: DurationUnit) -> Result<Value> {
    let bad = || SqlError::Overflow {
        what: "the date is out of range".into(),
    };
    match v {
        Value::Null => Ok(Value::Null),
        Value::Date(d) => Ok(Value::Date(match unit {
            DurationUnit::Years => add_months(*d, n * 12).ok_or_else(bad)?,
            DurationUnit::Months => add_months(*d, n).ok_or_else(bad)?,
            DurationUnit::Days => date_from_days(days_of(*d) + n).ok_or_else(bad)?,
            _ => {
                return Err(SqlError::Duration {
                    what: "a time duration on a date".into(),
                });
            }
        })),
        Value::Timestamp(t) => {
            let mut secs = i64::from(t.time.hour) * 3600
                + i64::from(t.time.minute) * 60
                + i64::from(t.time.second);
            let mut picos = i128::from(t.picos);
            let mut date = t.date;
            match unit {
                DurationUnit::Years => date = add_months(date, n * 12).ok_or_else(bad)?,
                DurationUnit::Months => date = add_months(date, n).ok_or_else(bad)?,
                DurationUnit::Days => date = date_from_days(days_of(date) + n).ok_or_else(bad)?,
                DurationUnit::Hours => secs += n * 3600,
                DurationUnit::Minutes => secs += n * 60,
                DurationUnit::Seconds => secs += n,
                DurationUnit::Microseconds => picos += i128::from(n) * 1_000_000,
            }
            let unit = i128::from(crate::value::PICOS);
            secs += i64::try_from(picos.div_euclid(unit)).map_err(|_| bad())?;
            let picos = u64::try_from(picos.rem_euclid(unit)).expect("below a second");
            let day_shift = secs.div_euclid(86_400);
            secs = secs.rem_euclid(86_400);
            if day_shift != 0 {
                date = date_from_days(days_of(date) + day_shift).ok_or_else(bad)?;
            }
            Ok(Value::Timestamp(Timestamp {
                date,
                time: Time::new(
                    u8::try_from(secs / 3600).expect("hour"),
                    u8::try_from(secs % 3600 / 60).expect("minute"),
                    u8::try_from(secs % 60).expect("second"),
                )
                .expect("valid"),
                picos,
                precision: t.precision,
            }))
        }
        Value::Time(t) => {
            let mut secs =
                i64::from(t.hour) * 3600 + i64::from(t.minute) * 60 + i64::from(t.second);
            match unit {
                DurationUnit::Hours => secs += n * 3600,
                DurationUnit::Minutes => secs += n * 60,
                DurationUnit::Seconds => secs += n,
                _ => {
                    return Err(SqlError::Duration {
                        what: "a date duration on a time".into(),
                    });
                }
            }
            secs = secs.rem_euclid(86_400);
            Ok(Value::Time(
                Time::new(
                    u8::try_from(secs / 3600).expect("hour"),
                    u8::try_from(secs % 3600 / 60).expect("minute"),
                    u8::try_from(secs % 60).expect("second"),
                )
                .expect("valid"),
            ))
        }
        other => Err(SqlError::TypeMismatch {
            what: format!("a duration on {}", other.type_name()),
        }),
    }
}

fn like_match(t: &[char], p: &[char], esc: Option<char>) -> bool {
    match p.first() {
        None => t.is_empty(),
        Some(&c) if Some(c) == esc => match p.get(1) {
            Some(&lit) => t.first() == Some(&lit) && like_match(&t[1..], &p[2..], esc),
            None => false,
        },
        Some('%') => (0..=t.len()).any(|i| like_match(&t[i..], &p[1..], esc)),
        Some('_') => !t.is_empty() && like_match(&t[1..], &p[1..], esc),
        Some(&c) => t.first() == Some(&c) && like_match(&t[1..], &p[1..], esc),
    }
}

/// `LIKE` with `%`, `_` and an escape character.
#[must_use]
pub fn like(text: &str, pattern: &str, escape: Option<char>) -> bool {
    let t: Vec<char> = text.trim_end().chars().collect();
    let p: Vec<char> = pattern.chars().collect();
    like_match(&t, &p, escape)
}

/// `CAST`.
pub fn cast(v: &Value, t: &TypeSpec) -> Result<Value> {
    super::cast::cast(v, t, qdecimal::DfRounding::HalfEven)
}


impl Evaluator<'_> {
    /// `date ± DECIMAL(8,0)`, `time ± DECIMAL(6,0)`, `timestamp ±
    /// DECIMAL(14+s,s)` (and the duration first for `+`); `None` when the
    /// operands are not such a pair.
    #[allow(clippy::many_single_char_names, clippy::unused_self)]
    fn unlabeled(&self, x: &Value, op: BinOp, y: &Value, a: &Expr, b: &Expr, env: &Env) -> Option<Result<Value>> {
        use super::datetime::DurationKind as K;
        let (v, d, de, sub) = match (x, y) {
            (Value::Date(_) | Value::Time(_) | Value::Timestamp(_), Value::Num(d)) => (x, d, b, op == BinOp::Sub),
            (Value::Num(d), Value::Date(_) | Value::Time(_) | Value::Timestamp(_)) if op == BinOp::Add => (y, d, a, false),
            _ => return None,
        };
        let scope = super::restype::Scope::of_env(env);
        let t = super::restype::infer(de, &scope);
        if !matches!(t.kind(), super::restype::Kind::Decimal | super::restype::Kind::Numeric) {
            return None;
        }
        let kind = match (v, t.len(), t.scale()) {
            (Value::Date(_) | Value::Timestamp(_), 8, 0) => K::Date,
            (Value::Time(_) | Value::Timestamp(_), 6, 0) => K::Time,
            (Value::Timestamp(_), p, s) if p == 14 + u32::from(s) => K::Timestamp,
            _ => return None,
        };
        Some(super::datetime::with_duration(v, d, sub, kind))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn like_and_days() {
        assert!(like("Alice", "A%", None));
        assert!(like("Alice", "_lic_", None));
        assert!(!like("Alice", "_lice_", None));
        assert!(like("a_b", "a\\_b", Some('\\')));
        assert!(!like("axb", "a\\_b", Some('\\')));
        assert!(like("abc   ", "abc", None));
        let d = Date::new(2026, 3, 1).unwrap();
        assert_eq!(date_from_days(days_of(d)).unwrap(), d);
        assert_eq!(
            date_from_days(days_of(d) - 1).unwrap(),
            Date::new(2026, 2, 28).unwrap()
        );
        assert_eq!(days_of(Date::new(1, 1, 1).unwrap()), 1);
        assert_eq!(
            add_months(Date::new(2026, 1, 31).unwrap(), 1).unwrap(),
            Date::new(2026, 2, 28).unwrap()
        );
    }
}
