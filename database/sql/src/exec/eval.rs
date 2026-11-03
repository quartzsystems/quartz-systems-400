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
use qdecimal::{Decimal, Rounding, Shape};
use store::txn::Txn;

use super::func;
use super::query::run_query;
use super::{ColumnDesc, SqlContext};
use crate::ast::{
    BinOp, CmpOp, DataType, DurationUnit, Expr, Function, Literal, Quantifier, Register, TypeSpec,
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
        for (i, c) in self.cols.iter().enumerate() {
            // A column by its system name or its SQL name.
            let named = c.name == name || c.field.as_ref().is_some_and(|f| f.alias.as_deref() == Some(name));
            if !named || table.is_some_and(|t| c.table.as_deref() != Some(t)) {
                continue;
            }
            if found.is_some() {
                return Err(SqlError::AmbiguousColumn { name: name.into() });
            }
            found = Some(i);
        }
        Ok(found)
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
        Expr::Function(f) => {
            (AGGREGATES.contains(&f.name.as_str()) && (f.star || f.args.len() == 1))
                || f.args.iter().any(has_aggregate)
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
            Expr::Compare(a, op, b, None) => {
                let x = self.eval(txn, a, env)?;
                let y = self.eval(txn, b, env)?;
                compare(&x, *op, &y)
            }
            Expr::Compare(a, op, b, Some(q)) => {
                let x = self.eval(txn, a, env)?;
                let rows = self.subquery_rows(txn, b, env)?;
                let mut unknown = false;
                for r in rows {
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
                let rs = run_query(txn, self.ctx, self.params, q, env)?;
                let mut unknown = false;
                for r in rs.rows {
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
                let rs = run_query(txn, self.ctx, self.params, q, env)?;
                Ok(Some(rs.rows.is_empty() == *not))
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

    fn subquery_rows(&mut self, txn: &mut Txn<'_>, e: &Expr, env: &Env) -> Result<Vec<Row>> {
        match e {
            Expr::Subquery(q) => Ok(run_query(txn, self.ctx, self.params, q, env)?.rows),
            _ => Err(SqlError::Unsupported {
                what: "a quantified comparison without a subquery".into(),
            }),
        }
    }

    /// The value of an expression bound for a `DECFLOAT`: an exponent
    /// literal is read from its text ([`decfloat_literal`]).
    pub fn eval_decfloat(&mut self, txn: &mut Txn<'_>, e: &Expr, env: &Env) -> Result<Value> {
        match decfloat_literal(e) {
            Some(v) => v,
            None => self.eval(txn, e, env),
        }
    }

    /// The value of an expression.
    #[allow(clippy::too_many_lines, clippy::many_single_char_names)]
    pub fn eval(&mut self, txn: &mut Txn<'_>, e: &Expr, env: &Env) -> Result<Value> {
        match e {
            Expr::Literal(l) => literal(l),
            Expr::Column(c) => {
                let name = c.column.value.as_str();
                let table = c.table.as_ref().map(|t| t.value.as_str());
                for (schema, row) in env.frames.iter().rev() {
                    if let Some(i) = schema.find(table, name)? {
                        return Ok(row[i].clone());
                    }
                    if table.is_some_and(|t| schema.has_table(t)) {
                        break;
                    }
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
                    let n = n.as_i64()?;
                    return add_duration(&x, if *op == BinOp::Sub { -n } else { n }, *unit);
                }
                if let Expr::Duration(inner, unit) = &**a
                    && *op == BinOp::Add
                {
                    let x = self.eval(txn, b, env)?;
                    let n = self.eval(txn, inner, env)?;
                    if x.is_null() || n.is_null() {
                        return Ok(Value::Null);
                    }
                    return add_duration(&x, n.as_i64()?, *unit);
                }
                let mut x = self.eval(txn, a, env)?;
                let mut y = self.eval(txn, b, env)?;
                // Beside a `DECFLOAT`, an exponent literal keeps its exponent.
                if *op != BinOp::Concat {
                    if matches!(x, Value::DecFloat(_))
                        && let Some(v) = decfloat_literal(b)
                    {
                        y = v?;
                    } else if matches!(y, Value::DecFloat(_))
                        && let Some(v) = decfloat_literal(a)
                    {
                        x = v?;
                    }
                }
                binary(&x, *op, &y)
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
                let v = match t.data_type {
                    DataType::DecFloat(_) => self.eval_decfloat(txn, a, env)?,
                    _ => self.eval(txn, a, env)?,
                };
                cast(&v, t)
            }
            Expr::Function(f) => self.function(txn, f, env),
            Expr::Subquery(q) => {
                let rs = run_query(txn, self.ctx, self.params, q, env)?;
                match rs.rows.len() {
                    0 => Ok(Value::Null),
                    1 => Ok(rs.rows[0][0].clone()),
                    _ => Err(SqlError::TooManyRows),
                }
            }
            Expr::Row(items) if items.len() == 1 => self.eval(txn, &items[0], env),
            Expr::Row(_) => Err(SqlError::Unsupported {
                what: "a row value where a single value is needed".into(),
            }),
            Expr::Default => Err(SqlError::Unsupported {
                what: "DEFAULT outside an INSERT or UPDATE value".into(),
            }),
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
            Register::Server => Value::Str("QS400".into()),
            Register::Schema => Value::Str(
                self.ctx
                    .current_schema
                    .map_or_else(|| "*LIBL".to_string(), |n| n.to_string()),
            ),
            Register::Path => Value::Str("*LIBL".into()),
            Register::Timezone => Value::int(0),
        }
    }

    /// A function call: aggregates over the group, else a scalar.
    fn function(&mut self, txn: &mut Txn<'_>, f: &Function, env: &Env) -> Result<Value> {
        let name = f.name.as_str();
        if AGGREGATES.contains(&name) && (f.star || f.args.len() == 1) {
            let Some(group) = env.group.clone() else {
                return Err(SqlError::Unsupported {
                    what: format!("{name} where no group is being formed"),
                });
            };
            return self.aggregate(txn, f, env, &group);
        }
        if name == "RRN" {
            let Some(Expr::Column(c)) = f.args.first() else {
                return Err(SqlError::Unsupported {
                    what: "RRN needs a table name".into(),
                });
            };
            let alias = c.column.value.as_str();
            for (schema, row) in env.frames.iter().rev() {
                if let Some(i) = schema
                    .cols
                    .iter()
                    .position(|col| col.hidden && col.table.as_deref() == Some(alias))
                {
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
            if i == 0 && matches!(name, "DECFLOAT" | "NORMALIZE_DECFLOAT") {
                args.push(self.eval_decfloat(txn, a, env)?);
                continue;
            }
            args.push(self.eval(txn, a, env)?);
        }
        let v = func::call(name, &args, self.ctx)?;
        // DIGITS of a column fills to the column's precision.
        if name == "DIGITS"
            && let Some(Expr::Column(c)) = f.args.first()
            && let Some(len) = column_length(env, c)
            && let Value::Str(s) = &v
        {
            return Ok(Value::Str(format!("{s:0>len$}")));
        }
        Ok(v)
    }

    fn aggregate(
        &mut self,
        txn: &mut Txn<'_>,
        f: &Function,
        env: &Env,
        group: &Rc<Vec<Row>>,
    ) -> Result<Value> {
        let mut values = Vec::new();
        if f.star {
            return Ok(Value::int(i64::try_from(group.len()).expect("fits")));
        }
        for row in group.iter() {
            self.ctx.tick(txn)?;
            let e = env.replaced(row.clone());
            let v = self.eval(txn, &f.args[0], &e)?;
            if v.is_null() {
                self.ctx.nulls_eliminated = true;
            } else {
                values.push(v);
            }
        }
        if f.distinct {
            let mut seen = std::collections::HashSet::new();
            values.retain(|v| seen.insert(v.key()));
        }
        match f.name.as_str() {
            "COUNT" | "COUNT_BIG" => Ok(Value::int(i64::try_from(values.len()).expect("fits"))),
            "SUM" | "AVG" => {
                if values.is_empty() {
                    return Ok(Value::Null);
                }
                if values.iter().any(|v| matches!(v, Value::DecFloat(_)) || v.is_float()) {
                    let mut sum = values[0].clone();
                    for v in &values[1..] {
                        sum = binary(&sum, BinOp::Add, v)?;
                    }
                    if f.name == "AVG" {
                        let n = Value::int(i64::try_from(values.len()).expect("fits"));
                        return binary(&sum, BinOp::Div, &n);
                    }
                    return Ok(sum);
                }
                let mut sum = Decimal::zero();
                for v in &values {
                    sum = sum.add_exact(&v.as_decimal()?).map_err(overflow)?;
                }
                if f.name == "AVG" {
                    let n =
                        Decimal::new(i128::try_from(values.len()).expect("fits"), 0).expect("fits");
                    let scale = (sum.scale() + 6).min(31);
                    return Ok(Value::Num(sum.div_to_scale(&n, scale).map_err(overflow)?));
                }
                Ok(Value::Num(sum))
            }
            "MIN" | "MAX" => {
                let want = if f.name == "MIN" {
                    Ordering::Less
                } else {
                    Ordering::Greater
                };
                let mut best: Option<Value> = None;
                for v in values {
                    best = Some(match best {
                        None => v,
                        Some(b) => {
                            if v.compare(&b)? == Some(want) {
                                v
                            } else {
                                b
                            }
                        }
                    });
                }
                Ok(best.unwrap_or(Value::Null))
            }
            _ => unreachable!("aggregate list"),
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

/// A number literal with an exponent (`1.00E+2`, `-0E3`), or its
/// negation, as the `DECFLOAT(34)` its text spells. Such a literal is a
/// double, which has no exponent; into or beside a `DECFLOAT` the
/// exponent written is kept. `None` for any other expression.
pub fn decfloat_literal(e: &Expr) -> Option<Result<Value>> {
    match e {
        Expr::Literal(Literal::Num(n)) if n.contains(['E', 'e']) => Some(
            qdecimal::DecFloat::parse(n, qdecimal::DfFormat::D34)
                .map(Value::DecFloat)
                .map_err(crate::value::decfloat_error),
        ),
        Expr::Neg(a) => decfloat_literal(a).map(|v| {
            v.map(|v| match v {
                Value::DecFloat(d) => Value::DecFloat(d.negated()),
                v => v,
            })
        }),
        _ => None,
    }
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
            if matches!(a, Value::Date(_) | Value::Timestamp(_)) && matches!(b, Value::Num(_)) {
                return Err(SqlError::Unsupported {
                    what: "date arithmetic without a labeled duration".into(),
                });
            }
            if let (Value::Date(x), Value::Date(y)) = (a, b) {
                if op == BinOp::Add {
                    return Err(SqlError::TypeMismatch {
                        what: "date + date".into(),
                    });
                }
                return Ok(Value::Num(date_duration(*x, *y)));
            }
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

/// `date - date` as the `yyyymmdd` duration DB2 gives.
fn date_duration(a: Date, b: Date) -> Decimal {
    let (hi, lo, neg) = if cmp_dates(a, b) == Ordering::Less {
        (b, a, true)
    } else {
        (a, b, false)
    };
    let mut years = i64::from(hi.year) - i64::from(lo.year);
    let mut months = i64::from(hi.month) - i64::from(lo.month);
    let mut days = i64::from(hi.day) - i64::from(lo.day);
    // Borrow from the months before the first date's until the days are
    // not negative.
    let (mut py, mut pm) = (hi.year, hi.month);
    while days < 0 {
        months -= 1;
        (py, pm) = if pm == 1 { (py - 1, 12) } else { (py, pm - 1) };
        days += i64::from(days_in_month(py, pm));
    }
    if months < 0 {
        years -= 1;
        months += 12;
    }
    let n = years * 10_000 + months * 100 + days;
    Decimal::new(i128::from(if neg { -n } else { n }), 0).expect("fits")
}

fn cmp_dates(a: Date, b: Date) -> Ordering {
    crate::value::cmp_date(&a, &b)
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
                return Err(SqlError::TypeMismatch {
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
                    return Err(SqlError::TypeMismatch {
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
    if v.is_null() {
        return Ok(Value::Null);
    }
    let fit = |p: u8, s: u8| -> Result<Value> {
        let d = v.as_decimal()?;
        let shape = Shape::new(p, s).map_err(overflow)?;
        Ok(Value::Num(d.fit(shape, Rounding::Truncate).map_err(
            |_| SqlError::Overflow {
                what: format!("{d} does not fit {p},{s}"),
            },
        )?))
    };
    let bytes = |n: u32, pad: bool| -> Result<Value> {
        let mut b = v.as_bytes()?;
        let n = usize::try_from(n).expect("fits");
        b.truncate(n);
        if pad {
            b.resize(n, 0);
        }
        Ok(Value::Bin(b))
    };
    let chars = |n: u32| -> String { v.as_text().chars().take(usize::try_from(n).expect("fits")).collect() };
    // A binary string as bit data is its bytes.
    let v = &match v {
        Value::Bin(b) if matches!(t.data_type, DataType::Char(_) | DataType::Varchar(_)) => {
            Value::Str(ccsid::from_ebcdic(b))
        }
        v => v.clone(),
    };
    Ok(match &t.data_type {
        DataType::DecFloat(n) => {
            let format = if *n == 16 { qdecimal::DfFormat::D16 } else { qdecimal::DfFormat::D34 };
            let d = match v {
                Value::Str(s) => qdecimal::DecFloat::parse(s, format).map_err(crate::value::decfloat_error)?,
                v => v.as_decfloat(format)?,
            };
            Value::DecFloat(d.to_format(format).map_err(crate::value::decfloat_error)?)
        }
        DataType::Binary(n) => bytes(*n, true)?,
        DataType::Varbinary(n) | DataType::Blob(n) => bytes(*n, false)?,
        DataType::Boolean => Value::Bool(v.as_bool()?),
        DataType::Clob(n) | DataType::Dbclob(n) => Value::Str(chars(*n)),
        DataType::Char(n) => {
            let s = v.as_text();
            let n = usize::try_from(*n).expect("fits");
            let mut out: String = s.chars().take(n).collect();
            while out.chars().count() < n {
                out.push(' ');
            }
            Value::Str(out)
        }
        DataType::Varchar(n) | DataType::Graphic(n) | DataType::Vargraphic(n) => Value::Str(
            v.as_text()
                .chars()
                .take(usize::try_from(*n).expect("fits"))
                .collect(),
        ),
        DataType::Decimal(p, s) | DataType::Numeric(p, s) => fit(*p, *s)?,
        DataType::SmallInt => fit(5, 0).and_then(|x| range(&x, 32_767))?,
        DataType::Integer => fit(10, 0).and_then(|x| range(&x, 2_147_483_647))?,
        DataType::BigInt => fit(19, 0).and_then(|x| range(&x, i64::MAX))?,
        DataType::Double => Value::Double(v.as_f64()?),
        DataType::Real => {
            let x = v.as_f64()?;
            #[allow(clippy::cast_possible_truncation)] // the single it rounds to
            let r = x as f32;
            if r.is_infinite() && x.is_finite() {
                return Err(SqlError::Overflow { what: format!("{} does not fit REAL", v.as_text()) });
            }
            Value::Real(r)
        }
        DataType::Date => Value::Date(v.as_date()?),
        DataType::Time => Value::Time(v.as_time()?),
        DataType::Timestamp(p) => Value::Timestamp(v.as_timestamp()?.with_precision(*p)),
    })
}

fn range(v: &Value, max: i64) -> Result<Value> {
    let n = v.as_i64()?;
    if n > max || n < -max - 1 {
        return Err(SqlError::Overflow {
            what: format!("{n} is out of range"),
        });
    }
    Ok(Value::int(n))
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
        assert_eq!(
            date_duration(
                Date::new(2026, 3, 1).unwrap(),
                Date::new(2025, 12, 31).unwrap()
            )
            .to_string(),
            "129"
        );
    }
}
